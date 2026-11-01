//! Collection: GitHub GraphQL through `gh api graphql`.
//!
//! The only step with I/O. It queries in calendar quarters (at most 92
//! days), and splits a window in half whenever a repository list or a
//! contribution list may have been cut off, down to a single day. What still
//! can't be attributed feeds the trunk, and the window is recorded in
//! `truncated_windows`. A partial snapshot is never returned.

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::Value;

use crate::Error;
use crate::color::Rgb;
use crate::date::{Date, DateRange};
use crate::snapshot::{Day, SCHEMA, Snapshot};

/// The most repositories GitHub returns per contribution list.
pub const MAX_REPOSITORIES: usize = 100;

const USER_QUERY: &str = "query($login:String!){user(login:$login){login databaseId contributionsCollection{contributionYears}}}";

const WINDOW_QUERY: &str = "query($login:String!,$from:DateTime!,$to:DateTime!){user(login:$login){contributionsCollection(from:$from,to:$to){\
contributionCalendar{weeks{contributionDays{date contributionCount}}}\
commitContributionsByRepository(maxRepositories:100){repository{nameWithOwner primaryLanguage{name color}}contributions(first:100){pageInfo{hasNextPage}nodes{occurredAt commitCount}}}\
pullRequestContributionsByRepository(maxRepositories:100){repository{nameWithOwner primaryLanguage{name color}}contributions(first:100){pageInfo{hasNextPage}nodes{occurredAt}}}\
}}}";

/// Something that answers GraphQL queries: `gh`, or fixtures in tests.
pub trait GraphQl {
    /// Fetches a public web page, for what the API doesn't expose. `None` when
    /// it can't; collection then goes on without it.
    fn fetch_page(&self, _url: &str) -> Option<String> {
        None
    }

    /// Runs `query` with string `variables` and returns the `data` object.
    ///
    /// # Errors
    ///
    /// When the query can't be sent or GitHub refuses it.
    fn query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, Error>;
}

/// GraphQL through the GitHub CLI, which brings its own authentication
/// locally and in Actions (`GH_TOKEN`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GhCli;

impl GraphQl for GhCli {
    fn fetch_page(&self, url: &str) -> Option<String> {
        let output = Command::new("curl")
            .args([
                "-sfL",
                "--max-time",
                "20",
                "-A",
                "Mozilla/5.0 (gitbonsai)",
                url,
            ])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, Error> {
        let mut last_error = String::new();
        // One retry: GitHub occasionally answers a heavy query with a 502.
        for _ in 0..2 {
            let mut command = Command::new("gh");
            command
                .args(["api", "graphql", "-f"])
                .arg(format!("query={query}"));
            for (name, value) in variables {
                command.arg("-f").arg(format!("{name}={value}"));
            }
            let output = command.output().map_err(|e| {
                Error::Collect(format!(
                    "can't run `gh` ({e}). Install the GitHub CLI from https://cli.github.com and run `gh auth login`"
                ))
            })?;
            let text = String::from_utf8_lossy(&output.stdout);
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                if let Some(errors) = value.get("errors").filter(|e| !e.is_null()) {
                    return Err(Error::Collect(format!(
                        "GitHub refused the query: {errors}"
                    )));
                }
                if output.status.success() {
                    return value
                        .get("data")
                        .cloned()
                        .ok_or_else(|| Error::Collect("GitHub's answer had no data".into()));
                }
            }
            last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        Err(Error::Collect(format!(
            "`gh api graphql` failed: {last_error}. Check `gh auth status`, or set GH_TOKEN"
        )))
    }
}

/// Collects `login`'s history through `observed_on`, reporting progress.
///
/// # Errors
///
/// When the user doesn't exist, a query fails, or GitHub's calendar stays
/// incomplete after a retry. No partial snapshot is returned.
pub fn collect(
    api: &dyn GraphQl,
    login: &str,
    observed_on: Date,
    progress: &mut dyn FnMut(&str),
) -> Result<Snapshot, Error> {
    let data = api.query(USER_QUERY, &[("login", login)])?;
    let user = data.get("user").filter(|u| !u.is_null()).ok_or_else(|| {
        Error::Collect(format!(
            "there is no GitHub user `{login}`; check the spelling"
        ))
    })?;
    let account_id = user
        .get("databaseId")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::Collect("GitHub didn't return the account id".into()))?;
    let login = user
        .get("login")
        .and_then(Value::as_str)
        .unwrap_or(login)
        .to_string();
    let first_year = user
        .pointer("/contributionsCollection/contributionYears")
        .and_then(Value::as_array)
        .and_then(|years| years.iter().filter_map(Value::as_i64).min());

    let mut acc = Accumulator::default();
    let from = first_year
        .and_then(|y| i32::try_from(y).ok())
        .and_then(|y| Date::from_ymd(y, 1, 1))
        .unwrap_or(observed_on)
        .min(observed_on);
    if first_year.is_some() {
        let quarters = quarters(from, observed_on);
        for (i, window) in quarters.iter().enumerate() {
            progress(&format!(
                "{} .. {} ({}/{})",
                window.from,
                window.to,
                i + 1,
                quarters.len()
            ));
            fetch_window(api, &login, *window, &mut acc)?;
        }
    }

    let days: Vec<Day> = acc
        .totals
        .iter()
        .filter(|(_, total)| **total > 0)
        .map(|(date, total)| Day {
            date: *date,
            total: *total,
            by_language: acc.by_language.remove(date).unwrap_or_default(),
        })
        .collect();
    progress("archived repositories and achievements");
    let archived_repos = archived(api, &login)?;
    let achievements = api
        .fetch_page(&format!("https://github.com/{login}?tab=achievements"))
        .map(|page| parse_achievements(&page, &login))
        .unwrap_or_default();
    let snapshot = Snapshot {
        schema: SCHEMA,
        login,
        account_id,
        range: DateRange {
            from,
            to: observed_on,
        },
        observed_on,
        collected_with: format!("gitbonsai {}", crate::VERSION),
        language_colors: acc.colors,
        days,
        truncated_windows: acc.truncated,
        achievements,
        archived_repos,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

const ARCHIVED_QUERY: &str = "query($login:String!){user(login:$login){repositories(first:100,ownerAffiliations:OWNER,isArchived:true){nodes{name archivedAt}}}}";

/// The archived repositories `login` owns, oldest archive first (one kintsugi
/// break each).
fn archived(api: &dyn GraphQl, login: &str) -> Result<Vec<String>, Error> {
    let data = api.query(ARCHIVED_QUERY, &[("login", login)])?;
    let mut repos: Vec<(String, String)> = data
        .pointer("/user/repositories/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let name = n.get("name")?.as_str()?.to_string();
            let at = n
                .get("archivedAt")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            Some((at, name))
        })
        .collect();
    repos.sort();
    Ok(repos.into_iter().map(|(_, name)| name).collect())
}

/// Achievement tiers from the public achievements page: each card links to
/// `?achievement=<slug>`, with an `x2`..`x4` label for higher tiers. The page
/// lists every card twice; the first wins.
pub fn parse_achievements(page: &str, login: &str) -> BTreeMap<String, u8> {
    let marker = format!("href=\"/{login}?achievement=");
    let starts: Vec<usize> = page.match_indices(&marker).map(|(i, _)| i).collect();
    let mut found = BTreeMap::new();
    for (k, &start) in starts.iter().enumerate() {
        let rest = &page[start + marker.len()..];
        let Some(slug) = rest.split(['&', '"']).next().filter(|s| !s.is_empty()) else {
            continue;
        };
        let end = starts
            .get(k + 1)
            .copied()
            .unwrap_or(page.len())
            .min(start + 4000);
        let card = &page[start..end];
        let tier = card
            .find("achievement-tier-label--")
            .and_then(|i| card[i..].find(">x").map(|j| i + j + 2))
            .and_then(|i| card[i..].chars().next())
            .and_then(|c| c.to_digit(10))
            .and_then(|d| u8::try_from(d).ok())
            .unwrap_or(1);
        found.entry(slug.to_string()).or_insert(tier);
    }
    found
}

#[derive(Default)]
struct Accumulator {
    totals: BTreeMap<Date, u32>,
    by_language: BTreeMap<Date, BTreeMap<String, u32>>,
    colors: BTreeMap<String, Option<Rgb>>,
    truncated: Vec<DateRange>,
}

/// Calendar quarters from `from` through `to`, clipped to the range.
///
/// # Panics
///
/// Never: every quarter starts on the first of a month that exists.
pub fn quarters(from: Date, to: Date) -> Vec<DateRange> {
    let mut out = Vec::new();
    let (mut year, month, _) = from.ymd();
    let mut quarter_start = (month - 1) / 3 * 3 + 1;
    loop {
        let start = Date::from_ymd(year, quarter_start, 1).expect("a quarter starts on a real day");
        let (next_year, next_month) = if quarter_start == 10 {
            (year + 1, 1)
        } else {
            (year, quarter_start + 3)
        };
        let end = Date::from_ymd(next_year, next_month, 1)
            .expect("a quarter starts on a real day")
            .add_days(-1);
        if start > to {
            break;
        }
        out.push(DateRange {
            from: start.max(from),
            to: end.min(to),
        });
        year = next_year;
        quarter_start = next_month;
    }
    out
}

fn fetch_window(
    api: &dyn GraphQl,
    login: &str,
    window: DateRange,
    acc: &mut Accumulator,
) -> Result<(), Error> {
    let from = format!("{}T00:00:00Z", window.from);
    let to = format!("{}T23:59:59Z", window.to);
    let mut data = api.query(
        WINDOW_QUERY,
        &[("login", login), ("from", &from), ("to", &to)],
    )?;
    let mut calendar = calendar_days(&data, window);
    if calendar.len() != usize::try_from(window.len_days()).unwrap_or(0) {
        // GitHub sometimes leaves out days (git-koi 1.0.2: across February 29).
        data = api.query(
            WINDOW_QUERY,
            &[("login", login), ("from", &from), ("to", &to)],
        )?;
        calendar = calendar_days(&data, window);
        if calendar.len() != usize::try_from(window.len_days()).unwrap_or(0) {
            return Err(Error::Collect(format!(
                "GitHub returned {} of {} calendar days for {} .. {}; try again later",
                calendar.len(),
                window.len_days(),
                window.from,
                window.to
            )));
        }
    }

    let collection = data
        .pointer("/user/contributionsCollection")
        .cloned()
        .unwrap_or(Value::Null);
    let commits = collection
        .get("commitContributionsByRepository")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let prs = collection
        .get("pullRequestContributionsByRepository")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let cut_off = |repos: &[Value]| {
        repos.len() >= MAX_REPOSITORIES
            || repos.iter().any(|r| {
                r.pointer("/contributions/pageInfo/hasNextPage")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
    };

    if cut_off(&commits) || cut_off(&prs) {
        if window.from < window.to {
            let half = window.len_days() / 2;
            let mid = window.from.add_days(half - 1);
            fetch_window(
                api,
                login,
                DateRange {
                    from: window.from,
                    to: mid,
                },
                acc,
            )?;
            return fetch_window(
                api,
                login,
                DateRange {
                    from: mid.add_days(1),
                    to: window.to,
                },
                acc,
            );
        }
        acc.truncated.push(window);
    }

    for (date, count) in calendar {
        acc.totals.insert(date, count);
    }
    for (repos, counted) in [(&commits, true), (&prs, false)] {
        for repo in repos {
            let Some(language) = repo
                .pointer("/repository/primaryLanguage")
                .filter(|l| !l.is_null())
            else {
                continue;
            };
            let Some(name) = language.get("name").and_then(Value::as_str) else {
                continue;
            };
            let color = language
                .get("color")
                .and_then(Value::as_str)
                .and_then(|c| c.parse::<Rgb>().ok());
            acc.colors.entry(name.to_string()).or_insert(color);
            let nodes = repo
                .pointer("/contributions/nodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for node in nodes {
                let Some(date) = node
                    .get("occurredAt")
                    .and_then(Value::as_str)
                    .and_then(|s| s.get(..10))
                    .and_then(|s| s.parse::<Date>().ok())
                else {
                    continue;
                };
                if !window.contains(date) {
                    continue;
                }
                let n = if counted {
                    node.get("commitCount")
                        .and_then(Value::as_u64)
                        .and_then(|n| u32::try_from(n).ok())
                        .unwrap_or(0)
                } else {
                    1
                };
                *acc.by_language
                    .entry(date)
                    .or_default()
                    .entry(name.to_string())
                    .or_default() += n;
            }
        }
    }
    Ok(())
}

fn calendar_days(data: &Value, window: DateRange) -> BTreeMap<Date, u32> {
    let mut days = BTreeMap::new();
    let weeks = data
        .pointer("/user/contributionsCollection/contributionCalendar/weeks")
        .and_then(Value::as_array);
    for week in weeks.into_iter().flatten() {
        let Some(list) = week.get("contributionDays").and_then(Value::as_array) else {
            continue;
        };
        for day in list {
            let date = day
                .get("date")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<Date>().ok());
            let count = day
                .get("contributionCount")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
            if let (Some(date), Some(count)) = (date, count)
                && window.contains(date)
            {
                days.insert(date, count);
            }
        }
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::RefCell;

    /// Answers with a calendar of one contribution a day, and one Rust commit
    /// per day; reports `cut` when the window is longer than `split_above` days.
    struct Fake {
        split_above: i32,
        calls: RefCell<usize>,
    }

    impl GraphQl for Fake {
        fn query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, Error> {
            *self.calls.borrow_mut() += 1;
            if query.starts_with("query($login:String!){") {
                return Ok(
                    json!({"user": {"login": "octo", "databaseId": 42, "contributionsCollection": {"contributionYears": [2026]}}}),
                );
            }
            let get = |name: &str| {
                variables
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| *v)
                    .unwrap()
            };
            let from: Date = get("from")[..10].parse().unwrap();
            let to: Date = get("to")[..10].parse().unwrap();
            let span = to.days_since(from) + 1;
            let days: Vec<Value> = (0..span)
                .map(|i| json!({"date": from.add_days(i).to_string(), "contributionCount": 1}))
                .collect();
            let nodes: Vec<Value> = (0..span)
                .map(|i| json!({"occurredAt": format!("{}T12:00:00Z", from.add_days(i)), "commitCount": 1}))
                .collect();
            Ok(json!({"user": {"contributionsCollection": {
                "contributionCalendar": {"weeks": [{"contributionDays": days}]},
                "commitContributionsByRepository": [{
                    "repository": {"nameWithOwner": "octo/r", "primaryLanguage": {"name": "Rust", "color": "#dea584"}},
                    "contributions": {"pageInfo": {"hasNextPage": span > self.split_above}, "nodes": nodes}
                }],
                "pullRequestContributionsByRepository": []
            }}}))
        }
    }

    #[test]
    fn achievement_tiers_are_read_from_the_page() {
        let page = concat!(
            r#"<a href="/octo?achievement=pull-shark&amp;tab=achievements"><img alt="Achievement: Pull Shark">"#,
            r#"<span class="achievement-tier-label achievement-tier-label--silver">x3</span></a>"#,
            r#"<a href="/octo?achievement=public-sponsor&amp;tab=achievements"><img alt="Achievement: Public Sponsor"></a>"#,
            r#"<a href="/octo?achievement=pull-shark&amp;tab=achievements"><span class="achievement-tier-label--bronze">x2</span></a>"#,
        );
        let found = parse_achievements(page, "octo");
        assert_eq!(found["pull-shark"], 3);
        assert_eq!(found["public-sponsor"], 1);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn quarters_cover_the_range_without_gaps() {
        let q = quarters("2025-02-10".parse().unwrap(), "2026-01-05".parse().unwrap());
        assert_eq!(q.first().unwrap().from.to_string(), "2025-02-10");
        assert_eq!(q.last().unwrap().to.to_string(), "2026-01-05");
        for pair in q.windows(2) {
            assert_eq!(pair[0].to.add_days(1), pair[1].from);
        }
        assert!(q.iter().all(|w| w.len_days() <= 92));
    }

    #[test]
    fn cut_off_windows_split_until_they_fit() {
        let fake = Fake {
            split_above: 20,
            calls: RefCell::new(0),
        };
        let observed: Date = "2026-03-31".parse().unwrap();
        let snapshot = collect(&fake, "octo", observed, &mut |_| {}).unwrap();
        assert_eq!(snapshot.account_id, 42);
        assert_eq!(snapshot.days.len(), 90);
        assert!(
            snapshot
                .days
                .iter()
                .all(|d| d.total == 1 && d.by_language["Rust"] == 1)
        );
        assert_eq!(snapshot.truncated_windows, []);
        assert!(*fake.calls.borrow() > 2, "the quarter was split");
    }
}
