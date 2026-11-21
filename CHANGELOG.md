# Changelog

All notable changes to gitbonsai, a Japanese maple bonsai grown from GitHub activity. A change to how a tree grows or looks is at least a minor version, because people notice their tree changing; a patch only fixes drawing bugs, wood or leaves drawn where they were never meant to be.

## [0.5.1] - 2026-11-21

Fix release: drawing faults from 0.5.0, and formal upright trees stand taller. Every pot, formal uprights, small trunks, trees with a thick or white trunk or a second trunk, and every tree with fine twigs or dead twigs look a little different. The changes under Changed (taller formal uprights, rows lit as one, bark on small trunks) would make this a minor version; it ships as a patch because they finish the look of 0.5.0.

### Changed

- **Formal uprights stand taller**: a chokkan crown is about as wide as it is tall (it was 1.2 times as wide), and its trunk keeps more girth toward the top; an old one such as torvalds stands 10 to 15% taller.
- **A row has one light**: `gitbonsai row` lights every tree from the upper left, so trees side by side are never lit from opposite sides; a tree on its own is still lit from its open side.
- **Small trunks show their bark**: a trunk too thin for the full light and shade of an old one gets a lit band and a bark line, so a young tree's trunk no longer looks flat.

### Fixed

- **The trunk ends at its foot**: it no longer shows as a band on the soil under the roots, as on torvalds and jacobwilliams; a leaning trunk still goes into the soil.
- **Crossing twigs lie one over the other**: each twig keeps its outline, drawn back to front, so where two cross only the outline of the one in front shows, instead of both outlines running through each other; a twig grows from behind the twig it comes from.
- **Dead twigs lie one over the other**: as living twigs do, so their ink outlines no longer meet in small triangles at forks and crossings, and a dead twig passes behind living ones as well as in front of them.
- **A second trunk stays behind the first**: the shadow along it no longer pokes out beside the main trunk as a jagged wedge near the soil.
- **Shadow bands stay on the wood**: where a trunk or limb bends, as at the trunk's foot, the dark band along it no longer cuts across the bend and shows as a loose line beside the wood or on the soil.
- **Bare windswept crowns branch out**: a resting windswept tree's bare twigs grow from one another instead of all from one spot on long stems, so its crown no longer reads as a comb of parallel lines.
- **Dead wood leaves the bark cleanly**: a dead limb runs out of the bark it leaves, without a round end and a line across it, a dead twig starts on the wood it grows from, and dead wood shows no ink line where its pieces meet.
- **Thin dead whips end in the crown**: a long dead whip with no forks is cut back where the crown no longer reaches over it; a branched dead top stays whole.
- **Small clouds show no grey patch**: a cloud with few leaves no longer shows its solid backing below them.
- **Pot details stay on the pot**: a crackle glaze line no longer runs under a foot, and the ribs of the plain training pot no longer reach past its bottom.
- **Achievement tiers carry the day they were reached**: `collect` dates a tier above the first by the pull request merge or accepted answer that GitHub links to it, and repairs a date an earlier run recorded as the day it first saw the tier, so `--as-of` and a timelapse show an older account as it was. A tier with no linked pull request or answer keeps the day it was first seen.
- **The rim sits on the pot**: a band across the pot's edge that tapers into its sides, shaded in steps with the wall, instead of a stroke whose square ends stuck out at both sides.

## [0.5.0] - 2026-11-21

Visual polish release: every language the crown draws shows, a left tree is lit from its open side, and the legend names only what the tree draws. Most trees change a little from 0.4.0: formal uprights, windswept trees, cascades, left trees and trees with private work the most.

### Added

- **The white trunk has its own legend row**: a semi-cascade's trunk reads "white trunk: Fortran, still growing", and the SVG's description says "A white Fortran trunk: a classic language, still growing." It no longer looks like a dropped language.
- **`hemisphere` sets whose seasons a tree keeps**: `hemisphere = "south"` under `[tree]` runs them six months from the north's; unset, the northern.

### Changed

- **Every language in the crown shows**: a small language's leaves are drawn in front of a big one's, so a language with 5% of the work shows as a patch instead of hiding behind the others.
- **A left tree is lit from its open side**: the light, the pot's highlight and the table shadow mirror with the tree, so every tree is lit from the side it leans away from.
- **`wind` sets the side**: a configured `[tree] wind` now also sets the side the tree leans to and is lit from, not only where a windswept crown streams.
- **A formal upright stands on a third of bare trunk**: its first limb leaves the straight trunk about a third of the way up, its triangle stands taller (1.2 times as wide as tall instead of 1.5) in four tiers a side, and its pads carry 40% more leaves.
- **A windswept crown stays above the rim**: a long downwind arm runs out level just above the pot instead of hanging past it; only a cascade goes lower.
- **A cascade's pads are fuller**: its clouds lie side by side along the arc instead of stacked on one another, so it shows 40 to 54% of the foliage the same tree shows upright, up from 22 to 36%.
- **Private work stays low**: silver leaves no longer top the crown of a windswept tree or a cascade; the top belongs to a public language.
- **Dead wood up a trunk shows in front**: a living trunk that crosses a dead one, or one with a strip of dead wood, passes behind it.
- **Leaves cover their clouds**: the flat color behind a cloud's leaves stays inside them, so sparse clouds no longer show plain patches.
- **The legend lists only leaves the crown draws**: a language too small for a cloud of its own no longer gets a leaf in the legend or a place in "Leaves of" in the description.
- **The legend's leaf is cut deeper**: its five lobes are joined about a third of the way up, as on a Japanese maple, instead of halfway.
- **A resting tree's legend line is shorter**: "resting for the winter", so the legend no longer shrinks the tree beside it.
- **SVGs stay under 215 KB**: a crown heavier than that keeps fewer, larger leaves (230 KB before).
- **A few surprises keep the seasons**: some come and go through the year.
- **The pot grows in steps**: a tree is repotted into the next of nine sizes at the start of spring, or at once when it outgrows its pot by two sizes. A pot never shrinks, and a timelapse keeps it centered.
- **A windswept crown streams downwind**: a branch that ran out level upwind swings over to run out downwind instead of standing straight up, so a single leaning trunk no longer carries its crown as a column.
- **A sprout's leaf pairs stand apart**: each pair stays where it opened, at least a leaf's height below the next.

### Fixed

- **No crown floats**: a branch is drawn at least as thick as the leaves it carries, so a high crown no longer sits on hairline twigs.
- **A cascade's leaves stay on its wood**: pads spread along the arc no longer reach past their branches.
- **A dropped language leaves no bare twigs**: its place in the crown no longer grows brown twigs from the living wood beside it.
- **A leaning trunk goes into the soil**: a steep trunk's foot no longer shows a square cut above the soil.
- **No lone tuft stands off a windswept crown**: a few clouds stranded high, or out on their own long bare arm, come in to the crown.
- **Leaves stay on living wood**: a windswept crown's cloud no longer rides off its own branch on a dead arm swinging past it.
- **Dead twigs never float**: a dead branch left out of the drawing takes its twigs with it.

## [0.4.0] - 2026-11-19

Styles release: a tree's last three years now pick its style, and each of the five styles has its own shape. Every tree may change shape from 0.3.2; set `style` in `bonsai.toml` (or the action's `style` input) to keep one style.

### Added

- **The style comes from the last three years**: with no `style` set, steady work in one main language grows a formal upright (chokkan), new languages an informal upright (moyogi), pauses a slanting tree (shakan), and pauses with several languages a windswept one (fukinagashi). Recent months weigh more, so a tree eases from one style into the next.
- **Kengai, the cascade**: steady work in a legacy language (COBOL, Fortran, Pascal, Perl and 19 others) grows a semi-cascade that leans over the rim of a deeper pot in flat pads, never below the pot. `style = "kengai"` sets it.
- **A legacy trunk turns to white deadwood**: on a kengai tree with a second language's trunk, the legacy trunk whitens over a year and a half and stays full size as deadwood, with the living trunk running over it.
- **A sprout for the first 60 contributions**: a shoot breaks the soil, two seed leaves open in the languages of the 2nd and 3rd contributions, and young leaf pairs follow in the two largest languages (private work in silver), before the seedling takes over at 60.
- **A tree leans to its own side**: the rhythm of its first 60 contributions picks left or right (unless `[tree] wind` sets it), and the windswept, slanting and cascade styles follow it. A left tree is the right one's mirror image, and the side never changes after the 60th contribution.
- **`explain` says where the style comes from**: the share of pauses, of work past one language and of legacy work, or that the style is configured.

### Changed

- **Steady years grow a formal upright**: a chokkan tree is a symmetric triangle in soft layers on a straight trunk, and from the fourth steady year on (120 or more active days each) its lower trunk and root flare thicken.
- **Informal uprights bend deeper and slanting trees lean further**: moyogi trunks make deep S-bends, and shakan trunks lean about 25 degrees with roots braced on the uphill side.
- **Windswept trees stream downwind**: the windward trunk curls in a C and carries its crown on the top arm, the downwind crown hangs lower with air above it, and low branches run level downwind.
- **The pot sits on its table**: a soft shadow under the pot, the light from the upper left.
- **The description names what the tree shows**: the SVG's description (and alt text) names the languages turned to deadwood ("C# turned to deadwood."), lists a small share of the leaves as "a bit of", and says "Mostly private work." when most recent work is private.
- **The legend's leaf is a Japanese maple's**: five broad, pointed lobes joined halfway, the lowest pair held out sideways and the middle one shorter. The crown's leaves are unchanged.

### Fixed

- **The crown's shadow stays on the soil**: on a tree whose crown leans to one side it no longer reaches past the rim as a grey disc beside the pot.
- **No branch ends blunt**: a thick branch end outside the leaves tapers on into the nearest foliage, and twigs no longer poke out through the far side of a cloud.
- **Deadwood is named only where it shows**: the legend and the SVG's description name a dropped language only when the tree draws its dead wood, so a language dropped after a handful of contributions is no longer listed.

## [0.3.2] - 2026-11-19

Release fix: 0.3.2 is 0.3.1 with its binaries. The v0.3.1 release on GitHub has none, so the action at `@v0.3.1` fails; use `@v0.3.2`.

### Fixed

- **Releases carry their binaries**: the release workflow uploads the binaries to a draft release and then publishes it, as immutable releases require. v0.3.1 was published before its binaries were attached and cannot get them.

## [0.3.1] - Unreleased (ships in 0.3.2)

Wood release: limbs stay in proportion to their trunk, dropped languages keep visible dead wood, and the roots grip the soil like a real maple's. Every tree looks different from 0.3.0; stay on 0.3.0 to keep the old look. It ships as a patch, although trees change, because it finishes the wood and roots of 0.3.0's redesign.

### Changed

- **Roots grip the soil**: surface roots are rounded ridges lying on the soil that widen into the trunk and flatten toward a blunt tip; the trunk ends where it enters the soil, and moss grows over the roots in front. A tree has 3 to 7 roots by age (was 5 to 9), longer and thicker the older it is, and a new root no longer moves the others.
- **No limb outgrows its trunk**: a limb is at most a third of the trunk's radius where it leaves, and what it gives up thickens the lower trunk, so a tree whose busy early years dwarf its crown no longer rises as a thin trunk out of a lump of limbs.
- **Dropped languages stay visible**: each era's two biggest dropped languages leave a dead twig, even when they were a small part of their years, and a weathered dead twig keeps 70% of its length and a few bare twigs.
- **The wind chime's clapper hangs inside the bell**: at the lip, where it strikes the rim, under the glass or the metal.
- **SVGs may reach 230 KB** (was 220 KB): the heaviest crowns keep more of their leaves.

### Fixed

- **Mostly private years keep their limb**: an era counts as dropped by its public work only when at least a quarter of it is public, so one dropped contribution among years of private work no longer kills the limb.
- **Gateway timeouts rarely fail collection**: a window GitHub's gateway gives up on (HTTP 502 or 504) is split and fetched in halves, down to single days, on the first request and on a repeat for missing days. A single day that still times out fails the run, as before.
- **Ages read naturally**: a new tree's description says "just planted" and a one-week tree's "grown over 1 week", and `explain` says "1 week" (was "0 weeks" and "1 weeks").

### Dependencies

- **resvg 0.50.0** (was 0.49.0), for `render`'s PNG.

## [0.3.0] - 2026-11-18

Growth release: a tree now changes a little each month and never all at once, its trunk only grows, and dead wood weathers where it grew. Every tree looks different from 0.2.3: languages come away from the trunk as strands instead of growing twin trunks, and crowns are fuller; stay on 0.2.3 to keep the old look. Snapshots from 0.2.3 still load, and `collect` fetches the whole history once to record the new dates.

### Added

- **Strands**: a language with 10% or more of the work claims one of the strands wound round the trunk, which comes away as a trunk of its own the more the language is used and leans out after a quick change of language. Strands replace the 23% twin trunks.
- **Shari**: when the language that founded the trunk is dropped, a bleached strip of dead wood climbs the living trunk, bleaching over the year after.
- **Tinted leaf tips**: silver leaves (private work, or work GitHub could not attribute to a language) take the public languages' colors at their tips, on as many leaves as public work's share of the recent weeks.
- **Monthly and quarterly timelapses**: `gitbonsai timelapse --every 1m` (or `1q`) steps by calendar month or quarter, and the frames come out the same size with a view that only ever widens, so the tree never jumps in the frame.
- **Past views show only what existed then**: `collect` records the day each profile sign was earned and each repository was archived, so `--as-of` and timelapse frames leave out what came later.

### Changed

- **Growth is smooth**: every measure of a tree is half today's value and half the average of the last three months (the leaves, two), so a burst of work shows half at once and the rest over a season. A test grows the fixtures month by month and checks for jumps.
- **A trunk never shrinks**: height, girth, bend and how far a strand has come away are the furthest they reached in any earlier month.
- **Dead wood stays where it grew**: a dropped language's wood dies in place and bleaches over a year, nothing living grows from it, and dead branches of years leaving the crown break back over two years before they fall.
- **The crown follows the recent years**: a language owns a sector of the crown once it has 14% of the crown years' work, growing in up to 22%, and sectors keep their places as shares change.
- **Size follows the work**: crown and limb reach grow with the work done, not with the calendar, so a few contributions over many years stay a small tree.
- **Fuller crowns**: 1.5 times as many leaves on the same clouds; a typical SVG stays within 190 KB (was 150 KB), and the heaviest crowns keep fewer, larger leaves so no SVG passes 220 KB.
- **Limbs come from behind the trunk**: limbs leave toward the side or back with a flared collar, and a side branch starts on its parent's surface, so no join shows a gap or a corner.
- **Limbs taper to fine tips**: a limb is about a third of the trunk's girth where it leaves and thins steadily to its tip, and the collar flares only at the join, so no limb ends in a thick round stump.
- **Broken dead limbs splinter**: a fresh break in a dead limb ends in torn fibres and weathers to a point as it bleaches; fine dead twigs keep their round ends.
- **Smoother wood in smaller files**: branches are smoothed before their outlines are simplified, which cuts up to 7% from an SVG.
- **A rested seedling fills out over a year**: a seedling that rested past its first year becomes a tree the day work resumes and fills out from there, instead of standing full grown that month.
- **A comeback keeps its old crown a while**: after a pause the old crown stays until the comeback year is a year old, then turns into a limb over a year.
- **`collect` keeps what it knew**: a `--full` run, or one that cannot reach a profile page, keeps the dates an earlier run recorded, and a sign's first date is fetched only once.
- **`explain` shows the leaves left**: each language and the crown show the share of leaves left, which follows how recent the work is, in place of a density figure the render no longer used.
- **Snapshots are schema 3**: they add the dates signs were earned and repositories archived; gitbonsai 0.2 cannot read them, so going back means running its `collect` again.

### Removed

- **Twin trunks and the dead snag beside the trunk**: a dropped trunk language now leaves a dead strand or a shari instead.

### Fixed

- **A past view never shrinks the tree**: `--as-of` and timelapse frames keep the start date the whole history gives, so a tree no longer drops back to a seedling in the month steady work begins after a few early stray days.
- **Heavy windows no longer fail collection**: a window GitHub refuses as too heavy (`RESOURCE_LIMITS_EXCEEDED`) is split and fetched in halves.
- **A huge `--every` is refused**: `timelapse --every 2147480000d` overflowed the day count and never finished.

### Dependencies

- **clap 4.6.8** (was 4.6.7).
- **toml 1.2.1** (was 1.1.8).

## [0.2.3] - 2026-11-17

Maintenance release: a release created in the GitHub UI gets its binaries again and the crate on crates.io, and the Action checks each binary's build provenance instead of the release's attestation. Trees change only on antique pots with kintsugi and where a dropped language held part of the recent years, which would make this a minor version; it ships as a patch because the changes are small.

### Added

- **Releases publish to crates.io**: once the binaries are attached and the Action passes its end-to-end check, the release workflow publishes the crate through trusted publishing; a version already there is skipped.
- **A release checks its version first**: the tag, `Cargo.toml`, `Cargo.lock`, the README, the example workflow and the CHANGELOG must name the same version, or nothing is built. CI runs the same check (`scripts/check-version.sh`) on every push.

### Changed

- **A release created in the GitHub UI gets its binaries**: the release job uploads them to it, or creates the release when only a tag was pushed, and fails when binaries are missing.
- **A dropped language keeps no leaves in the crown**: its recent years show as deadwood only, as the legend says; the crown holds living languages.
- **The Action checks build provenance**: each binary carries a signed attestation from the release workflow, shipped with the release as `gitbonsai-provenance.sigstore.json` and checked offline with `gh attestation verify --bundle` before the sha256 check, so no token permission is needed. Releases are no longer immutable, so this replaces `gh release verify-asset`.

### Fixed

- **Kintsugi runs under the pot's bands and seal**: the gold seams of archived repositories no longer cross the carved bands and maker's seal of an antique pot.
- **Small trees with a dropped trunk language stand the right way round**: dead wood takes at most 35% of a tree's girth, so the living trunk carries the bark. An account of about 330 contributions drew its dead snag as the trunk.
- **A misspelled login gets a plain message**: `collect` says there is no such GitHub user instead of printing GitHub's raw error.
- **The size is limited to 4096 px**: `--size` and `[render] size` above 4096 (or 0) are a config error; `--size 99999` wrote a 72 MB PNG.
- **`--as-of` before a snapshot's first day** shows an empty pot dated that day, not a later one.
- **SVG text drops characters XML forbids**, so a hand-edited snapshot can no longer produce an invalid SVG.
- **The README installs with `--locked`**, so a dependency update never changes a tree.

## [0.2.2] - 2026-11-16

Release fix: 0.2.1 was published without its binaries, so its Action cannot run. 0.2.2 ships the same changes; use `@v0.2.2`.

### Fixed

- **Releases are created from the tag only**: the release job creates the release, uploads the binaries and publishes it, and fails if a release for the tag already exists. A release published by hand is immutable before its binaries arrive.

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
