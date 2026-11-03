# B04U-T01 implementation report

Implemented and committed `95f3766` (`feat(desktop): replace breadcrumb with Codex-style application menu`) on `codex/b04u-t01-codex-header`.

Owned files: `apps/desktop/src/App.tsx`, `apps/desktop/src/styles.css`, new `apps/desktop/src/AppMenuBar.tsx`. No unrelated API/docs/devlog files or user deletions were staged, discarded, or committed.

The 36px full-width neutral menu bar replaces the breadcrumb and its version tag. Back/forward use actual bounded page history (64 entries), avoid adjacent duplicates, and truncate forward entries on new navigation. All existing sidebar, brand and in-content page actions use this history. The sidebar control exposes its actual hidden state through aria-controls/expanded. File/Edit/View/Help use existing page/settings/help/version actions; Edit selects the current main content and writes only selected main text with the supported clipboard API, reporting failures. No clipboard reads, IPC additions or new dependencies.

Menu keyboard support uses native button Enter/Space, roving menubar focus, Left/Right switching, Up/Down/Home/End item navigation, Escape focus restoration, normal Tab exit and outside pointer/focus dismissal. Settings and CLI headings are focus targets only after their page renders. Dropdown coordinates clamp to viewport edges; help/version actions are disabled while the existing preview is running. Reviewed the complete frontend diff and new component for scope, actual actions, hidden sidebar focus and disabled-state styling.

Actual verification (all exit 0):

- `C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin/node.exe node_modules/typescript/bin/tsc --project apps/desktop/tsconfig.json --noEmit` (twice; final after label changes).
- The same absolute Node executable with `node_modules/vite/bin/vite.js build --config apps/desktop/vite.config.ts`, Node bin added to this command's process PATH. Vite transformed 19 modules and emitted production HTML/CSS/JS successfully.
- `git diff --check` and focused `git diff --check -- apps/desktop/src/App.tsx apps/desktop/src/styles.css`; final `git diff --cached --check` over the three staged frontend files.

Remaining controller checks: browser history/sidebar/keyboard/clipboard and 360/640/1200px behavior with screenshots; installed native IPC/window-settings/help/version; fresh independent review, full repository checks, devlog/docs and push. These are not claimed passed by this implementation report. Root owns the shared browser automation session and Vite dev process; neither was accessed or stopped here.

## Screenshot spacing/color polish

Controller requested a style-only refinement after actual screenshot inspection and initial approved review. Commit `8d7f2c6` changes only the topbar selectors in `apps/desktop/src/styles.css`: background #f3f3f3; 3px horizontal padding; 9px group gap; 34px navigation button width with 0 gap; 12px menu gap; icon/menu color #858585. Height36px/font14px/icons18px and focus behavior remain unchanged. `git diff --check -- apps/desktop/src/styles.css` passed (exit0). No new TypeScript or broad tests were run for this CSS change.

Focused Vite command used the same absolute Node executable/PATH as above; exit0. Raw output:

```text
vite v8.3.5 building client environment for production...
transforming...
✓ 19 modules transformed.
rendering chunks...
computing gzip size...
apps/desktop/dist/index.html                   0.50 kB │ gzip:  0.31 kB
apps/desktop/dist/assets/index-onuZfCLl.css   14.81 kB │ gzip:  3.94 kB
apps/desktop/dist/assets/index-BjhUyx_f.js   242.27 kB │ gzip: 75.74 kB

✓ built in 121ms

```
