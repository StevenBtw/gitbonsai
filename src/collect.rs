//! Collection: GitHub GraphQL through `gh api graphql`.
//!
//! The only step with I/O. It queries in calendar quarters (at most 92
//! days), and splits a window in half whenever a repository list or a
//! contribution list may have been cut off, down to a single day. What still
//! can't be attributed feeds the trunk, and the window is recorded in
//! `truncated_windows`. A partial snapshot is never returned.

use std::collections::BTreeMap;
use std::process::Command;

use serde_json::{Value, json};

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
issueContributionsByRepository(maxRepositories:100){repository{nameWithOwner primaryLanguage{name color}}contributions(first:100){pageInfo{hasNextPage}nodes{occurredAt}}}\
pullRequestReviewContributionsByRepository(maxRepositories:100){repository{nameWithOwner primaryLanguage{name color}}contributions(first:100){pageInfo{hasNextPage}nodes{occurredAt}}}\
repositoryContributions(first:100){pageInfo{hasNextPage}nodes{occurredAt repository{primaryLanguage{name color}}}}\
restrictedContributionsCount earliestRestrictedContributionDate latestRestrictedContributionDate\
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

/// The `data` of a GraphQL answer. An error that only says the user was not
/// found leaves the data to tell (a `null` user is a misspelled login, which
/// `collect` explains); any other error refuses the query.
fn data_of(value: &Value) -> Result<Value, Error> {
    if let Some(errors) = value.get("errors").filter(|e| !e.is_null()) {
        let not_found = errors.as_array().is_some_and(|list| {
            list.iter().all(|e| {
                e.get("type").and_then(Value::as_str) == Some("NOT_FOUND")
                    && e.get("path") == Some(&json!(["user"]))
            })
        });
        if !not_found {
            return Err(Error::Collect(format!(
                "GitHub refused the query: {errors}"
            )));
        }
    }
    value
        .get("data")
        .filter(|d| !d.is_null())
        .cloned()
        .ok_or_else(|| Error::Collect("GitHub's answer had no data".into()))
}

/// GraphQL through the GitHub CLI, which brings its own authentication
/// locally and in Actions (`GH_TOKEN`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GhCli;

impl GraphQl for GhCli {
    fn fetch_page(&self, url: &str) -> Option<String> {
        let mut command = Command::new("curl");
        command.args(["-sfL", "--max-time", "20", "-A", "Mozilla/5.0 (gitbonsai)"]);
        // An achievement's detail is a fragment GitHub serves only to the
        // page that asks for it (406 otherwise).
        if url.ends_with("/detail") {
            command.args(["-H", "X-Requested-With: XMLHttpRequest"]);
        }
        let output = command.arg(url).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, Error> {
        let mut last_error = String::new();
        // Two retries: GitHub occasionally answers a heavy query with a 502.
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
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
            if let Ok(value) = serde_json::from_str::<Value>(&text)
                && (output.status.success() || value.get("errors").is_some_and(|e| !e.is_null()))
            {
                return data_of(&value);
            }
            last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        Err(Error::Collect(format!(
            "`gh api graphql` failed: {last_error}. Check `gh auth status`, or set GH_TOKEN"
        )))
    }
}

/// Collects `login`'s history through `observed_on`, reporting progress.
/// With a usable `previous` snapshot ([`reusable`]) and not `full`, only the
/// days from the start of the previous calendar quarter are fetched again;
/// older days are kept as they were. Either way the dates `previous` knew
/// for achievement tiers are kept: GitHub shows only the first.
///
/// # Errors
///
/// When the user doesn't exist, a query fails, or GitHub's calendar stays
/// incomplete after a retry. No partial snapshot is returned.
pub fn collect(
    api: &dyn GraphQl,
    login: &str,
    observed_on: Date,
    previous: Option<&Snapshot>,
    full: bool,
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
    // Reuse what an earlier run of this version collected, up to the
    // previous quarter.
    let cut = refetch_from(observed_on);
    let kept = previous.filter(|p| !full && reusable(p, account_id, from, cut));
    if let Some(p) = kept {
        progress(&format!(
            "reusing {} days before {cut}",
            p.days.iter().filter(|d| d.date < cut).count()
        ));
        acc.truncated = p
            .truncated_windows
            .iter()
            .filter(|w| w.to < cut)
            .copied()
            .collect();
    }
    let fetch_from = if kept.is_some() { cut.max(from) } else { from };
    if first_year.is_some() {
        let quarters = quarters(fetch_from, observed_on);
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

    // Fresh colors win; languages only in older days keep theirs.
    for (language, color) in kept.iter().flat_map(|p| &p.language_colors) {
        acc.colors.entry(language.clone()).or_insert(*color);
    }
    let older = kept
        .into_iter()
        .flat_map(|p| p.days.iter().filter(|d| d.date < cut).cloned());
    let fresh = acc
        .totals
        .iter()
        .filter(|(_, total)| **total > 0)
        .map(|(date, total)| Day {
            date: *date,
            total: *total,
            by_language: acc.by_language.remove(date).unwrap_or_default(),
            private: acc.private.get(date).copied().unwrap_or(0),
        });
    let days: Vec<Day> = older.chain(fresh).collect();
    let from = kept.map_or(from, |p| p.range.from.min(from));
    progress("archived repositories and achievements");
    let archived = archived(api, &login)?;
    let archived_dates = archived
        .iter()
        .filter_map(|(name, at)| Some((name.clone(), (*at)?)))
        .collect();
    let archived_repos = archived.into_iter().map(|(name, _)| name).collect();
    // An earlier run of this account dated the tiers it saw.
    let seen = previous.filter(|p| p.account_id == account_id);
    // A page GitHub won't serve right now (429, 5xx) keeps what the earlier
    // run saw, instead of clearing the eggs.
    let achievements = api
        .fetch_page(&format!("https://github.com/{login}?tab=achievements"))
        .map(|page| parse_achievements(&page, &login))
        .or_else(|| seen.map(|p| p.achievements.clone()))
        .unwrap_or_default();
    let achievement_dates = achievements
        .iter()
        .map(|(slug, &tier)| {
            let before = seen.and_then(|p| p.achievement_dates.get(slug));
            // A tier-1 date never changes: only a new achievement is fetched.
            let first = before.and_then(|b| b.first().copied()).or_else(|| {
                api.fetch_page(&format!(
                    "https://github.com/users/{login}/achievements/{slug}/detail"
                ))
                .and_then(|page| parse_first_unlocked(&page))
            });
            let dates = tier_dates(tier, first, before.map(Vec::as_slice), observed_on);
            (slug.clone(), dates)
        })
        .filter(|(_, dates)| !dates.is_empty())
        .collect();
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
        achievement_dates,
        archived_dates,
        start: None,
    };
    snapshot.validate()?;
    Ok(snapshot)
}

/// The first day collection fetches again when it reuses a snapshot: the
/// start of the calendar quarter before `observed_on`'s.
fn refetch_from(observed_on: Date) -> Date {
    let (year, month, _) = observed_on.ymd();
    let start = (month - 1) / 3 * 3 + 1;
    let (year, month) = if start == 1 {
        (year - 1, 10)
    } else {
        (year, start - 3)
    };
    Date::from_ymd(year, month, 1).unwrap_or(observed_on)
}

/// Whether `previous` can stand in for the days from `from` to before `cut`:
/// this schema, this account, collected by this gitbonsai version (a release
/// that changes collection always starts clean), starting no later than the
/// account's history does now, and reaching up to `cut`.
pub fn reusable(previous: &Snapshot, account_id: u64, from: Date, cut: Date) -> bool {
    previous.schema == SCHEMA
        && previous.account_id == account_id
        && previous.collected_with == format!("gitbonsai {}", crate::VERSION)
        && previous.range.from <= from
        && previous.observed_on >= cut.add_days(-1)
}

const ARCHIVED_QUERY: &str = "query($login:String!){user(login:$login){repositories(first:100,ownerAffiliations:OWNER,isArchived:true){nodes{name archivedAt}}}}";

/// The archived repositories `login` owns with the day each was archived,
/// oldest archive first (one kintsugi break each).
fn archived(api: &dyn GraphQl, login: &str) -> Result<Vec<(String, Option<Date>)>, Error> {
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
    Ok(repos
        .into_iter()
        .map(|(at, name)| (name, at.get(..10).and_then(|d| d.parse().ok())))
        .collect())
}

/// The day an achievement was first unlocked, from its detail fragment:
/// "First unlocked" for one with tiers, "Unlocked" for one without.
pub fn parse_first_unlocked(page: &str) -> Option<Date> {
    // "Unlocked" right before its date; HTML may break the line between.
    let rest = page.match_indices("nlocked").find_map(|(i, w)| {
        let after = &page[i + w.len()..];
        after
            .trim_start()
            .starts_with("<relative-time")
            .then_some(after)
    })?;
    let at = rest.find("datetime=\"")? + "datetime=\"".len();
    rest.get(at..at + 10)?.parse().ok()
}

/// The day each tier up to `tier` was reached: tier 1 on `first` (or as an
/// earlier run recorded it), each higher tier as an earlier run recorded it,
/// else `observed_on`, the first run that saw it. Undated without tier 1.
pub fn tier_dates(
    tier: u8,
    first: Option<Date>,
    previous: Option<&[Date]>,
    observed_on: Date,
) -> Vec<Date> {
    let previous = previous.unwrap_or(&[]);
    let Some(first) = first.or_else(|| previous.first().copied()) else {
        return Vec::new();
    };
    std::iter::once(first)
        .chain((1..usize::from(tier)).map(|i| previous.get(i).copied().unwrap_or(observed_on)))
        .collect()
}

/// Achievement tiers from the public achievements page: each card links to
/// `?achievement=<slug>`, with an `x2`..`x4` label for higher tiers. The page
/// lists every card twice; the first wins. A card ends at its `</a>`.
pub fn parse_achievements(page: &str, login: &str) -> BTreeMap<String, u8> {
    let marker = format!("href=\"/{login}?achievement=");
    let starts: Vec<usize> = page.match_indices(&marker).map(|(i, _)| i).collect();
    let mut found = BTreeMap::new();
    for (k, &start) in starts.iter().enumerate() {
        let rest = &page[start + marker.len()..];
        let Some(slug) = rest.split(['&', '"']).next().filter(|s| !s.is_empty()) else {
            continue;
        };
        // A card ends at its link's end; the last card has no next marker.
        let close = page[start..].find("</a>").map_or(page.len(), |i| start + i);
        let mut end = starts
            .get(k + 1)
            .copied()
            .unwrap_or(page.len())
            .min(close)
            .min(start + 4000);
        // A page is not ASCII: never cut a character in half.
        while !page.is_char_boundary(end) {
            end -= 1;
        }
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
    /// Private contributions per day, spread from each window's count.
    private: BTreeMap<Date, u32>,
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

/// Fetches `window` as two halves.
fn split_window(
    api: &dyn GraphQl,
    login: &str,
    window: DateRange,
    acc: &mut Accumulator,
) -> Result<(), Error> {
    let mid = window.from.add_days(window.len_days() / 2 - 1);
    let first = DateRange {
        from: window.from,
        to: mid,
    };
    fetch_window(api, login, first, acc)?;
    let second = DateRange {
        from: mid.add_days(1),
        to: window.to,
    };
    fetch_window(api, login, second, acc)
}

fn fetch_window(
    api: &dyn GraphQl,
    login: &str,
    window: DateRange,
    acc: &mut Accumulator,
) -> Result<(), Error> {
    let from = format!("{}T00:00:00Z", window.from);
    let to = format!("{}T23:59:59Z", window.to);
    let mut data = match api.query(
        WINDOW_QUERY,
        &[("login", login), ("from", &from), ("to", &to)],
    ) {
        // A busy window can be too heavy for GitHub in one query.
        Err(Error::Collect(why))
            if why.contains("RESOURCE_LIMITS_EXCEEDED") && window.from < window.to =>
        {
            return split_window(api, login, window, acc);
        }
        answer => answer?,
    };
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
    let list = |name: &str| {
        collection
            .get(name)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let prs = list("pullRequestContributionsByRepository");
    let issues = list("issueContributionsByRepository");
    let reviews = list("pullRequestReviewContributionsByRepository");
    // Created repositories, reshaped like the lists above: one entry per
    // repository creation.
    let created: Vec<Value> = collection
        .pointer("/repositoryContributions/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|n| {
            json!({
                "repository": n.get("repository").cloned().unwrap_or(Value::Null),
                "contributions": {"nodes": [{"occurredAt": n.get("occurredAt").cloned().unwrap_or(Value::Null)}]}
            })
        })
        .collect();
    let more_created = collection
        .pointer("/repositoryContributions/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cut_off = |repos: &[Value]| {
        repos.len() >= MAX_REPOSITORIES
            || repos.iter().any(|r| {
                r.pointer("/contributions/pageInfo/hasNextPage")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
    };

    if cut_off(&commits) || cut_off(&prs) || cut_off(&issues) || cut_off(&reviews) || more_created {
        if window.from < window.to {
            return split_window(api, login, window, acc);
        }
        acc.truncated.push(window);
    }

    for (date, count) in calendar {
        acc.totals.insert(date, count);
    }
    // Commits count their commits; every other kind counts one.
    for (repos, counted) in [
        (&commits, true),
        (&prs, false),
        (&issues, false),
        (&reviews, false),
        (&created, false),
    ] {
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
    // GitHub gives a window's private count, not its days: spread it over
    // the days the languages leave something on.
    let count = collection
        .get("restrictedContributionsCount")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0);
    if count > 0 {
        let day_of = |name: &str| {
            collection
                .get(name)
                .and_then(Value::as_str)
                .and_then(|s| s.get(..10))
                .and_then(|s| s.parse::<Date>().ok())
        };
        let span = day_of("earliestRestrictedContributionDate")
            .zip(day_of("latestRestrictedContributionDate"));
        let leftover: BTreeMap<Date, u32> = acc
            .totals
            .range(window.from..=window.to)
            .map(|(day, total)| {
                let named: u32 = acc.by_language.get(day).map_or(0, |l| l.values().sum());
                (*day, total.saturating_sub(named))
            })
            .collect();
        acc.private.extend(spread_private(&leftover, count, span));
    }
    Ok(())
}

/// `count` private contributions over the days of `leftover` (what the
/// languages leave each day), only days inside `span` when given: in
/// proportion to the leftover, never above it, rounded by largest remainder
/// (the earlier day on a tie) so the days sum to `count`, or to all the
/// leftover when GitHub counted more.
fn spread_private(
    leftover: &BTreeMap<Date, u32>,
    count: u32,
    span: Option<(Date, Date)>,
) -> BTreeMap<Date, u32> {
    let days: Vec<(Date, u32)> = leftover
        .iter()
        .filter(|(day, left)| **left > 0 && span.is_none_or(|(a, z)| a <= **day && **day <= z))
        .map(|(day, left)| (*day, *left))
        .collect();
    let room: u64 = days.iter().map(|(_, left)| u64::from(*left)).sum();
    let n = u64::from(count).min(room);
    if n == 0 {
        return BTreeMap::new();
    }
    // Integer shares: floor(n * left / room), remainders ranked.
    let mut out: Vec<(Date, u64, u64)> = days
        .iter()
        .map(|(day, left)| {
            let exact = n * u64::from(*left);
            (*day, exact / room, exact % room)
        })
        .collect();
    let given: u64 = out.iter().map(|o| o.1).sum();
    let mut order: Vec<usize> = (0..out.len()).collect();
    order.sort_by(|&a, &b| out[b].2.cmp(&out[a].2).then(out[a].0.cmp(&out[b].0)));
    for &i in order.iter().take(usize::try_from(n - given).unwrap_or(0)) {
        out[i].1 += 1;
    }
    out.into_iter()
        .filter(|o| o.1 > 0)
        .map(|(day, k, _)| (day, u32::try_from(k).unwrap_or(u32::MAX)))
        .collect()
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

    /// Answers every window query with `answer(from, to)` as the
    /// contributions collection, counting calls.
    struct Scripted<F: Fn(Date, Date) -> Value> {
        answer: F,
        calls: RefCell<usize>,
    }

    impl<F: Fn(Date, Date) -> Value> GraphQl for Scripted<F> {
        fn query(&self, _query: &str, variables: &[(&str, &str)]) -> Result<Value, Error> {
            *self.calls.borrow_mut() += 1;
            let get = |name: &str| variables.iter().find(|(n, _)| *n == name).unwrap().1;
            let from: Date = get("from")[..10].parse().unwrap();
            let to: Date = get("to")[..10].parse().unwrap();
            Ok(json!({"user": {"contributionsCollection": (self.answer)(from, to)}}))
        }
    }

    fn calendar(from: Date, to: Date, count: u32) -> Value {
        let days: Vec<Value> = (0..=to.days_since(from))
            .map(|i| json!({"date": from.add_days(i).to_string(), "contributionCount": count}))
            .collect();
        json!({"weeks": [{"contributionDays": days}]})
    }

    fn repo(language: &str, more: bool, days: &[Date]) -> Value {
        let nodes: Vec<Value> = days
            .iter()
            .map(|d| json!({"occurredAt": format!("{d}T12:00:00Z"), "commitCount": 1}))
            .collect();
        json!({
            "repository": {"nameWithOwner": "octo/r", "primaryLanguage": {"name": language, "color": null}},
            "contributions": {"pageInfo": {"hasNextPage": more}, "nodes": nodes}
        })
    }

    fn d(s: &str) -> Date {
        s.parse().unwrap()
    }

    #[test]
    fn issues_reviews_and_new_repos_grow_their_language() {
        let day = d("2026-10-05");
        let api = Scripted {
            answer: |from, to| {
                json!({
                    "contributionCalendar": calendar(from, to, 3),
                    "commitContributionsByRepository": [],
                    "pullRequestContributionsByRepository": [],
                    "issueContributionsByRepository": [repo("Go", false, &[day])],
                    "pullRequestReviewContributionsByRepository": [repo("Go", false, &[day])],
                    "repositoryContributions": {"pageInfo": {"hasNextPage": false}, "nodes": [
                        {"occurredAt": format!("{day}T09:00:00Z"), "repository": {"primaryLanguage": {"name": "Rust", "color": null}}}
                    ]},
                    "restrictedContributionsCount": 0
                })
            },
            calls: RefCell::new(0),
        };
        let mut acc = Accumulator::default();
        fetch_window(&api, "octo", DateRange { from: day, to: day }, &mut acc).unwrap();
        let langs = &acc.by_language[&day];
        assert_eq!((langs["Go"], langs["Rust"]), (2, 1), "{langs:?}");
        assert_eq!(acc.private.get(&day).copied().unwrap_or(0), 0);
    }

    #[test]
    fn a_cut_off_review_list_splits_the_window() {
        let (a, b) = (d("2026-10-05"), d("2026-10-06"));
        let api = Scripted {
            answer: |from: Date, to: Date| {
                json!({
                    "contributionCalendar": calendar(from, to, 1),
                    "commitContributionsByRepository": [],
                    "pullRequestContributionsByRepository": [],
                    "issueContributionsByRepository": [],
                    "pullRequestReviewContributionsByRepository": [repo("Go", to > from, &[from])],
                    "restrictedContributionsCount": 0
                })
            },
            calls: RefCell::new(0),
        };
        let mut acc = Accumulator::default();
        fetch_window(&api, "octo", DateRange { from: a, to: b }, &mut acc).unwrap();
        assert_eq!(*api.calls.borrow(), 3, "the window and its two halves");
        assert_eq!(acc.by_language[&b]["Go"], 1);
    }

    #[test]
    fn a_windows_private_count_lands_on_its_days() {
        let day = d("2026-10-05");
        let api = Scripted {
            answer: |from, to| {
                json!({
                    "contributionCalendar": calendar(from, to, 3),
                    "commitContributionsByRepository": [repo("Rust", false, &[day])],
                    "pullRequestContributionsByRepository": [],
                    "restrictedContributionsCount": 2,
                    "earliestRestrictedContributionDate": "2026-10-05",
                    "latestRestrictedContributionDate": "2026-10-05"
                })
            },
            calls: RefCell::new(0),
        };
        let mut acc = Accumulator::default();
        fetch_window(&api, "octo", DateRange { from: day, to: day }, &mut acc).unwrap();
        assert_eq!(acc.private[&day], 2);
    }

    fn spread(leftover: &[(Date, u32)], count: u32, span: Option<(Date, Date)>) -> Vec<u32> {
        let map: BTreeMap<Date, u32> = leftover.iter().copied().collect();
        let out = spread_private(&map, count, span);
        leftover
            .iter()
            .map(|(d, _)| out.get(d).copied().unwrap_or(0))
            .collect()
    }

    #[test]
    fn private_is_spread_by_leftover() {
        let (a, b) = (d("2026-10-05"), d("2026-10-06"));
        assert_eq!(
            spread(&[(a, 3), (b, 1)], 2, None),
            [2, 0],
            "1.5 and 0.5: the earlier wins"
        );
        assert_eq!(spread(&[(a, 3), (b, 1)], 4, None), [3, 1]);
        assert_eq!(spread(&[(a, 2), (b, 2)], 3, None).iter().sum::<u32>(), 3);
    }

    #[test]
    fn private_never_exceeds_the_leftover() {
        let a = d("2026-10-05");
        assert_eq!(spread(&[(a, 1)], 5, None), [1]);
        assert_eq!(spread(&[(a, 0)], 5, None), [0]);
    }

    #[test]
    fn private_stays_inside_its_dates() {
        let (a, b) = (d("2026-10-05"), d("2026-10-06"));
        assert_eq!(spread(&[(a, 3), (b, 3)], 2, Some((b, b))), [0, 2]);
    }

    /// GitHub's answer for a login that does not exist.
    fn nobody() -> Value {
        json!({"data": {"user": null}, "errors": [{"type": "NOT_FOUND", "path": ["user"],
            "message": "Could not resolve to a User with the login of 'octo-typo'."}]})
    }

    #[test]
    fn a_misspelled_login_gets_the_friendly_message() {
        struct Nobody;
        impl GraphQl for Nobody {
            fn query(&self, _: &str, _: &[(&str, &str)]) -> Result<Value, Error> {
                data_of(&nobody())
            }
        }
        let err = collect(
            &Nobody,
            "octo-typo",
            d("2026-11-15"),
            None,
            false,
            &mut |_| {},
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("there is no GitHub user `octo-typo`"), "{err}");
    }

    #[test]
    fn only_not_found_errors_leave_the_data_to_tell() {
        assert_eq!(data_of(&nobody()).unwrap(), json!({"user": null}));
        let refused =
            json!({"data": null, "errors": [{"type": "RATE_LIMITED", "message": "slow down"}]});
        assert!(data_of(&refused).is_err());
        assert_eq!(
            data_of(&json!({"data": {"x": 1}})).unwrap(),
            json!({"x": 1})
        );
        assert!(data_of(&json!({})).is_err(), "no data at all");
        // Only a missing user is left to the data; anything else missing
        // refuses, so nothing is skipped silently.
        let nested = json!({"data": {"user": {}}, "errors": [{"type": "NOT_FOUND",
            "path": ["user", "contributionsCollection", "repositoryContributions"]}]});
        assert!(data_of(&nested).is_err());
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
    fn the_detail_names_the_first_unlock() {
        let page = r#"<span > Bronze and Silver unlocked </span> &middot; First unlocked <relative-time datetime="2024-12-13T20:51:00Z" month="short">"#;
        assert_eq!(
            parse_first_unlocked(page),
            Some("2024-12-13".parse().unwrap())
        );
        assert_eq!(parse_first_unlocked("<div>nothing</div>"), None);
        // An achievement without tiers says "Unlocked".
        let single = "<span> 100% unlocked </span> &middot; Unlocked
      <relative-time datetime=\"2025-12-17T05:14:29Z\">";
        assert_eq!(
            parse_first_unlocked(single),
            Some("2025-12-17".parse().unwrap())
        );
    }

    #[test]
    fn higher_tiers_are_dated_by_the_run_that_first_saw_them() {
        let d = |s: &str| -> Date { s.parse().unwrap() };
        let today = d("2026-11-16");
        let first = Some(d("2024-12-13"));
        assert_eq!(tier_dates(1, first, None, today), [d("2024-12-13")]);
        assert_eq!(
            tier_dates(3, first, None, today),
            [d("2024-12-13"), today, today],
            "seen for the first time today"
        );
        let before = [d("2024-12-13"), d("2025-06-01")];
        assert_eq!(
            tier_dates(3, first, Some(&before), today),
            [d("2024-12-13"), d("2025-06-01"), today],
            "an earlier run's dates are kept"
        );
        assert_eq!(tier_dates(2, None, Some(&before), today), before, "kept");
        assert_eq!(tier_dates(2, None, None, today), [], "undated");
    }

    #[test]
    fn the_last_card_ends_at_its_link() {
        let card = |slug: &str, label: &str| {
            format!(r#"<a href="/octo?achievement={slug}&amp;tab=achievements">{label}</a>"#)
        };
        let page = format!(
            "{}{}<div><span class=\"achievement-tier-label--gold\">x3</span></div>",
            card(
                "starstruck",
                r#"<span class="achievement-tier-label--gold">x2</span>"#
            ),
            card("quickdraw", "")
        );
        let found = parse_achievements(&page, "octo");
        assert_eq!(found["starstruck"], 2);
        assert_eq!(
            found["quickdraw"], 1,
            "the label after the link is not its own"
        );
    }

    #[test]
    fn achievements_survive_a_cut_inside_a_character() {
        let head = r#"<a href="/octo?achievement=starstruck&amp;tab=achievements">"#;
        // Pad so byte 4000 after the card's start (after `<a `) falls inside
        // an e-acute.
        let pad = "a".repeat(3 + 3999 - head.len());
        let page = format!(
            "{head}{pad}\u{e9}\u{e9}<span class=\"achievement-tier-label--gold\">x4</span></a>"
        );
        assert!(
            !page.is_char_boundary(3 + 4000),
            "the cut lands inside a character"
        );
        let found = parse_achievements(&page, "octo");
        assert_eq!(found["starstruck"], 1, "the tier label lies past the cut");
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
        let snapshot = collect(&fake, "octo", observed, None, false, &mut |_| {}).unwrap();
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

    /// Refuses windows longer than 20 days as too heavy, as GitHub does for a
    /// busy quarter.
    struct Heavy(Fake);

    impl GraphQl for Heavy {
        fn query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, Error> {
            let span = |v: &[(&str, &str)]| -> Option<i32> {
                let get = |n: &str| v.iter().find(|(k, _)| *k == n).map(|(_, v)| *v);
                let from: Date = get("from")?[..10].parse().ok()?;
                let to: Date = get("to")?[..10].parse().ok()?;
                Some(to.days_since(from) + 1)
            };
            if span(variables).is_some_and(|s| s > 20) {
                return data_of(
                    &json!({"data": null, "errors": [{"type": "RESOURCE_LIMITS_EXCEEDED", "message": "Resource limits for this query exceeded."}]}),
                );
            }
            self.0.query(query, variables)
        }
    }

    #[test]
    fn a_window_too_heavy_for_github_is_split() {
        let heavy = Heavy(Fake {
            split_above: 1000,
            calls: RefCell::new(0),
        });
        let observed: Date = "2026-03-31".parse().unwrap();
        let snapshot = collect(&heavy, "octo", observed, None, false, &mut |_| {}).unwrap();
        assert_eq!(snapshot.days.len(), 90);
        assert_eq!(snapshot.truncated_windows, []);
    }

    /// A snapshot collected through 2026-10-31 by the fake, with a marker:
    /// 2026-02-10 holds 99 contributions.
    fn earlier() -> Snapshot {
        let fake = Fake {
            split_above: 1000,
            calls: RefCell::new(0),
        };
        let mut s = collect(&fake, "octo", d("2026-10-31"), None, false, &mut |_| {}).unwrap();
        let day = s
            .days
            .iter_mut()
            .find(|x| x.date == d("2026-02-10"))
            .unwrap();
        day.total = 99;
        s
    }

    /// Window queries a collect through 2026-11-15 makes with `previous`,
    /// and whether the marker survived.
    fn again(previous: &Snapshot) -> (usize, bool) {
        let fake = Fake {
            split_above: 1000,
            calls: RefCell::new(0),
        };
        let s = collect(
            &fake,
            "octo",
            d("2026-11-15"),
            Some(previous),
            false,
            &mut |_| {},
        )
        .unwrap();
        let marker = s
            .days
            .iter()
            .any(|x| x.date == d("2026-02-10") && x.total == 99);
        // Minus the user and archived queries.
        (*fake.calls.borrow() - 2, marker)
    }

    /// `earlier()` with a starstruck x2 reached on two known days.
    fn with_eggs() -> Snapshot {
        let mut s = earlier();
        s.achievements.insert("starstruck".into(), 2);
        s.achievement_dates
            .insert("starstruck".into(), vec![d("2025-01-10"), d("2025-06-01")]);
        s
    }

    /// `Fake` with an achievements page of one starstruck x2, counting the
    /// detail pages asked for.
    struct Paged {
        fake: Fake,
        details: RefCell<usize>,
    }

    impl GraphQl for Paged {
        fn fetch_page(&self, url: &str) -> Option<String> {
            if url.ends_with("/detail") {
                *self.details.borrow_mut() += 1;
                return None;
            }
            Some(
                r#"<a href="/octo?achievement=starstruck&amp;tab=achievements"><span class="achievement-tier-label--gold">x2</span></a>"#
                    .into(),
            )
        }

        fn query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, Error> {
            self.fake.query(query, variables)
        }
    }

    #[test]
    fn a_known_first_unlock_is_not_fetched_again() {
        let api = Paged {
            fake: Fake {
                split_above: 1000,
                calls: RefCell::new(0),
            },
            details: RefCell::new(0),
        };
        let previous = with_eggs();
        let s = collect(
            &api,
            "octo",
            d("2026-11-15"),
            Some(&previous),
            false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(*api.details.borrow(), 0, "the date was known");
        assert_eq!(s.achievement_dates, previous.achievement_dates);
    }

    #[test]
    fn an_unreachable_achievements_page_keeps_the_eggs() {
        // `Fake` can't fetch pages, like GitHub answering 429.
        let fake = Fake {
            split_above: 1000,
            calls: RefCell::new(0),
        };
        let previous = with_eggs();
        let s = collect(
            &fake,
            "octo",
            d("2026-11-15"),
            Some(&previous),
            false,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(s.achievements, previous.achievements);
        assert_eq!(s.achievement_dates, previous.achievement_dates);
    }

    #[test]
    fn a_full_collect_keeps_the_tier_dates() {
        let fake = Fake {
            split_above: 1000,
            calls: RefCell::new(0),
        };
        let previous = with_eggs();
        let s = collect(
            &fake,
            "octo",
            d("2026-11-15"),
            Some(&previous),
            true,
            &mut |_| {},
        )
        .unwrap();
        // Minus the user and archived queries: every quarter of 2026 again.
        assert_eq!(*fake.calls.borrow() - 2, 4, "full fetches every day again");
        assert_eq!(s.achievement_dates, previous.achievement_dates);
    }

    #[test]
    fn a_previous_snapshot_is_refetched_from_the_quarter_before() {
        let (windows, marker) = again(&earlier());
        assert_eq!(windows, 2, "2026-07..09 and 2026-10..11-15 only");
        assert!(marker, "older days are kept as they were");
    }

    #[test]
    fn an_unusable_snapshot_means_a_full_collect() {
        let full = 4; // the four quarters of 2026
        let mut other = earlier();
        other.account_id = 7;
        assert_eq!(again(&other), (full, false), "another account");
        let mut old = earlier();
        old.schema = crate::snapshot::LEGACY_SCHEMA;
        assert_eq!(again(&old), (full, false), "schema 1");
        let mut version = earlier();
        version.collected_with = "gitbonsai 0.0.1".into();
        assert_eq!(again(&version), (full, false), "another version");
        let mut stale = earlier();
        stale.days.retain(|x| x.date < d("2026-03-01"));
        stale.observed_on = d("2026-03-31");
        stale.range.to = stale.observed_on;
        assert_eq!(again(&stale), (full, false), "it ends before the refetch");
        // The account's history now starts earlier than the snapshot's (an
        // imported repository, a linked email): fetch it all again.
        let mut late = earlier();
        late.days.retain(|x| x.date >= d("2026-02-01"));
        late.range.from = d("2026-02-01");
        late.days[0].total = 99;
        let (windows, _) = again(&late);
        assert_eq!(windows, full, "history before the snapshot's start");
    }
}
