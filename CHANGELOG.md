# Changelog

All notable changes to gitbonsai, a Japanese maple bonsai grown from GitHub activity. Any change to how a tree grows or looks is at least a minor version, because people notice their tree changing.

## [0.2.0] - 2026-11-16

Growth release: every tree is regrown as an exhibition maple, short, wide and full instead of tall, with its history in its trunks, limbs and crown. Every existing tree changes shape and some leaf colors change; there is no setting for the old growth, so stay on 0.1.2 to keep it.

### Added

- **Trunks per language**: a language with 23% or more of lifetime contributions grows its own trunk, so one trunk is common, two happen and three are rare. A first-year account is always a single sprout.
- **Eras as low limbs**: older years become limbs under the crown, oldest lowest, and a long pause leaves a gap of bare trunk.
- **A domed crown of the recent years**: the newest three to six years form one dome of leaf clouds, with a sector per crown language.
- **Silver private work**: contributions without language data, such as work in private repositories, grow silver leaves. The legend shows a "private work" row and `explain` prints its share.
- **Private work settings**: `[leaves] private` sets the silver, and `[leaves.private_periods]` declares languages for a period, for example `"2019..2023" = ["Python"]`.
- **Bare winter trees**: after a year without contributions the crown stands bare, a dome of fine twigs, and the legend says so.
- **`gitbonsai row`**: several trees side by side at one scale, written as `row-light.svg` and `row-dark.svg`.
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
