# on-air v0.2.0 — release notes (draft)

**Status: draft. Nothing here is tagged, published or released.**
Prepared 2026-10-19 on branch `docs/release-0.2.0`. Not for publication until
the checklist at the end passes and the pending decisions in
[DECISIONS-PENDING](DECISIONS-PENDING.md) are answered.

v0.1.0 shipped the first working Linux audio path. v0.2.0 is the release that
makes it *good*: real stereo you can see, an audio path measured instead of
guessed, a machine that tunes itself on first run, and the foundation for
playing several different kinds of speaker together.

---

## Highlights

### Stereo, end to end, and visible

Left and right are kept apart from capture through the EQ, the resampler and the
radio stream that Sonos and AirPlay pull. A mono destination gets a true
`(L+R)/2` downmix; a destination reporting more than two channels gets silence
on the extras rather than a silent channel collapse.

What changed here is not just the pipeline — it is that **you can now see it**.
Every client renders separate left and right meters, and a per-output channel
mode (`auto` / `stereo` / `mono`) lets you choose. Selecting `stereo` on a
device that only plays mono is **refused with an explanation** rather than
quietly dropping a channel.

A structural guard makes silent stereo collapse a compile error rather than a
reviewer's catch: no sender can spell a channel count directly, and a new sender
does not build until it is classified.

### The audio path measures itself

- **Dithered L16 conversion.** The float→16-bit step used to truncate toward
  zero, which is the worst possible rounding for audio — the error is
  signal-correlated and heard as distortion on quiet passages. It now rounds to
  nearest with one LSB of triangular-PDF dither, leaving a constant hiss floor
  near −93 dBFS instead.
- **Three resampler quality profiles** (`economy` / `balanced` / `high`),
  selected by a benchmark that measures each one *on your machine* and
  recommends from the result.
- **First-use tuning.** A new computer runs the audio check once, by itself, and
  configures its capture buffer and resampler from the verdict. It never
  overwrites a setting you chose, and it never runs twice.
- **The audio path allocates nothing per chunk** in steady state, and that is a
  test rather than a claim in a comment.

### Several speakers, one moment (foundation)

This is the release where cross-protocol sync becomes possible rather than
theoretical. Every audio chunk now carries a frame index and a capture
timestamp; outputs can be joined into a **sync group** started at one
presentation epoch; each holds a delay line so a fast transport waits for a slow
one; and clock drift is steered through a variable-ratio resampler.

A per-output **latency trim** and sync state (`free-running` / `syncing` /
`locked`, with drift in ppm) are exposed over the API and shown in the clients.

> **Read this before trusting the numbers.** Every latency figure in v0.2.0 is a
> **documented default, not a measurement** — AirPlay ≈ 2 s, Bluetooth ≈ 150 ms,
> local Pulse 50 ms, and Sonos 1000 ms is an explicitly flagged placeholder. The
> trim exists precisely because these must be corrected. This release ships the
> acoustic calibration harness that measures them; running it on real hardware is
> the step that turns them into numbers.

### MPRIS and Omarchy

External media controls can see and drive on-air. The D-Bus bridge holds **no bus
connection at all** while nothing is playing, so a laptop with no disc and no
active output shows no phantom player. On Omarchy 4 the panel gains a media
widget and coalesced notifications for the events worth knowing about —
output connected, disconnected or failed, audio paused or resumed, and a rotated
pairing PIN.

### The patchbay is cables only

Connecting is a gesture: press a socket, drag a cable to the other side.
Right-clicking anywhere on a cable removes it immediately. The click-to-connect
menu is gone — it was a second way to do what the cable already did, and the two
together were confusing.

### Install

`packaging/omarchy/install.sh` is now a one-line installer for the prebuilt
release, with checksums pinned from the release's own `SHA256SUMS`. It fails
closed: a missing or malformed checksum entry is refused rather than installed.
The AUR recipes follow the submission guidelines.

---

## Upgrading from v0.1.0

Nothing here should lose data, but a few things change behaviour.

1. **Settings gain new keys** (`dsp_quality`, `recommended_dsp_quality`,
   `channel_modes`, `sync_trims`, `sync_groups`, `first_run_tuning`). An existing
   `settings.json` is read as before and the new fields default sensibly. A
   v0.2.0 settings file is **not** readable by v0.1.0 — downgrade means losing
   those fields.

2. **The audio path sounds different, on purpose.** Dithered rounding replaces
   truncation. If you have tuned around the old conversion's distortion, expect
   quiet passages to change.

3. **The TUI keymap changed.** `c` now cycles the channel mode; `C` calibrates.
   The help overlay scrolls with the mouse wheel because the key table no longer
   fits a 30-row terminal.

4. **The product invariant changed.** "Exactly one output at a time" is retired:
   one live sender per session remains, two sessions claiming one receiver is
   still a hard refusal, and simultaneous multi-output is now first-class through
   sync groups. If you relied on the old single-output rule as a guarantee, read
   the new one in `docs/glossary.md` before upgrading.

5. **A first-run audio check will execute on the first start** unless you have
   already run one. It takes about three seconds of real pipeline work, runs in
   the background after capture is live, and applies its verdict only to settings
   you have not touched. To skip it, run the audio check once manually before
   upgrading.

6. **The installer changed shape.** `packaging/omarchy/install.sh` now defaults
   to prebuilt binaries and compiles only with `--source` or `--checkout`.
   Re-running it updates.

---

## Notable fixes

- The desktop installer printed *"Installed on-air-desktop"* and exited 0 when
  the underlying `pacman -U` **had failed**. A failed install is now reported as
  failed.
- The one-line installer could silently fall back to `SKIP` checksums if a
  release recipe's arrays were ever renamed — `sed` exits 0 whether or not it
  matched anything. It now refuses to install unverified binaries.
- `bluetoothctl` was called unbounded on the `/api/outputs` request path; on a
  host without a working system bus that hung the request forever. Bounded now.
- The help overlay silently dropped its bottom rows as features accumulated,
  while claiming to fit a 30-row terminal. It scrolls now.
- Two features had been bound to the same `c` key, silently disabling one.

---

## Known limitations

- **No latency figure is calibrated.** See above.
- **Multi-speaker sync has not been verified acoustically.** The infrastructure
  is built and tested against synthetic known delays; whether the speakers
  actually sound aligned is unverified. The calibration harness (B10) and a
  listening pass (B11) are the steps that close this.
- macOS AirPlay goes through the system route picker and cannot be driven from
  the phone. Windows Bluetooth device detection still has gaps.
- Some release channels await published artifacts (see the checklist).

---

## Merge plan

PRs land in waves so that each wave is reviewable and independently revertable.
`#23` is the integration surface for the seven core PRs; merging it and closing
those seven is the intended outcome, not a merge of all eight.

| Wave | PR | What | Notes |
| --- | --- | --- | --- |
| 1 — core | **#23** | Core audio stack | Supersedes and should retire **#11, #16, #17, #18, #19, #21, #22**. 392 tests, clippy clean. |
| 2 — core features | **#24** | Channel modes + structural channel registry | Green 10/10. |
| | **#26** | Acoustic calibration harness | Based on #23. |
| | **#28** | TUI keymap guard + help scroll | Based on #26. Overlaps #26 — see note. |
| 3 — tests | **#25** | e2e across the new API surface | 604 tests, 7 discrimination proofs. |
| 4 — UI | **#15** | Drag-only patchbay, media widget, toasts | QML; needs a manual Omarchy click-through. |
| 5 — packaging | **#12** | One-line installer + AUR guidelines | Needs a published release to exercise the prebuilt path. |
| | **#20** | Desktop installer test coverage | Includes the failed-install fix. |
| | **#27** | `pnpm dlx` instead of `npx` | One line. |
| 6 — docs | **#13** | Roadmap action plan | |
| | **#14** | Project website + build log | |

**#26 / #28 overlap.** #28 was built on #26 to fix bugs #26 introduced, so the
fix set now exists on both. Identical content merges without conflict, but if you
prefer one PR, keep #28 (it also carries the
`every_action_in_the_help_table_is_reachable` guard) and drop the duplicated
commits from #26.

**Recommended order within wave 2:** #24 → #26 → #28, since each is based on the
previous.

---

## Release checklist

Do these in order. **Nothing here is done yet.**

### Before tagging

- [ ] All waves merged to `main`, or the outstanding ones consciously deferred.
- [ ] `main` CI green on the release commit.
- [ ] Pending decisions in [DECISIONS-PENDING](DECISIONS-PENDING.md) answered —
      especially the `process-wide single-output` invariant (it is in the upgrade
      notes above and would be wrong to publish unconfirmed).
- [ ] Manual QA on a real desktop:
  - [ ] Omarchy: drag a cable, right-click to delete it, mixes and groups still work.
  - [ ] `preview.png` regenerated on a machine with a Nerd Font — the committed
        one renders icons as tofu boxes.
  - [ ] MPRIS picked up by Waybar's `mpris` module and `omarchy.media`.
  - [ ] Multi-speaker listening pass (B11) — at least two transports together.
- [ ] Run the acoustic calibration harness on real hardware and, if the measured
      values differ from the defaults, decide whether to fold them in or leave
      them as user-corrected.
- [ ] `docs/REVIEW-2026-10-19.md` from the independent reviewer read and its
      findings triaged.

### Version bump

- [ ] `packages/core/Cargo.toml` → `0.2.0` (source of truth; `packaging/omarchy/test.sh`
      asserts every recipe pins the same version).
- [ ] `Cargo.lock` regenerated.
- [ ] `package.json` files in the workspace.
- [ ] `apps/mobile/app.json` (Expo version + `versionCode`/`buildNumber`).
- [ ] `apps/desktop/src-tauri/tauri.conf.json`.
- [ ] Verify: `sh packaging/omarchy/test.sh` → `all ok`.

### Tag and release

- [ ] `CHANGELOG.md` `[Unreleased]` folded into `## [0.2.0] - <date>`.
- [ ] Tag `v0.2.0`, annotated, on the release commit.
- [ ] Publish the GitHub release with notes from this document.

### Build artifacts

- [ ] `on-air-core-serve-linux-x86_64`, `on-air-tui-linux-x86_64`
- [ ] `on-air-core-serve-linux-aarch64`, `on-air-tui-linux-aarch64`
- [ ] `SHA256SUMS` covering every asset — **the one-line installer refuses to
      run without a complete, well-formed set.**
- [ ] macOS `.dmg` for the tray app.
- [ ] Windows NSIS and MSI, x64 + arm64.
- [ ] `on-air-desktop_<ver>_amd64.deb` and the arm64 equivalent.
- [ ] Android debug/release APK (CI already builds the debug one).
- [ ] iOS simulator build (CI already exercises this).

### Distribute

- [ ] **AUR**: run `packaging/update-checksums.sh` to fill the pins in
      `on-air-bin`, `on-air-desktop-bin`, `on-air`, then regenerate each
      `.SRCINFO` with `srcinfo.sh`. Submit when the AUR registration window
      reopens.
- [ ] **Homebrew**: fill `Casks/on-air.rb` (`sha256 :no_check` is currently a
      placeholder) and `Formula/on-air-core.rb` (add a `revision:` pin).
- [ ] **winget**: replace the `InstallerSha256: 000…0` placeholders in
      `packaging/winget/*.yaml`, then submit to `microsoft/winget-pkgs`.
- [ ] **APT / RPM**: host the `.deb` and `.rpm`, or record that they remain
      build-from-source only.
- [ ] **Flatpak**: build and decide whether to submit to Flathub.
- [ ] **Nix**: update `flake.nix` / `default.nix`.
- [ ] **Omarchy**: verify `packaging/omarchy/install.sh` end to end on a clean
      Omarchy 4 box using the **prebuilt** path — this is the only thing that
      exercises D3 and it cannot be tested before a release exists.

### After

- [ ] `docs/history/` entry for the release.
- [ ] Website build log updated; `website/` install section no longer says
      "awaiting v0.2.0".
- [ ] Close the superseded PRs (#11, #16, #17, #18, #19, #21, #22 if #23 landed).
- [ ] Delete merged remote branches.
- [ ] Consider the shared `CARGO_TARGET_DIR` decision if CI build times matter.
