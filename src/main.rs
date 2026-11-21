//! gitbonsai CLI: collect a snapshot, render the tree, explain it, and the
//! design tools (timelapse, gallery, fixtures).

#![deny(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use gitbonsai::config::Legend;
use gitbonsai::date::Date;
use gitbonsai::{
    Bonsai, Config, Error, Snapshot, Style, Theme, collect, explain, fixtures, html, output, render,
};

/// Grow a bonsai from your GitHub activity.
///
/// Exit codes: 0 success, 1 runtime error, 2 invalid usage or config.
#[derive(Parser)]
#[command(name = "gitbonsai", author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Suppress progress messages on stderr
    #[arg(long, short, global = true)]
    quiet: bool,
}

/// Options that shape a tree, shared by the rendering commands.
#[derive(clap::Args, Clone)]
struct TreeArgs {
    /// Config file [default: bonsai.toml, all defaults when absent]
    #[arg(long)]
    config: Option<PathBuf>,

    /// Override [tree] style: chokkan, moyogi, shakan, fukinagashi, kengai
    #[arg(long)]
    style: Option<String>,

    /// Show the tree as it was on this day (YYYY-MM-DD)
    #[arg(long)]
    as_of: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Collect a snapshot of an account's public activity through `gh`
    Collect {
        /// GitHub login
        login: String,

        /// Write here instead of stdout; replaced only after success. A
        /// snapshot already there from this version is reused: only recent
        /// months are fetched again
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Fetch the whole history, even with a reusable snapshot at --output
        #[arg(long)]
        full: bool,
    },

    /// Render light and dark SVGs, a PNG and preview.html
    Render {
        /// Snapshot file (or `fixture:<name>`)
        snapshot: String,

        #[command(flatten)]
        tree: TreeArgs,

        /// Output folder
        #[arg(long, default_value = "bonsai")]
        output_dir: PathBuf,

        /// Override [render] size: display width in px
        #[arg(long)]
        size: Option<u32>,

        /// Leave every animation rule out
        #[arg(long)]
        no_animate: bool,

        /// Override [render] legend: off, left or right
        #[arg(long)]
        legend: Option<Legend>,
    },

    /// Print every derived number behind the tree
    Explain {
        /// Snapshot file (or `fixture:<name>`)
        snapshot: String,

        #[command(flatten)]
        tree: TreeArgs,
    },

    /// Render numbered frames over the account's life, with a scrubber page
    Timelapse {
        /// Snapshot file (or `fixture:<name>`)
        snapshot: String,

        #[command(flatten)]
        tree: TreeArgs,

        /// Interval between frames: `4w`, `10d`, or `1m` and `1q` for calendar
        /// months and quarters
        #[arg(long, default_value = "4w")]
        every: String,

        /// Output folder
        #[arg(long, default_value = "frames")]
        output_dir: PathBuf,
    },

    /// Render every fixture in every style into one page, for design review
    Gallery {
        /// Output folder
        #[arg(long, default_value = "gallery")]
        output_dir: PathBuf,

        /// Real snapshots to show alongside the fixtures
        #[arg(long = "snapshot")]
        snapshots: Vec<PathBuf>,

        /// Config file [default: bonsai.toml, all defaults when absent]
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Render several trees side by side at one scale: row-light.svg and row-dark.svg
    Row {
        /// Trees as `Name=snapshot` or `Name=snapshot@YYYY-MM-DD`; a snapshot is a file or `fixture:<name>`
        #[arg(required = true)]
        trees: Vec<String>,

        /// Config file [default: bonsai.toml, all defaults when absent]
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output folder
        #[arg(long, default_value = "bonsai")]
        output_dir: PathBuf,

        /// Frame trees under a year old no wider than they are tall, so they take less room
        #[arg(long)]
        tight: bool,
    },

    /// Write a synthetic snapshot, or list them
    Fixture {
        /// Fixture name; omit to list them all
        name: Option<String>,

        /// Write here instead of stdout
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            let code = err.downcast_ref::<Error>().map_or(1, Error::exit_code);
            ExitCode::from(code)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    let quiet = cli.quiet;
    let status = |msg: &str| {
        if !quiet {
            eprintln!("{msg}");
        }
    };
    match cli.command {
        Command::Collect {
            login,
            output,
            full,
        } => {
            let observed_on = Date::today_utc().add_days(-1);
            status(&format!("collecting @{login} through {observed_on}"));
            let previous = previous_snapshot(output.as_deref(), &mut |line| status(line));
            let snapshot = collect::collect(
                &collect::GhCli,
                &login,
                observed_on,
                previous.as_ref(),
                full,
                &mut |line| status(line),
            )?;
            let json = snapshot.to_json();
            match output {
                Some(path) => {
                    output::write_atomic(&path, json.as_bytes())?;
                    status(&format!(
                        "wrote {} ({} active days)",
                        path.display(),
                        snapshot.days.len()
                    ));
                }
                None => print!("{json}"),
            }
        }
        Command::Render {
            snapshot,
            tree,
            output_dir,
            size,
            no_animate,
            legend,
        } => {
            let snapshot = load_snapshot(&snapshot)?;
            let mut config = load_config(&tree, &snapshot, &status)?;
            if let Some(size) = size {
                config.render.size = size;
                config.validate()?;
            }
            if no_animate {
                config.render.animate = false;
            }
            if let Some(legend) = legend {
                config.render.legend = legend;
            }
            let bonsai = Bonsai::grow(&snapshot, &config, as_of(&tree)?);
            render_outputs(&bonsai, &config, &output_dir)?;
            status(&format!("wrote {}", output_dir.display()));
        }
        Command::Explain { snapshot, tree } => {
            let snapshot = load_snapshot(&snapshot)?;
            let config = load_config(&tree, &snapshot, &status)?;
            let bonsai = Bonsai::grow(&snapshot, &config, as_of(&tree)?);
            print!("{}", explain::report(&bonsai));
        }
        Command::Timelapse {
            snapshot,
            tree,
            every,
            output_dir,
        } => {
            let snapshot = load_snapshot(&snapshot)?;
            let config = load_config(&tree, &snapshot, &status)?;
            let step = parse_interval(&every)?;
            let end = as_of(&tree)?
                .unwrap_or(snapshot.observed_on)
                .min(snapshot.observed_on);
            let start = snapshot.first_active().unwrap_or(end);
            let mut days = vec![start];
            while let Some(&day) = days.last().filter(|&&d| d < end) {
                days.push(step.after(day).min(end));
            }
            // Two passes, one tree in memory at a time: every frame's box for
            // the shared camera, then each frame grown again and written.
            let grow = |day: Date| Bonsai::grow(&snapshot, &config, Some(day));
            let boxes: Vec<_> = days
                .iter()
                .map(|&day| render::frame(&grow(day), &config))
                .collect();
            let mut frames = Vec::new();
            for (&day, camera) in days.iter().zip(render::cameras(&boxes)) {
                let bonsai = &grow(day);
                let base = format!("{:04}", frames.len() + 1);
                for theme in Theme::ALL {
                    let path = output_dir.join(format!("{base}-{}.svg", theme.name()));
                    let svg = render::svg_in(bonsai, &config, theme, Some(camera));
                    output::write_atomic(&path, svg.as_bytes())?;
                }
                frames.push((
                    base,
                    format!("{day}  ·  week {}", bonsai.timeline.age_weeks()),
                ));
            }
            let page = html::timelapse(&format!("@{} timelapse", snapshot.login), &frames);
            output::write_atomic(&output_dir.join("index.html"), page.as_bytes())?;
            status(&format!(
                "wrote {} frames to {}",
                frames.len(),
                output_dir.display()
            ));
        }
        Command::Gallery {
            output_dir,
            snapshots,
            config,
        } => {
            let config = config_file(config.as_deref(), &mut |s| status(s))?;
            gallery(&output_dir, &snapshots, &config)?;
            status(&format!(
                "wrote {}",
                output_dir.join("index.html").display()
            ));
        }
        Command::Row {
            trees,
            config,
            output_dir,
            tight,
        } => {
            let mut config = config_file(config.as_deref(), &mut |s| status(s))?;
            let mut grown = Vec::new();
            for text in &trees {
                let (name, source, day) = gitbonsai::row::spec(text)?;
                let snapshot = load_snapshot(&source)?;
                let shown = day.map_or_else(|| snapshot.clone(), |d| snapshot.truncated(d));
                for warning in config.warnings(&shown) {
                    status(&format!("warning: {name}: {warning}"));
                }
                grown.push((name, Bonsai::grow(&snapshot, &config, day)));
            }
            // Side by side, trees lit from both sides read as a mistake:
            // the row has one light, from the right, where a tree grows.
            config.render.light = Some(1.0);
            for theme in Theme::ALL {
                let svgs: Vec<(String, String)> = grown
                    .iter()
                    .map(|(name, b)| {
                        let svg = if tight && b.timeline.age_weeks() < 52 {
                            let (x, y, w, h) = render::frame(b, &config);
                            let [x, y, w, h] = gitbonsai::row::tight([x, y, w, h]);
                            render::svg_in(b, &config, theme, Some((x, y, w, h)))
                        } else {
                            render::svg(b, &config, theme)
                        };
                        (name.clone(), svg)
                    })
                    .collect();
                output::write_atomic(
                    &output_dir.join(format!("row-{}.svg", theme.name())),
                    gitbonsai::row::row(&svgs, theme).as_bytes(),
                )?;
            }
            status(&format!("wrote {}", output_dir.display()));
        }
        Command::Fixture { name, output } => match name {
            None => {
                for f in fixtures::all() {
                    println!("{:<10} {}", f.name, f.about);
                }
            }
            Some(name) => {
                let fixture = fixtures::by_name(&name).ok_or_else(|| unknown_fixture(&name))?;
                let json = fixture.snapshot.to_json();
                match output {
                    Some(path) => output::write_atomic(&path, json.as_bytes())?,
                    None => print!("{json}"),
                }
            }
        },
    }
    Ok(())
}

fn unknown_fixture(name: &str) -> Error {
    Error::Config(format!(
        "no fixture `{name}`; use one of {}",
        fixtures::names().join(", ")
    ))
}

/// A snapshot file, or `fixture:<name>`.
fn load_snapshot(source: &str) -> Result<Snapshot> {
    if let Some(name) = source.strip_prefix("fixture:") {
        return Ok(fixtures::by_name(name)
            .ok_or_else(|| unknown_fixture(name))?
            .snapshot);
    }
    Ok(Snapshot::load(Path::new(source))?)
}

/// The config at `path`, or `bonsai.toml` when none is given. A missing
/// `bonsai.toml` means all defaults; a missing file named on the command line
/// does too, with a warning, since it is likely a typo.
fn config_file(path: Option<&Path>, status: &mut dyn FnMut(&str)) -> Result<Config> {
    match path {
        Some(path) if !path.exists() => {
            status(&format!(
                "warning: {} not found; using the defaults",
                path.display()
            ));
            Ok(Config::default())
        }
        Some(path) => Ok(Config::load(path)?),
        None => Ok(Config::load(Path::new("bonsai.toml"))?),
    }
}

fn load_config(args: &TreeArgs, snapshot: &Snapshot, status: &dyn Fn(&str)) -> Result<Config> {
    let mut config = config_file(args.config.as_deref(), &mut |s| status(s))?;
    if let Some(style) = &args.style {
        config.tree.style = Some(style.parse::<Style>()?);
    }
    for warning in config.warnings(snapshot) {
        status(&format!("warning: {warning}"));
    }
    Ok(config)
}

fn as_of(args: &TreeArgs) -> Result<Option<Date>> {
    args.as_of
        .as_deref()
        .map(|s| s.parse::<Date>().map_err(|e| Error::Config(e.to_string())))
        .transpose()
        .map_err(Into::into)
}

/// The snapshot at `output` from an earlier run: `None` for stdout, a first
/// run, or a file that isn't a valid snapshot (reported, then replaced).
fn previous_snapshot(output: Option<&Path>, status: &mut dyn FnMut(&str)) -> Option<Snapshot> {
    let path = output.filter(|p| p.exists())?;
    match Snapshot::load(path) {
        Ok(snapshot) => Some(snapshot),
        Err(e) => {
            status(&format!("not reusing {}: {e}", path.display()));
            None
        }
    }
}

/// A timelapse step: a number of days, or of calendar months.
#[derive(Debug, Clone, Copy)]
enum Interval {
    Days(i32),
    Months(i32),
}

impl Interval {
    /// The next frame's day after `day`: days later, or the first day of the
    /// next month that starts a whole step (a quarter starts in January,
    /// April, July or October).
    fn after(self, day: Date) -> Date {
        match self {
            Self::Days(n) => day.add_days(n),
            Self::Months(n) => {
                let (y, m, _) = day.ymd();
                let i = (y * 12 + m.cast_signed() - 1) / n * n + n;
                Date::from_ymd(i.div_euclid(12), (i.rem_euclid(12) + 1).cast_unsigned(), 1)
                    .unwrap_or(day)
            }
        }
    }
}

fn parse_interval(text: &str) -> Result<Interval> {
    let invalid = || {
        Error::Config(format!(
            "`{text}` is not an interval; use for example 4w, 10d, 1m or 1q"
        ))
    };
    // The unit is the last character, which need not be one byte.
    let cut = text.char_indices().last().map_or(0, |(i, _)| i);
    let (number, unit) = text.split_at(cut);
    let n: i32 = number.parse().map_err(|_| invalid())?;
    if n < 1 {
        return Err(invalid().into());
    }
    Ok(match unit {
        // Steps beyond ten thousand years are no step, and would wrap.
        "w" if n <= 520_000 => Interval::Days(n * 7),
        "d" if n <= 3_650_000 => Interval::Days(n),
        // Months beyond ten thousand years are no step.
        "m" if n <= 120_000 => Interval::Months(n),
        "q" if n <= 40_000 => Interval::Months(n * 3),
        _ => return Err(invalid().into()),
    })
}

fn render_outputs(bonsai: &Bonsai, config: &Config, dir: &Path) -> Result<()> {
    for theme in Theme::ALL {
        let svg = render::svg(bonsai, config, theme);
        output::write_atomic(
            &dir.join(format!("bonsai-{}.svg", theme.name())),
            svg.as_bytes(),
        )?;
    }
    let png = cfg!(feature = "png");
    #[cfg(feature = "png")]
    {
        // Without its font stack resvg draws no text, so the PNG goes without the legend.
        let mut still = config.clone();
        still.render.animate = false;
        still.render.legend = Legend::Off;
        let svg = render::svg(bonsai, &still, Theme::Light);
        let bytes = gitbonsai::raster::png(&svg, config.render.size)?;
        output::write_atomic(&dir.join("bonsai.png"), &bytes)?;
    }
    output::write_atomic(
        &dir.join("preview.html"),
        html::preview(&bonsai.login, png).as_bytes(),
    )?;
    Ok(())
}

/// Ages shown in each gallery growth row, in weeks.
const STAGES: [(u32, &str); 7] = [
    (4, "4 weeks"),
    (26, "6 months"),
    (52, "1 year"),
    (104, "2 years"),
    (260, "5 years"),
    (520, "10 years"),
    (u32::MAX, "now"),
];

fn gallery(dir: &Path, snapshots: &[PathBuf], config: &Config) -> Result<()> {
    let mut accounts: Vec<(String, String, Snapshot)> = Vec::new();
    for path in snapshots {
        let snapshot = Snapshot::load(path)?;
        accounts.push((
            format!("@{}", snapshot.login),
            format!("Collected from GitHub ({}).", path.display()),
            snapshot,
        ));
    }
    for f in fixtures::all() {
        accounts.push((f.name.to_string(), f.about.to_string(), f.snapshot));
    }

    let mut sections = Vec::new();
    for (title, about, snapshot) in &accounts {
        let slug: String = title
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let mut styles = Vec::new();
        for style in Style::ALL {
            let mut config = config.clone();
            config.tree.style = Some(style);
            let bonsai = Bonsai::grow(snapshot, &config, None);
            let base = format!("{slug}-{style}");
            write_pair(dir, &base, &bonsai, &config)?;
            styles.push(html::Tile {
                base,
                caption: style.to_string(),
                detail: summary(&bonsai),
            });
        }
        let mut stages = Vec::new();
        if let Some(first) = snapshot.first_active() {
            let first_monday = Date::monday_of(first.week());
            for (weeks, label) in STAGES {
                let day = if weeks == u32::MAX {
                    snapshot.observed_on
                } else {
                    let day =
                        first_monday.add_days(i32::try_from(weeks * 7).unwrap_or(i32::MAX) + 6);
                    if day >= snapshot.observed_on {
                        continue;
                    }
                    day
                };
                let bonsai = Bonsai::grow(snapshot, config, Some(day));
                let base = if weeks == u32::MAX {
                    format!("{slug}-now")
                } else {
                    format!("{slug}-age{weeks}")
                };
                write_pair(dir, &base, &bonsai, config)?;
                stages.push(html::Tile {
                    base,
                    caption: format!("{label} · {day}"),
                    detail: summary(&bonsai),
                });
            }
        }
        let mut rows = vec![("Styles".to_string(), styles)];
        if stages.len() > 1 {
            let style = config
                .tree
                .style
                .map_or_else(|| "style from history".to_string(), |s| s.to_string());
            rows.push((format!("Growth, {style}"), stages));
        }
        sections.push(html::Section {
            title: title.clone(),
            about: about.clone(),
            rows,
        });
    }
    output::write_atomic(&dir.join("index.html"), html::gallery(&sections).as_bytes())?;
    Ok(())
}

fn write_pair(dir: &Path, base: &str, bonsai: &Bonsai, config: &Config) -> Result<()> {
    for theme in Theme::ALL {
        let svg = render::svg(bonsai, config, theme);
        output::write_atomic(
            &dir.join(format!("{base}-{}.svg", theme.name())),
            svg.as_bytes(),
        )?;
    }
    Ok(())
}

fn summary(b: &Bonsai) -> String {
    format!(
        "{}w · {:.0} lifetime · {} clouds · L{} · pot {} · {:.0}/wk",
        b.timeline.age_weeks(),
        b.timeline.capped_lifetime(),
        b.skeleton.clouds.len(),
        b.extras.level,
        b.extras.pot_tier,
        b.window.total
    )
}

#[cfg(test)]
mod tests {
    use super::{Date, config_file, parse_interval, previous_snapshot};

    #[test]
    fn an_explicit_missing_config_warns() {
        let mut said = Vec::new();
        let missing = std::path::Path::new("no-such-dir/nope.toml");
        let config = config_file(Some(missing), &mut |s| said.push(s.to_string())).unwrap();
        assert_eq!(config, gitbonsai::Config::default());
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("nope.toml"), "{said:?}");
        // The implicit bonsai.toml may be absent without a word (the tests
        // run in the crate root, which has none).
        assert!(!std::path::Path::new("bonsai.toml").exists());
        said.clear();
        config_file(None, &mut |s| said.push(s.to_string())).unwrap();
        assert!(said.is_empty(), "{said:?}");
    }

    #[test]
    fn a_missing_or_corrupt_previous_snapshot_is_ignored() {
        let dir = std::env::temp_dir().join(format!("gitbonsai-prev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut said = Vec::new();
        let missing = dir.join("none.json");
        assert!(previous_snapshot(Some(&missing), &mut |s| said.push(s.to_string())).is_none());
        assert!(said.is_empty(), "a first run says nothing");
        let corrupt = dir.join("bad.json");
        std::fs::write(&corrupt, "{not json").unwrap();
        assert!(previous_snapshot(Some(&corrupt), &mut |s| said.push(s.to_string())).is_none());
        assert_eq!(said.len(), 1, "a corrupt file is reported: {said:?}");
        let good = dir.join("good.json");
        let snapshot = gitbonsai::fixtures::by_name("mono").unwrap().snapshot;
        std::fs::write(&good, snapshot.to_json()).unwrap();
        assert!(previous_snapshot(Some(&good), &mut |_| {}).is_some());
        assert!(previous_snapshot(None, &mut |_| {}).is_none(), "stdout");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_interval_with_a_multibyte_unit_is_an_error() {
        // A step too long to take would wrap the day count and never end.
        assert!(parse_interval("2147480000d").is_err());
        assert!(parse_interval("300000000w").is_err());
        assert!(parse_interval("4\u{e9}").is_err());
        assert!(parse_interval("\u{e9}").is_err());
        let day: Date = "2024-05-20".parse().unwrap();
        let next = |text: &str| parse_interval(text).unwrap().after(day).to_string();
        assert_eq!(next("4w"), "2024-06-17");
        assert_eq!(next("10d"), "2024-05-30");
        assert!(parse_interval("400000000w").is_err(), "too many weeks");
        assert!(parse_interval("613566757w").is_err(), "no wrap-around");
    }

    #[test]
    fn months_and_quarters_step_to_calendar_starts() {
        let next = |text: &str, day: &str| {
            parse_interval(text)
                .unwrap()
                .after(day.parse().unwrap())
                .to_string()
        };
        assert_eq!(next("1m", "2024-05-20"), "2024-06-01");
        assert_eq!(next("1m", "2024-12-01"), "2025-01-01");
        assert_eq!(next("1q", "2024-05-20"), "2024-07-01");
        assert_eq!(next("1q", "2024-10-01"), "2025-01-01");
        assert_eq!(next("2q", "2024-02-01"), "2024-07-01");
        assert!(parse_interval("0m").is_err());
        assert!(parse_interval("2000000000q").is_err(), "no wrap-around");
    }
}
