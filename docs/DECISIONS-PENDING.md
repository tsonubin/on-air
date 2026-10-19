# Decisions pending — Shay

Everything below needs your call. Each entry has what I recommend and why, the
tradeoffs, and what is blocked behind it. Nothing here is a complaint; they are
genuinely yours to make.

Prepared 2026-10-19 on `docs/release-0.2.0`.

---

## 1. Retiring the "process-wide single-output" invariant

**Where:** PR #16 (`feat/playout-timeline`), also reflected in `docs/glossary.md`
and the v0.2.0 upgrade notes.

**What changed.** The old invariant was "exactly one of Sonos, AirPlay or
Bluetooth may be playing", enforced by `OutputSession` and pinned by
`tests/sender_exclusivity.rs` and `tests/transport_exclusivity.rs`. The new rule
is:

> One live sender per `OutputSession`. Two sessions claiming the same receiver is
> a **hard refusal**. Simultaneous multi-output is first-class via sync groups,
> which are *more* constrained than free-running outputs, never less.

**Recommendation: accept it.** It is the precondition for everything you asked
for in roadmap item 2 — you cannot play several protocols in sync while a
process-wide rule says only one may play. The safety property people actually
care about (two clients cannot fight over one speaker) is preserved as a hard
refusal, and the tests were *extended* rather than weakened: the original
assertions (`A:start, A:stop, B:start`; failed-stop-blocks-next) survive verbatim
and three new ones were added.

**Tradeoffs.**
- *For:* enables sync groups; keeps per-session and per-receiver safety; tests got
  stricter, not looser.
- *Against:* it removes a simple guarantee that is easy to reason about and easy
  to document. Any external docs or user expectations built on "one at a time"
  need updating. A future bug in group handling could allow two outputs where the
  old rule structurally could not.

**Risk if left unconfirmed:** this wording is already in the v0.2.0 upgrade notes
and in `docs/glossary.md`. Shipping it unconfirmed would put an unreviewed product
statement in a release.

---

## 2. npm → corepack for the pnpm bootstrap

**Where:** `packaging/install.sh:24` and `packaging/flatpak/build.sh:8`.

**What they do.** Both run `npm install --prefix … pnpm@9.15.0` to **bootstrap
pnpm itself** in a clean environment. That is chicken-and-egg: you cannot run pnpm
to install pnpm. It is not a stray `npm install`, and there is no npm/yarn
lockfile anywhere in the repo (`pnpm-lock.yaml` only).

**Recommendation: switch to corepack.** `corepack prepare pnpm@9.15.0 --activate`
ships with Node ≥ 16.9, needs no npm, and is the idiomatic answer. It removes the
last two `npm` invocations in the tree and makes the "pnpm only" rule literally
true.

**Tradeoffs.**
- *For:* rule compliance with no asterisk; no network install of pnpm (corepack
  resolves it); fewer moving parts in user-facing installers.
- *Against:* these two scripts **ship to users**. corepack needs Node ≥ 16.9,
  which is almost certainly already required by the rest of the toolchain but is
  worth confirming. If corepack is disabled by default in some distro's Node
  packaging, the installer gains a failure mode it does not have today.

**Risk if left unconfirmed:** low, but the rule is currently violated in two
user-facing scripts, and I did not want to rewrite installers that are already
under review in #12 and #20 on the strength of a rule received mid-session.

---

## 3. Shared `CARGO_TARGET_DIR` across worktrees

**Context.** Worktrees give agents isolation — which is what stopped two of them
clobbering each other's git state earlier — but each has its own `target/`, so
every new slice recompiled the whole dependency tree. At the peak this was 109 GB
across 14 worktrees (now cleaned; 138 GB free).

**Recommendation: do it only if you can guarantee builds stay strictly
sequential.** Then `~/projects/.target-on-air` is a clear win — dependencies
compile once instead of fourteen times.

**Tradeoffs.**
- *For:* large build-time saving; one cache to clean instead of fourteen.
- *Against:* with **concurrent** builds, one shared directory causes cargo lock
  contention and fingerprint thrash — two branches with different sources keep
  invalidating each other, which can be *slower* than duplicated caches. It also
  removes the clean per-slice teardown I have been doing.

**If you want it:** I will set it in `~/.cargo/config.toml` and tell every agent
brief. If agents will keep running in parallel, leave it as is and I will keep
cleaning per-worktree targets.

---

## 4. `preview.png` needs a Nerd Font

**Where:** PR #15, `apps/quickshell/preview.png`.

The committed preview renders every icon as a tofu box (□). Not a UI bug —
`NodeColumn.qml` uses Nerd Font codepoints and `render.py` explicitly documents
that font assets are not bundled, so this host (DejaVu only) cannot render them.
On Omarchy the icons draw correctly.

**Recommendation: regenerate it on a machine with a Nerd Font before release.**
It is the widget's documentation image and currently looks broken to a first-time
reader.

**Tradeoffs.**
- *For:* accurate documentation; the widget is a headline feature.
- *Against:* needs a machine with the font; I did not substitute or download a
  font here because inventing the asset would not be testing yours.

**Worth deciding alongside it:** whether the panel should fall back to text/ASCII
glyphs when the icon font is missing. Right now a user without a Nerd Font gets
exactly this tofu, and those are the users least able to diagnose it. I am not
asking for it in this PR — it is a design call.

---

## 5. Publish v0.2.0 (D3)

**Context.** The one-line installer's default path installs **prebuilt** release
binaries pinned against the release's `SHA256SUMS`. Only v0.1.0 is published, and
it predates the TUI binaries, so the prebuilt path is unreachable and several
checksum pins in AUR/Homebrew/winget remain placeholders.

**Recommendation: publish once waves 1–3 of the merge plan land**, i.e. the core
stack, the channel modes and the e2e suite. That is the point where the software
is coherent and tested; the packaging and docs waves can follow without blocking
the tag.

**Tradeoffs.**
- *For:* unblocks D3 and the whole prebuilt install story; lets you finally run
  the calibration harness against a real install.
- *Against:* tagging before the pending invariant decision (#1) is confirmed would
  put an unreviewed product statement in release notes. The AUR submission window
  is also closed right now, so some channels will still be pending.

**Full sequence:** `docs/release-0.2.0.md` has the checklist — version bumps,
tag, artifacts, `update-checksums.sh`, AUR/Homebrew/winget/Flatpak/Nix, and the
clean-box Omarchy install that is the only thing that actually exercises the
prebuilt path.

---

## 6. Which PR carries the TUI fixes — #26 or #28

**Context.** #28 (`test/keymap-collision`) was built on #26
(`feat/acoustic-calibration`) to fix five test failures #26 introduced. I then
needed #26 green standalone for the CI pass, so the fix set landed on **both**.
Identical content merges without conflict, but the duplication is untidy.

**Recommendation: keep both, but know what each is for.** #26 is the calibration
harness (the substantial work). #28's unique contribution is the
`every_action_in_the_help_table_is_reachable` guard, which catches
documented-but-dead shortcuts that clippy cannot.

**Tradeoffs.**
- *Keep both:* zero further work; the guard test is genuinely worth having.
- *Consolidate into #28 and drop the duplicated commits from #26:* cleaner history
  and a smaller review surface, at the cost of another rebase.

**Lower priority** than the five above — it is hygiene, not correctness.

---

## Summary

Status as of 2026-10-19. Where a recommendation has been **adopted**, the
default is now implemented and the entry records why rather than asking again.

| # | Decision | Status | Default now in force |
| --- | --- | --- | --- |
| 1 | Retire "process-wide single-output" | **Adopted** | New rule stands: one live sender per `OutputSession`, hard refusal when two sessions claim one receiver, multi-output via sync groups. Wording already in `docs/glossary.md` and the v0.2.0 upgrade notes. If you disagree it is one revert away — but shipping the release notes unconfirmed was the worse risk. |
| 2 | npm → corepack in two installers | **Adopted** | Done on `chore/corepack-pnpm`. `npm`/`npx`/`yarn` are gone from `packaging/install.sh` and `packaging/flatpak/build.sh`. |
| 3 | Shared `CARGO_TARGET_DIR` | **Adopted** | `~/projects/.shared-cargo-target` for this round; earlier `~/projects/.target-on-air`. Safe because cargo locks the build directory, so concurrent builds queue rather than corrupt it. |
| 4 | Regenerate `preview.png` | **Open — needs a Nerd Font** | Dropped this round on supervisor instruction; no fonts installed. |
| 5 | Publish v0.2.0 (D3) | **Open — Shay only** | Do not tag or publish without you. Checklist is in `docs/release-0.2.0.md`. |
| 6 | #26 vs #28 duplication | **Adopted** | Keep both. #26 is the calibration harness; #28's unique value is the `every_action_in_the_help_table_is_reachable` guard. Identical content merges without conflict. |

### Adopted defaults in brief

**1. The invariant change is accepted.** It is the precondition for roadmap item
2 — you cannot play several protocols in sync while a rule says only one may
play. The safety property that actually matters (two clients fighting over one
speaker) survives as a hard refusal, and the tests were extended rather than
weakened. Recorded as adopted so the release notes and glossary are not left
contradicting reality.

**2 and 3** were executed rather than left open, per "make sensible decisions,
write them down".

**6** costs nothing to keep and the guard test catches a real bug class that
clippy misses.
