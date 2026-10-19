# Verification ledger — 2026-10-19 architecture review

Companion to `docs/REVIEW-2026-10-19.md`. Every load-bearing claim in the review
was re-checked mechanically against the branch heads listed in the review's §10
(2026-10-19). Commands and outcomes:

| Claim | Check | Outcome |
|---|---|---|
| 25 content commits of #23 at identical shas to the individual PRs | `git merge-base --is-ancestor <sha> origin/<pr-branch>` for all 25 | 25/25 in-branch |
| #23 history shape | `git log --oneline origin/main..origin/integration/core-stack` | 36 commits: 25 content + 7 merge + 4 integration-only (`87cee8b`, `04474cc`, `2e6dbde`, `8aa3157`) |
| #23 merge-commit consequence | `git merge-base --is-ancestor origin/<pr> origin/integration/core-stack` | NOT ancestor for all 8 — no auto-close, diffs do not empty (each PR has 1–2 uncarried tip commits) |
| Tip deltas subsumed | `git show e4f1f0e` vs `87cee8b`; `git show 7e70967` vs stack blobs; `git diff` #19 tip files vs stack | #11 bound subsumed by `87cee8b`; #17 fixtures present except `queue.current?.` hunk (dropped by #25's `5b548b6`); #19 byte-equal |
| fmt commits duplicated | `git show 8aa3157 \| git patch-id --stable` vs `6323b33` | identical patch-id (`7b7352ce…`) |
| #26/#28 duplicate | `git diff --stat origin/feat/acoustic-calibration origin/test/keymap-collision`; `git merge-tree --write-tree` | empty diff (byte-identical trees); 0 conflicts |
| Shared commits #26/#28 | `git merge-base --is-ancestor` for `a54734b`, `dc5337e`, `33f58c9` | in both branches |
| `keys: "c"` display bug | `git show <branch>:apps/tui/src/keys.rs` | `keys: "c"` at lines 72 (CycleChannel) and 132 (Calibrate) on both #26 and #28; `action_for` `'c'`/`'C'` at 238–239; guard `every_action_in_the_help_table_is_reachable` checks reachability only; `view/overlay.rs::help_overlay` prints `binding.keys` verbatim |
| #23 body stale re channel-modes | `gh pr view 23` | "deliberately left out of this branch" contradicted by merge `04474cc`; 14-row resolution table present |
| #17 body copies #18's | `gh pr view 17/18` | first ~100 lines identical; wrong subject matter for #17 |
| #20 harness facts | `gh pr view 20`; `git grep` on branch | 28 tests (`def test_`), `DEFAULT_MOCKS` includes `npm`, `test_missing_pnpm_and_npm_stop_before_building` at `test-install.py:378`, suite runs at `test.sh:106` **and** `ci.yml:114` (with `ci.yml:112` running `test.sh`), `bash -n packaging/install.sh` at `test.sh:105` and `:182`; tip `047115c` fixes the PR body's flagged `install_as_root` swallow |
| Byte-identical shared files across #23/#25/#26/#28 | `git rev-parse <branch>:<path>` blob hashes | equal for `transport_exclusivity.rs`, `timeline.rs`, `dsp/channel.rs`, `alloc_guard.rs`, `sync/lifecycle.rs`, `api_sync.rs`; **not** for `apps/tui/src/update.rs` (calibration commits) |
| Conflict matrix (§5) | `git merge-tree --write-tree --name-only` over all 153 pairs, counting `CONFLICT` lines | matches the table (30/23/21/19/17/16/15/14/13/11/…); `channel-modes ↔ stereo-meter-quality = 15` added; #26↔#28 = 0 |
| Duplication spot-checks | `git grep`/`git diff` on the stack tree | `DspQuality` double-def then collapsed (58→33 lines, single enum in `packages/api`); `TRIM_LIMIT_MS = 10_000` ×3; meter twins = same skeleton + DOM/RN delta (47/27 lines); `service_paused` three different sentences; `METER_FLOOR_DB = -60` in `levels.ts` and `tui/model.rs`; quality labels in `audioPerformance.ts` + `cli.rs`, none iterate the wire `profiles`; move-dance in 3 clients (`useSyncGroups.ts`, `sync-groups.tsx:71`, `tui/client.rs`); group-name 1..=64 in 3 clients + `sync/lifecycle.rs` |
| #25 pins | `git show origin/feat/e2e-coverage:e2e/desktop/audio-surface.spec.ts` | 618 lines; exact prose at :437 and :488; WAV byte-equality at :524; `{trim_ms: 250, effective_latency_ms: 1250}` at :133; cited core test names all exist |
| #23's stale "auto-close" plan corrected | ancestry + `git cherry` | see §6 of the review |
| PR diffstats (§10) | `gh pr list --json additions,deletions,changedFiles` | all rows match; #26 corrected to +20942/−676 (equal to #28, same tree) |

Unverified/residual: PR-body narrative claims that match their diffs (e.g. #25's
"discrimination proof" red/green runs) are taken as reported by the author; not
re-run (this is a reading review — no builds or tests were executed).
