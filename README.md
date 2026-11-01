# gitbonsai

Grow a Japanese maple bonsai from your GitHub activity, as an animated SVG for your profile README.

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/veteran-dark.svg" />
  <img src="assets/veteran-light.svg" width="400" alt="A Japanese maple bonsai grown from 16 years of synthetic GitHub activity" />
</picture>

*A synthetic account: active since 2010, busy since 2023, with every easter egg at its top tier.*

## How your activity becomes a tree

| What you see | What it shows |
|---|---|
| **Height** | Age, from your first sustained activity. It slows down but never stops. |
| **Girth** | Every contribution ever, flowing down the branches (Murray's law). Wood never shrinks. |
| **Branches and pads** | Each language earns foliage pads on its own side of the tree as its lifetime grows. Your top language feeds the crown. |
| **Leaf color** | The language, with its GitHub color moved onto the Japanese maple palette (purple, crimson, orange, gold, green). |
| **Leaf density** | Your last 13 weeks. |
| **Bronze tips** | New growth from last week. |
| **Bare wood** | A language you haven't used in 13 weeks. |
| **Bleached deadwood (jin)** | A language silent for a year. |
| **Samaras** | Lifetime level: the winged seeds appear from level 3 and multiply with every level. |
| **Falling leaves** | You crossed a level this week. |
| **Moss** | Your current weekly streak. |
| **The pot** | Your longest streak, from a plastic training pot to an antique with a maker's seal. |
| **Drip irrigation** | Level 11: 300,000 contributions. |

Weeks are capped at 1,000 contributions, so bots and mass auto-commits can't inflate the tree.

### Easter eggs

GitHub achievements leave small objects in and around the pot. Each grows with the tier:

| Achievement | x1 | x2 | x3 | x4 |
|---|---|---|---|---|
| Pull Shark | small stone fish | goldfish pebble | stone tuna | stone shark |
| Starstruck | paper star | clear-glass fūrin | blue fūrin painted with stars | golden fūrin, its strip swaying |
| Pair Extraordinaire | | two pebbles | yin-yang pebbles | red-crowned cranes |
| Galaxy Brain | glowworm | firefly | firefly and a crescent moon | firefly and a full moon |
| Public Sponsor | a 5-yen coin (*go-en*, "good connection") | | | |
| Arctic Code Vault | snow on the rim | | | |
| Mars 2020 | a bamboo copter (*taketombo*) | | | |

The one-click achievements (Quickdraw, YOLO, Heart On Your Sleeve, Open Sourcerer) are left out to keep the tree calm.

**Kintsugi**: every archived repository is a break in the pot, mended with gold (up to 19; about half face the front).

## Quick start

Needs Rust 1.99 and the [GitHub CLI](https://cli.github.com) (`gh auth login`).

```sh
cargo install gitbonsai
gitbonsai collect <login> --output me.snapshot.json
gitbonsai render me.snapshot.json            # writes bonsai/: light and dark SVGs, a PNG and preview.html
gitbonsai explain me.snapshot.json           # every number behind the tree
```

Embed it in your profile README:

```html
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="bonsai/bonsai-dark.svg" />
  <img src="bonsai/bonsai-light.svg" width="400" alt="My bonsai, grown from my GitHub activity" />
</picture>
```

`collect` reads public data only: your contribution calendar and which languages your commits and pull requests were in (GraphQL through `gh`), your archived repositories, and your public achievements page.

## Settings

Put a `bonsai.toml` next to your snapshot; see [examples/bonsai.toml](examples/bonsai.toml). A missing file means all defaults.

- **Styles**: `moyogi` (informal upright, the default), `chokkan` (formal upright), `shakan` (slanting) and `fukinagashi` (windswept, with `wind = "left"` or `"right"`).
- **Leaves**: override a language's color verbatim, for example `Rust = "#C0703A"`.
- **Pot**: a glaze color, used from pot tier 3 up.
- **Render**: `animate = false` leaves out every animation; `size` sets the display width.

All motion respects `prefers-reduced-motion`.

## Design tools

| Command | What it's for |
|---|---|
| `gitbonsai gallery [--snapshot me.snapshot.json]` | Every fixture in every style plus growth stages, on one page (`gallery/index.html`) |
| `gitbonsai timelapse <snapshot> --every 4w` | Numbered frames over an account's life, with a scrubber page (`frames/index.html`) |
| `gitbonsai fixture [name]` | List the synthetic accounts, or write one as a snapshot |
| `--as-of YYYY-MM-DD` | On `render`, `explain` and `timelapse`: the tree as it was that day |

Any snapshot argument also takes `fixture:<name>`, for example `gitbonsai explain fixture:veteran`.

## How it works

```text
collect -> snapshot.json -> timeline -> skeleton growth -> scene -> lit painting -> SVG
```

- **Growth** is space colonization (Runions, Lane and Prusinkiewicz, 2007): each week your contributions become attraction points inside your languages' foliage pads, and branches grow toward them. Growth only appends: next week's tree is this week's plus new wood, and no branch ever moves.
- **Girth** follows Murray's law: a branch's radius cubed is the sum of its children's.
- **Painting** uses one light. Each leaf tuft is shaded by how it faces the light and how deep it sits in its pad; wood gets a lit and a shadow side; everything is depth-sorted. There are no filters or gradients, so the SVG renders the same everywhere, including behind GitHub's image proxy.
- **Determinism**: collection is the only step with I/O. Everything after it is a pure function of the snapshot and the config, with its own hash and random streams, so the same input gives the same bytes on every platform (CI checks this).

## Development

```sh
cargo install prek && prek install   # fmt, clippy, typos, cargo-deny on commit; tests on push
prek run --all-files
```

The `png` feature (on by default) pulls in resvg; `--no-default-features` builds the SVG-only binary.

## Credits

- The pipeline shape and determinism approach follow [git-koi](https://github.com/yalongwastaken/git-koi) by Anthony Yalong (MIT).
- Space colonization: Runions, Lane and Prusinkiewicz, *Modeling Trees with a Space Colonization Algorithm* (2007).
- Self-organizing trees and the pipe model: Palubicki et al., *Self-organizing tree models for image synthesis* (2009).
- Phyllotaxis: Vogel, *A better way to construct the sunflower head* (1979).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
