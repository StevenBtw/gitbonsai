//! Nebari, moss, blossoms, the pot and drip irrigation.

use crate::date::{Date, WeekNo};
use crate::math;
use crate::timeline::Timeline;

/// Capped lifetime thresholds of blossom levels 1 through 11. Level 7 is
/// over 9000.
pub const LEVELS: [f64; 11] = [
    10.0, 100.0, 500.0, 1_000.0, 2_500.0, 5_000.0, 9_001.0, 25_000.0, 50_000.0, 100_000.0,
    300_000.0,
];

/// Longest-streak thresholds, in weeks, of pot tiers 1 through 6.
pub const POT_TIERS: [u32; 6] = [4, 13, 26, 52, 104, 208];

/// Weeks over which moss recedes after a streak ends.
pub const MOSS_RECEDE_WEEKS: f64 = 8.0;

/// Weeks of streak for full moss.
pub const MOSS_FULL_WEEKS: f64 = 26.0;

/// The extras, as of `observed_on`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Extras {
    /// Nebari spread factor `n`: 0 new, 1 at 20 years.
    pub nebari: f64,
    /// Surface roots, 3 to 7.
    pub roots: u32,
    /// The weekly streak running now, in weeks.
    pub current_streak: u32,
    /// The longest weekly streak ever, in weeks.
    pub longest_streak: u32,
    /// Moss coverage of the soil, 0 to 1.
    pub moss: f64,
    /// Pot tier, 0 to 6. Never goes down, because the longest streak can't.
    pub pot_tier: u8,
    /// The highest blossom level reached, 0 to 11.
    pub level: u8,
    /// The level blooming now: crossed in the 7 days ending on `observed_on`.
    pub bloom: Option<u8>,
    /// Level 11 reached: drip irrigation, permanently.
    pub drip: bool,
    /// Weekend work in the window: 1 from 3/7 of the contributions on
    /// Saturday and Sunday, 2 from 4/7. An evenly spread week gives 2/7.
    pub weekend: u8,
}

impl Extras {
    /// The extras of `timeline` at age `age_weeks`.
    pub fn new(timeline: &Timeline, age_weeks: u32) -> Self {
        let n = math::ln(1.0 + f64::from(age_weeks) / 52.0) / math::ln(21.0);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "in 0..=4"
        )]
        let roots = 3 + (4.0 * n.min(1.0)).floor() as u32;

        let streaks = Streaks::new(timeline);
        let tier = POT_TIERS.iter().filter(|&&t| streaks.longest >= t).count();

        let (level, bloom) = blossoms(timeline);

        #[allow(clippy::cast_possible_truncation, reason = "at most 6")]
        let pot_tier = tier as u8;
        Self {
            nebari: n,
            roots,
            current_streak: streaks.current,
            longest_streak: streaks.longest,
            moss: streaks.moss,
            pot_tier,
            level,
            bloom,
            drip: level >= 11,
            weekend: weekend(timeline),
        }
    }
}

/// Measures how much of the window's work falls on the weekend: 0 below
/// 3/7, 1 from 3/7 and 2 from 4/7. Under 10 contributions you get 0.
// Weekends by UTC date, the only date the snapshot keeps.
fn weekend(timeline: &Timeline) -> u8 {
    let (mut all, mut end) = (0.0, 0.0);
    // The first day carries the strays folded into it: not that day's work.
    for day in timeline
        .window_days()
        .filter(|d| Some(d.date) != timeline.first_active)
    {
        all += day.total;
        if day.date.weekday() >= 5 {
            end += day.total;
        }
    }
    // Capped totals are fractions: a hair of slack keeps 3/7 and 4/7 exact.
    let slack = 1e-9 * all;
    if all < crate::timeline::SUSTAINED {
        0
    } else if 7.0 * end + slack >= 4.0 * all {
        2
    } else {
        u8::from(7.0 * end + slack >= 3.0 * all)
    }
}

struct Streaks {
    current: u32,
    longest: u32,
    moss: f64,
}

impl Streaks {
    /// Weekly streaks: runs of weeks with at least one active day. The
    /// in-progress week can extend a streak but never ends one.
    fn new(timeline: &Timeline) -> Self {
        let weeks = &timeline.weeks;
        let Some(last) = weeks.last() else {
            return Self {
                current: 0,
                longest: 0,
                moss: 0.0,
            };
        };
        let last_complete: Option<WeekNo> = timeline.last_complete_week().map(|w| w.no);

        // Runs as (first week, last week), over active weeks.
        let mut runs: Vec<(WeekNo, WeekNo)> = Vec::new();
        for week in weeks.iter().filter(|w| w.active_days > 0) {
            match runs.last_mut() {
                Some(run) if run.1 + 1 == week.no => run.1 = week.no,
                _ => runs.push((week.no, week.no)),
            }
        }
        #[allow(clippy::cast_sign_loss, reason = "a run ends at or after it starts")]
        let len = |run: &(WeekNo, WeekNo)| (run.1 - run.0 + 1) as u32;
        let longest = runs.iter().map(len).max().unwrap_or(0);

        // The run still going: it reaches the in-progress week, or the last
        // complete week (the in-progress week may simply not have started).
        let running = runs
            .last()
            .filter(|run| run.1 == last.no || Some(run.1) == last_complete);
        let current = running.map_or(0, len);
        let coverage = |weeks: u32| (f64::from(weeks) / MOSS_FULL_WEEKS).min(1.0);

        let mut moss = coverage(current);
        if let Some(last_complete) = last_complete {
            for run in &runs {
                if Some(run) == running {
                    continue;
                }
                let since = f64::from(last_complete - run.1);
                let receding = coverage(len(run)) * (1.0 - since / MOSS_RECEDE_WEEKS).max(0.0);
                moss = moss.max(receding);
            }
        }
        Self {
            current,
            longest,
            moss,
        }
    }
}

/// The highest level reached, and the level blooming now.
fn blossoms(timeline: &Timeline) -> (u8, Option<u8>) {
    let bloom_from: Date = timeline.observed_on.add_days(-6);
    let mut cumulative = 0.0;
    let mut level = 0u8;
    let mut bloom = None;
    for day in &timeline.days {
        cumulative += day.total;
        while usize::from(level) < LEVELS.len() && cumulative >= LEVELS[usize::from(level)] {
            level += 1;
            if day.date >= bloom_from {
                bloom = Some(level);
            }
        }
    }
    (level, bloom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nebari_follows_the_documented_table() {
        let roots = |w: u32| Extras::new(&empty(), w).roots;
        assert_eq!(roots(0), 3);
        assert_eq!(roots(104), 4);
        assert_eq!(roots(260), 5);
        assert_eq!(roots(520), 6);
        assert_eq!(roots(1040), 7);
    }

    /// 13 weeks to 2026-10-31 (a Saturday) with `week` contributions on
    /// Monday to Friday and `end` on Saturday and Sunday, every week, after
    /// a month of daily work in January, so the window holds no first day.
    fn weeks(week: u32, end: u32) -> Timeline {
        let observed: Date = "2026-10-31".parse().unwrap();
        let january: Date = "2026-01-05".parse().unwrap();
        let days: Vec<(Date, u32, &[(&str, u32)])> = (0..28)
            .map(|i| (january.add_days(i), 3, &[][..]))
            .chain(
                (0..91)
                    .map(|i| observed.add_days(-i))
                    .rev()
                    .map(|d| (d, if d.weekday() >= 5 { end } else { week }, &[][..])),
            )
            .filter(|d| d.1 > 0)
            .collect();
        Timeline::new(&crate::growth::testkit::snap(&days, observed))
    }

    #[test]
    fn weekend_work_reaches_its_tiers() {
        let weekend = |week, end| Extras::new(&weeks(week, end), 52).weekend;
        // Evenly spread is 2/7 of the work on the weekend: nothing.
        assert_eq!(weekend(3, 3), 0);
        // 3/7 and up: x1.
        assert_eq!(weekend(4, 8), 1);
        // 4/7 and up: x2.
        assert_eq!(weekend(3, 10), 2);
        assert_eq!(weekend(0, 5), 2);
        // Too little work to say.
        assert_eq!(Extras::new(&empty(), 52).weekend, 0);
    }

    fn empty() -> Timeline {
        Timeline {
            observed_on: "2026-10-31".parse().unwrap(),
            first_active: None,
            days: Vec::new(),
            weeks: Vec::new(),
        }
    }
}
