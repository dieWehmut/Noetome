# Spec Compliance

- ✅ Spec compliant for the Task 3 implementation gate. Reviewed exact package `b2c0cba117993a5d755b7e69692d37a561291aba..634d6382b687b706b7bd34396ee5f386c57ef67e` (three commits, 25 files, 712 additions / four removals), using the official `subagent-driven-development/task-reviewer-prompt.md` template, bound task brief, implementation report and appended user Task 3 requirements.
- ✅ `crates/noetome-desktop/tauri.conf.json:24` removes native decorations while preserving the main label, 640px minimum width and existing window metadata. `capabilities/main.json:5` restricts the capability to main; lines 7–13 grant exactly the five required window commands and event listen/unlisten, without default/broad file/network/process permissions.
- ✅ `apps/desktop/src/AppMenuBar.tsx:162` composes the new controls in the existing menu header; `:164` places the alert outside its height. `styles.css:27` makes the existing 36px row sticky; `:29`–`:37` keep drag space and fixed-size controls in the same row with visible button focus inherited from `:45`. Existing navigation/menu handlers are preserved by the diff.
- ✅ `WindowControls.tsx:74` limits mouse handling to a direct left click on the empty drag sibling. Lines 78–82 issue one detail-2 maximize toggle and one detail-1 drag; neither the header nor controls/menu ancestors carry a drag attribute. Actual Tauri 2.12.5 `src/window/scripts/drag.js:77`–`:108` uses the same Windows click-detail semantics and only handles elements carrying `data-tauri-drag-region`; it cannot duplicate this manual handler.
- ✅ `WindowControls.tsx:8` guards native-window lookup with actual API `isTauri`; lines 88–95 use named native HTML buttons and disable them without the host. Unknown maximize state is disabled. Lines 25–29 suppress stale state-query completions, lines 37–47 handle pending-listener cleanup/unlisten, and lines 51–71 serialize window operations and surface rejection. No destroy/hide/exit shortcut or new desktop Rust API was added.
- ✅ Named close integration risk inspected outside the diff: local JS API 2.12.4 `window.js:1090` invokes `plugin:window|close` for the current window, and `:1096` documents the different forced-destroy semantics. Existing `crates/noetome-desktop/src/main.rs:193`–`:234` still resolves the persisted preference, permits exit, or prevents close and hides the original main window only with a tray recovery path. The builder retains `.on_window_event(handle_window_event)` at `:256` and builds from unchanged window configuration at `:273`. Ordinary minimize is untouched.
- ⚠️ Installed native acceptance is intentionally root-owned and pending: actual NSIS build/install and program fingerprint, complete top-of-window screenshot plus OS caption/style observation, actual drag/double click/minimize/taskbar/maximize/restore, close/tray/exit preference and real existing-menu IPC. The browser and controlled IPC proofs do not establish any of these OS behaviors. `doc/execution/B04-T03_REVIEW_PLAN.md:140` and `:144` explicitly assign this next gate to root; the implementation gate does not complete the overall task or authorize claiming native acceptance.
- ⚠️ Root's final `doc/validation/B04-T03-ui-single-row.md`, DESIGN synchronization, full regression and ledger closure remain outside this implementer's scoped ownership; their absence from this diff is not a missing implementation finding.

# Strengths

- The new 99-line `WindowControls.tsx` owns only native-window state/operations and communicates failure through one stable `onError` prop (`:7`–`:16`); no unrelated App.tsx, Rust preference, source, API, ledger or model changes enter the patch.
- The observed ten retained checks exercise the real React DOM/CSS and locked JS API with a clearly marked IPC boundary, including 640px geometry, actual page scrolling, keyboard activation, menu/sidebar retention, exact current-window commands, drag exclusion, maximize state/resize, one double-click toggle, operation rejection/retry and failed post-toggle query recovery. The verifier `doc/validation/assets/B04-T03-ui-implementation-check.mjs:60` explicitly states its OS limitation, and `:158` records `native_os_proof=false`.
- Retained evidence independently parsed during this review shows initial RED 1 passed / 8 failed, first GREEN 9/9, state RED 9 passed / 1 failed, and final 10/10. Historical WindowControls hashes differ as expected; all five final recorded source hashes match actual current file bytes. All 19 explicitly named proof assets were hashed. Final TypeScript/Vite logs are present and contain no warning/error output; Vite reports 24 modules and a successful build. Reported process exit codes were not independently rerun or reconstructed.
- Viewed actual final 640px and controlled-error PNGs: all three controls stay within one 36px bar, and the alert is below it. These remain browser screenshots. Their SHA256 values are saved in the evidence JSON.

# Issues

## Critical (Must Fix)

- None found in this task scope.

## Important (Should Fix)

- None found in this task scope.

## Minor (Nice to Have)

- `apps/desktop/src/WindowControls.tsx:25`: a successful resize-driven state refresh calls only `setMaximized(value)`, so after a failed query at `:29` the controls can recover while the old alert continues to say the state read failed and recommends retry/reopening. The retained tenth check (`B04-T03-ui-implementation-check.mjs:143`–`:152`) proves control recovery but does not clear/inspect that alert. Consider tracking which operation owns the displayed error and clearing only a recovered state-query error; a state refresh should not erase a separate minimize/close/drag failure. This does not block native acceptance or imply that any OS operation already passed.

# Assessment

**Task quality:** Approved.

**Reasoning:** The scoped implementation preserves the existing menu/close behavior and uses the actual current-window APIs with minimal permissions, isolated drag handling, truthful host absence/error states and source-bound behavioral evidence. No blocking correctness or structural defect was found; root may proceed to the required build/install/native gate, then full regression and branch review.

# Actual Review Checks and Boundaries

- Read the exact supplied diff once as the changed-file view. Its tool output elided the verifier/evidence middle; reread the specifically named retained verifier, final stdout and final TypeScript/Vite logs at their stated paths to recover evidence. Changed product source was not reread as an alternate code view; only byte hashes were computed.
- Named outside-diff checks: locked JS API/permission availability and event lifecycle (`node_modules/@tauri-apps/api/{package.json,core.js,window.js,event.js}`, generated desktop permission schema); duplicate drag/maximize risk (exact Tauri 2.12.5 drag.js); close preference and config-consumption integration (one focused main.rs view, with continuation of output cut mid-function). No broader code crawl or Git invocation.
- Ran only read-only file/rg inspection, SHA256 and retained-results audit, plus local image viewing. No fresh behavioral probe was needed: the retained checks and exact native API implementation answer the specific scoped risks. No test suite, build, package install, Cargo/npm, CUA, network/model/private input, Git/index/branch mutation, binary mutation, delegation or cleanup.
- Only SDK `task-3-review.md`, `task-3-review-evidence.ps1` and `task-3-review-evidence.json` were written. Evidence identifies hashes and observations, not invented fresh test passes.
