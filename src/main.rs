//! gitbonsai CLI: collect a snapshot, render the tree, explain it, and the
//! design tools (timelapse, gallery, fixtures).

#![deny(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    /// Config file; a missing file means all defaults
    #[arg(long, default_value = "bonsai.toml")]
    config: PathBuf,

    /// Override [tree] style: chokkan, moyogi, shakan, fukinagashi
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

        /// Write here instead of stdout; replaced only after success
        #[arg(long, short)]
        output: Option<PathBuf>,
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

        /// Interval between frames: `4w`, `2w` or `10d`
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

        /// Config file; a missing file means all defaults
        #[arg(long, default_value = "bonsai.toml")]
        config: PathBuf,
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
        Command::Collect { login, output } => {
            let observed_on = Date::today_utc().add_days(-1);
            status(&format!("collecting @{login} through {observed_on}"));
            let snapshot = collect::collect(&collect::GhCli, &login, observed_on, &mut |line| {
                status(line);
            })?;
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
            let mut frames = Vec::new();
            let mut day = start;
            loop {
                let bonsai = Bonsai::grow(&snapshot, &config, Some(day));
                let base = format!("{:04}", frames.len() + 1);
                for theme in Theme::ALL {
                    let path = output_dir.join(format!("{base}-{}.svg", theme.name()));
                    output::write_atomic(&path, render::svg(&bonsai, &config, theme).as_bytes())?;
                }
                frames.push((
                    base,
                    format!("{day}  ·  week {}", bonsai.timeline.age_weeks()),
                ));
                if day >= end {
                    break;
                }
                day = day.add_days(step).min(end);
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
            let config = Config::load(&config)?;
            gallery(&output_dir, &snapshots, &config)?;
            status(&format!(
                "wrote {}",
                output_dir.join("index.html").display()
            ));
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

fn load_config(args: &TreeArgs, snapshot: &Snapshot, status: &dyn Fn(&str)) -> Result<Config> {
    let mut config = Config::load(&args.config)?;
    if let Some(style) = &args.style {
        config.tree.style = style.parse::<Style>()?;
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

fn parse_interval(text: &str) -> Result<i32> {
    let invalid = || {
        Error::Config(format!(
            "`{text}` is not an interval; use for example 4w or 10d"
        ))
    };
    let (number, unit) = text.split_at(text.len().saturating_sub(1));
    let n: i32 = number.parse().map_err(|_| invalid())?;
    let days = match unit {
        "w" => n * 7,
        "d" => n,
        _ => return Err(invalid().into()),
    };
    if days < 1 {
        return Err(invalid().into());
    }
    Ok(days)
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
        let mut still = config.clone();
        still.render.animate = false;
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
            config.tree.style = style;
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
            rows.push((format!("Growth, {}", config.tree.style), stages));
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
        "{}w · {:.0} lifetime · {} pads · L{} · pot {} · {:.0}/wk",
        b.timeline.age_weeks(),
        b.timeline.capped_lifetime(),
        b.skeleton.pads.iter().filter(|p| !p.retired).count(),
        b.extras.level,
        b.extras.pot_tier,
        b.window.total
    )
}
