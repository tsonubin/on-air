# Project website and build log

Record for 2026-10-18: adding `website/` (project site plus build log) and
extending `CHANGELOG.md`. This is a documentation change; it touches no
code, packaging or client.

## What was added

| Path | What it is |
| --- | --- |
| `website/index.html` | The project site: what on-air is and why, the shipping features, how the core/clients architecture works (with an inline SVG audio-flow diagram), install, and the roadmap. |
| `website/build-log.html` | The build log: every commit on `main` since 2026-08-26, grouped by milestone, plus the open PRs. |
| `website/styles.css` | Theming (light and dark via `prefers-color-scheme`, overridable per visitor), layout and print-friendly typography. |
| `website/theme.js` | The only script: an optional light/dark override kept in `localStorage`. |
| `CHANGELOG.md` | Extended `Unreleased`: the website, the one-line Omarchy installer (PR #12, in review), the L16 dither change (PR #11, in review) and a labelled `Roadmap` subsection. |
| `README.md` | One added line linking to `website/`. |

## Tech choice and why

Plain HTML, CSS and a few lines of vanilla JavaScript. No build step, no
framework, no external fonts or CDN links, no tracking. Rationale:

- The site must render with no network access at runtime. Everything it
  loads lives in `website/`.
- There is nothing to compile, so nothing joins `pnpm-workspace.yaml` or
  `turbo.json`; `pnpm check:ci` at the repository root still checks the
  files because Biome's `files.includes` covers the tree.
- The audio-flow diagram is inline SVG drawn with `currentColor` and CSS
  custom properties (no painted background, no scripts inside the SVG), so
  it follows the visitor's theme.
- The theming works without JavaScript: `prefers-color-scheme` selects the
  palette and `theme.js` only stores an explicit override.

## How the build log was derived

- Source of truth: `git log --oneline --date=short --pretty='%ad %s'` on
  `main` as of 2026-10-18 (121 commits) plus the existing `CHANGELOG.md`.
- Entries are verbatim commit subjects and dates, generated mechanically
  from the log and transcribed without edits. Nothing is invented or
  rounded; group headings and one short context paragraph per group are the
  only editorial text.
- Groups follow the milestones named in `docs/design/`: quality overhaul
  (2026-10-08), omarchy-first TUI (2026-10-13), TUI/GUI parity (2026-10-14),
  quickshell patchbay (2026-10-17) and the stereo pipeline (2026-10-17 →
  2026-10-18). Three groups cover what predate or sit between those
  records: the design and Linux audio path (2026-08-26 → 2026-08-27), the
  desktop/phone/streaming work (2026-08-28 → 2026-09-30) and the contributor
  reorganisation (2026-10-15).
- Work that is still open is listed under "In review" and marked as not
  landed: PR #11 (L16 dither) and PR #12 (one-line Omarchy installer), both
  opened 2026-10-18. PR #13 (roadmap action plan, same day) is listed there
  too because the log claims to cover the open PRs.

## Claims on the site

Every shipping claim was checked against the tree before it was written:
port 47990 and the control-plane ownership (`README.md`, `INSTALL.md`),
the WAV stream under a per-activation nonce
(`packages/core/src/session/mod.rs::new_stream_nonce`,
`packages/core/src/api/stream.rs`), interleaved L16 stereo with per-channel
EQ state and the `(L+R)/2`/silent-extra channel mapping
(`packages/core/src/dsp/{mod.rs,eq.rs,bridge.rs}`), the audio benchmark and
capture buffer (`packages/core/src/audio_benchmark.rs`,
`/api/audio/benchmark`, `/api/audio/buffer`), the Verlet cable simulation
(`apps/quickshell/Rope.js`), and the packaging channels (`INSTALL.md`).
The release status follows `INSTALL.md`: only v0.1.0 is published, and the
channels that fetch 0.2.0 assets are labelled as awaiting that release.

Roadmap items are labelled "in progress" or "planned" and are never listed
as features.

## Not done here

- No deployment: the site is served by whatever hosts `website/` (any
  static server, or the GitHub Pages setup a maintainer chooses). This
  change does not touch CI or publishing.
- The build log is a dated snapshot and is extended by hand as work lands;
  no generator script is kept in the tree.
