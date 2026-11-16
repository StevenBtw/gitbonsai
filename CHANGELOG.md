# Changelog

All notable changes to gitbonsai, a Japanese maple bonsai grown from GitHub activity. Any change to how a tree grows or looks is at least a minor version, because people notice their tree changing.

## [0.2.2] - Unreleased

Release fix: 0.2.1 was published without its binaries, so its Action cannot run. 0.2.2 ships the same changes; use `@v0.2.2`.

### Fixed

- **A release created in the GitHub UI gets its binaries again**: the release job uploads them to it, or creates the release when only a tag was pushed, and fails when binaries are missing.

### Removed

- **The Action's attestation check**: releases are no longer immutable, so `gh release verify-asset` is gone; the sha256 check stays.

## [0.2.1] - 2026-11-16

Honesty and safety release: silver leaves now mean private work only, issues and reviews grow their language, and weekly runs fetch only recent months. Trees with silver change, which would make this a minor version; it ships as a patch because the change is small. The first run after upgrading collects the whole history once.

### Added

- **Weekly runs fetch only recent months**: `gitbonsai collect --output <file>` reuses a snapshot from the same version there and fetches again from the start of the previous quarter; `--full` fetches everything. The Action keeps it as `bonsai.snapshot.json` in `output-dir` (new `snapshot` input and output, and `full` to fetch everything once), and the example workflow commits it.
- **Issues, reviews and created repositories grow their language**: each counts one for its repository's primary language, as opened pull requests do.
- **A gallery reference**: `gallery.sha256` holds the hash of every gallery SVG, and CI fails when a tree changes without it. `scripts/gallery-hashes.sh` writes it.

### Changed

- **Silver leaves are private work only**: GitHub's private count is spread over the days it covers, and public work without a language (discussions, repositories without one) takes the colors of that week's languages. Snapshots are now schema 2; schema 1 snapshots still load and render as before.
- **The example workflow runs on Mondays** (`17 6 * * 1`): the week just ended is complete, so new growth shows a week sooner.
- **The minimum Rust version is 1.88** (was 1.99), checked in CI.
- **`explain` and the README name silver as private work**, shown only when a profile includes private contributions.

### Fixed

- **Achievement pages with non-ASCII text no longer stop collection**: a card cut inside a multibyte character panicked, so `collect` and the Action failed every run.
- **`timelapse --every` rejects a non-ASCII unit**: `4é` was a panic and is now an error.
- **A missing config file named with `--config` is reported**: it prints a warning and uses the defaults; a missing default `bonsai.toml` stays silent.
- **The crate docs describe the 0.2.0 growth**: girth, spread and fullness, not height.

### Security

- **Release builds run without write access**: only the publishing job can write, checkouts keep no credentials, and every action is pinned to a commit.
- **The Action checks where its binary came from**: `gh release verify-asset` checks the release's attestation before the sha256 check. Releases are immutable from 0.2.1.

## [0.2.0] - 2026-11-16

Growth release: every tree is regrown as an exhibition maple, short, wide and full instead of tall, with its history in its trunks, limbs and crown. Every existing tree changes shape and some leaf colors change; there is no setting for the old growth, so stay on 0.1.2 to keep it.

### Added

- **Trunks per language**: a language with 23% or more of lifetime contributions grows its own trunk, so one trunk is common, two happen and three are rare. A first-year account is always a single sprout.
- **Eras as low limbs**: older years become limbs under the crown, oldest lowest, and a long pause leaves a gap of bare trunk.
- **A domed crown of the recent years**: the newest three to six years form one dome of leaf clouds, with a sector per crown language.
- **Silver private work**: contributions without language data, such as work in private repositories, grow silver leaves. The legend shows a "private work" row and `explain` prints its share.
- **Private work settings**: `[leaves] private` sets the silver, and `[leaves.private_periods]` declares languages for a period, for example `"2019..2023" = ["Python"]`.
- **Bare winter trees**: after a year without contributions the crown stands bare, a dome of fine twigs, and the legend says so.
- **`gitbonsai row`**: `gitbonsai row Name=snapshot[@YYYY-MM-DD] ...` puts several trees side by side at one scale, written as `row-light.svg` and `row-dark.svg`.
- **`newbie` and `showcase` fixtures**: the README's example trees, reproducible with `gitbonsai render fixture:showcase` and `gitbonsai row`.

### Changed

- **Age shows as girth, spread and fullness, not height**: a newcomer is a thin sprout and an old account short and stout. `explain` now lists trunks and crown years and gives height in units.
- **Deadwood follows dropped languages**: a language turns to deadwood only after a year of silence and 30 more active days of other work, so a resting account drops nothing. A dropped trunk language stands as a dead snag, a dropped era as a dead limb, with at most three small stubs on the trunk.
- **Leaf colors**: TypeScript is blue-violet, JavaScript bright yellow, Java autumn red, and C and other colorless languages deep burgundy. The ten busiest languages always stay clearly apart from each other and from private silver; `[leaves.colors]` still overrides them.
- **Movement follows rhythm**: irregular activity bends trunks and limbs more and steady activity grows straighter. `[tree] style` now nudges this movement (`chokkan` straighter, `shakan` and `fukinagashi` leaning).
- **Old foliage thins**: a year's leaves count for half after four years; the wood stays.
- **"History is never erased" replaces "wood never moves"**: the tree is restyled as it matures, but every active year keeps its place and the trunk never thins.
- **The README**: a generated example with its legend, and five trees at one scale: a simulated newcomer, StevenBtw, tj, torvalds and steipete.

### Removed

- **Space-colonization growth**: the height curve (`growth::height`, `H_REF`), foliage pads and crown recession are gone.

### Fixed

- **Surface roots stay inside the pot** on thick trunks.
- **Deadwood has an inked edge and grain**, so bleached wood shows on a white page.

## [0.1.2] - 2026-11-01

- A GitHub Action: `uses: StevenBtw/gitbonsai@v0.1.2` downloads the release,
  checks its sha256, then collects and renders, with inputs for style, legend
  and config. The profile workflow example uses it.

## [0.1.1] - 2026-11-01

Trees change look in this release: smoother wood and softer leaves.

- An optional legend beside the tree (`[render] legend = "left"` or `"right"`,
  or `render --legend`): each language's leaf color and what the signs mean.
  Off by default; the PNG leaves it out.
- Smoother wood: curved outlines, no knobs where pieces meet, side branches
  that start at their own girth instead of as thorns, and tapered tips.
- Softer leaves: each pad is lit as one volume in three soft tones, with cool
  shadows and warm light.
- Smaller SVGs.

## [0.1.0] - 2026-11-01

The first release.

- `collect`: contribution history through `gh api graphql` in quarter windows.
- A Japanese maple bonsai grown by space colonization toward per-language
  foliage pads, with Murray's-law girth, crown recession and deadwood (jin)
  for languages silent for a year.
- Lit rendering: leaf tufts shaded by light and occlusion, shaded wood,
  depth sorting, light and dark variants, a static PNG.
- Leaf colors from GitHub's language colors on the Japanese maple palette;
  bronze new growth from last week; samaras for the lifetime level; falling
  momiji in a milestone week; moss for the current streak; pot tiers for the
  longest streak.
- A few hidden surprises.
- Four styles: moyogi, chokkan, shakan, fukinagashi.
- `render`, `explain`, `timelapse`, `gallery` and `fixture` commands.

---

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
