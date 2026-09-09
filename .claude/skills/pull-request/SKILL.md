---
name: pull-request
description: >
  Take a change from working tree to a merge-ready pull request, then keep
  iterating until the CI checks and AI reviewers (CodeRabbit, cubic) all pass.
  Runs the local quality gate, opens or updates the PR following this repo's PR
  hygiene rules, and — for UI changes — verifies the GUI headlessly and attaches
  screenshots. Use when asked to "open a PR", "ship this", "get this through
  review", or "fix the review comments". Invoke with /pull-request.
metadata:
  version: "1.0.0"
---

# Pull request: open, verify, and pass review

End-to-end flow for turning the current change into a pull request that passes
this repo's automated quality gate, fixing review feedback in a loop until green.
When green, it also flags changes worth a blog post and offers to file a blog
issue (Phase 6).

The quality gate here is **not** classic GitHub Actions (there are none yet). It
is:

1. **Local gate** you run before pushing: `cargo fmt`, `cargo clippy`, `cargo build`, `cargo test`.
2. **AI reviewers** that run as PR status checks and post review threads:
   - `CodeRabbit` (`coderabbitai[bot]`)
   - `cubic · AI code reviewer` (`cubic-dev-ai[bot]`)

"Green" = all `gh pr checks` pass **and** no unresolved review threads from those
bots. The loop below drives toward that state.

## When to use

- The user wants a change opened as a PR and taken through to merge-ready.
- The user wants existing review comments addressed and pushed until checks pass.
- Any user-visible (UI) change that should ship with visual evidence.

## Prerequisites (check, don't assume)

- `gh auth status` is logged in. If not, stop and ask the user to `gh auth login`.
- Working tree changes are the ones intended for this PR (`git status`, `git diff`).
- For UI verification: `script/ui-run.sh setup` has been run once and `xdotool`
  is installed (see `docs/agent-ui-verification.md`). If they're missing and the
  change is UI-facing, ask the user to install them rather than skipping evidence.

The two helper scripts live next to this file:

- `scripts/review-status.sh [PR#]` — prints checks + unresolved AI threads, exits
  `0` green / `1` blocked / `2` error.
- `scripts/attach-screenshots.sh <slug> <png...>` — publishes images to the
  `pr-assets` branch and prints Markdown embeds.

---

## Phase 1 — Branch and local quality gate

1. **Never commit on `develop` or `main`.** Check `git rev-parse --abbrev-ref HEAD`.
   If on a protected branch, create a topic branch first (descriptive, e.g.
   `git switch -c fix-project-panel-crash`).
2. Run the local gate and fix everything it reports **before** pushing — this is
   cheaper than a review round-trip:
   ```bash
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo build
   cargo test
   ```
   (For GUI code, build with the linker path from `ui-run.sh setup`:
   `RUSTFLAGS="-L $HOME/.local/devlibs" cargo build --features gui --bin nohrs`.)
3. Follow the repo's Rust guidelines (CLAUDE.md): no `unwrap()`/panics, no silent
   `let _ =` on fallible calls, propagate errors with `?`. The AI reviewers flag
   these, so getting them right now saves a loop iteration.

## Phase 2 — UI verification (only if the change is user-visible)

If the diff touches layout, panels, navigation, previews, or anything rendered,
verify it for real and capture evidence. Full workflow: `docs/agent-ui-verification.md`.

```bash
# Build, launch, screenshot the relevant states.
RUSTFLAGS="-L $HOME/.local/devlibs" cargo build --features gui --bin nohrs
./script/ui-run.sh launch                      # prints WINDOW / DISPLAY / PID
./script/ui-run.sh shot /tmp/before.png        # then Read the PNG to confirm state
# Drive the UI with xdotool, re-shoot after each step (coordinates are absolute):
DISP=$(./script/ui-run.sh display); WIN=$(./script/ui-run.sh win)
DISPLAY=$DISP xdotool windowactivate "$WIN" mousemove X Y sleep 0.4 click 1 sleep 1.2
./script/ui-run.sh shot /tmp/after.png         # Read it; verify the expected change
./script/ui-run.sh stop
```

**Read each PNG back yourself** and confirm the change actually happened — a clean
build is not evidence. Keep the PNGs that best show before/after for the PR. Mind
the gotchas in the doc (`pkill -x nohrs`, find window by PID, black-first-frame).

## Phase 3 — Open or update the PR

1. Commit with a clear message and push the branch:
   ```bash
   git add -A && git commit -m "<imperative summary>"
   git push -u origin HEAD
   ```
   End commit messages with the trailer:
   `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
2. If a PR for this branch already exists (`gh pr view`), update it instead of
   opening a new one.
3. **Attach screenshots** (UI changes only) before writing the body, so you have
   the embed URLs:
   ```bash
   .claude/skills/pull-request/scripts/attach-screenshots.sh ui-change /tmp/before.png /tmp/after.png
   ```
   Paste the printed `![...](...)` lines into the PR body under a `## Verification`
   heading. (Images go to the `pr-assets` branch, so they never appear in the code
   diff the AI reviewers read.)
4. Open the PR honoring **PR hygiene** (CLAUDE.md):
   - Imperative, correctly-capitalized title, no conventional-commit prefix
     (`fix:`/`feat:`), no trailing punctuation. Optionally prefix with the crate
     name when one crate is the clear scope (e.g. `git_ui: Add history view`).
   - Body ends with a `Release Notes:` section — blank line after the heading,
     one bullet: `- Added ...` / `- Fixed ...` / `- Improved ...`, or `- N/A` for
     docs-only / non-user-facing changes.

   ```bash
   gh pr create --base develop --title "<title>" --body "$(cat <<'EOF'
   <what changed and why>

   ## Verification
   <screenshot embeds + how it was verified, or "N/A">

   Release Notes:

   - <Added/Fixed/Improved ...  or  N/A>
   EOF
   )"
   ```

## Phase 4 — Review loop (drive to green)

Repeat until `review-status.sh` exits `0`, capped at **3 fix iterations** (see
Phase 5 for what to do if still blocked).

1. **Wait for the gate to settle**, then read it:
   ```bash
   gh pr checks --watch --interval 30      # blocks until checks finish (or fail)
   .claude/skills/pull-request/scripts/review-status.sh
   ```
   - Exit `0` → green. Go to "Done".
   - Exit `1` → blocked. The output lists failing checks and every unresolved
     AI review thread (`[bot] path:line: comment`). Continue.
   - Exit `2` → query error (no PR / auth). Resolve and retry.
2. **Address every unresolved thread on its merits.** For each:
   - If the comment is correct, fix the code. Re-run the relevant part of the
     **local gate** (Phase 1) so you don't re-break clippy/tests.
   - If it's a false positive or out of scope, reply on the thread explaining why,
     rather than silently ignoring it. Use:
     ```bash
     gh pr comment <PR#> --body "..."        # general reply
     # or reply inline to a specific review comment via the API if needed.
     ```
   - Do **not** mark threads resolved on the author's behalf without a real fix or
     a clear justification — that defeats the gate.
3. If a UI behavior changed during fixes, **re-run Phase 2** and refresh the
   screenshots (run `attach-screenshots.sh` again; update the body embeds).
4. Commit and push the fixes (this re-triggers the AI reviewers):
   ```bash
   git add -A && git commit -m "Address review feedback" && git push
   ```
5. Go back to step 1.

## Phase 5 — Done or escalate

- **Green:** Tell the user the PR is passing — link (`gh pr view --web` URL),
  a one-line summary of what changed, and what was verified (with the screenshot
  links for UI work). Do **not** merge unless the user explicitly asks.
- **Still blocked after 3 iterations:** Stop looping. Post a concise summary to
  the user: which checks/threads remain, what you tried, and the specific decision
  or access you need from them. Don't keep pushing speculative fixes — repeated
  no-op pushes spam the reviewers and burn CI.

## Phase 6 — Blog-worthy? (propose, don't auto-write)

Once the PR is green and reported (Phase 5 "Green"), judge whether the change
holds a lesson worth a blog post — then **propose** it. Don't write the article
and don't file anything without the user's go-ahead. Skip this entirely on the
escalate path (a still-blocked PR isn't ready to write up).

**The bar (propose only if it clears it).** This repo's blog is for design
stories and instructive findings (see #154, #157), not changelog entries.
Propose when the PR contains one of:

- A non-obvious **design decision / architectural shift** with real tradeoffs
  (e.g. #154 — removing tokio for consistency, not speed).
- A **subtle bug** whose root cause generalizes — teaches something beyond this
  codebase.
- A **surprising discovery** about a tool, library, or platform behavior (GPUI
  quirks, build/runtime gotchas).

Do **not** propose for routine feature adds, mechanical refactors, dependency
bumps, docs-only changes, or trivial fixes. One proposal max — if the user
declines, drop it (don't nag on later pushes).

**If it clears the bar:** give the user a 1–2 line pitch (the angle/thesis, not
"added X") and ask if they want a blog issue filed. The blog engine (#99) isn't
built yet, so blog topics live as GitHub issues following the #154/#157
convention.

**On approval**, file the issue while the diff is fresh — the `path:line`
references are the most valuable part to capture now:

```bash
gh issue create \
  --title "Blog: <topic>" \
  --label "type:docs,area:docs,area:web" \
  --body "$(cat <<'EOF'
関連: PR #<this-pr> / #99 (blog エンジン) / <related issues>

## 目標
<the design story / lesson — what other developers learn, not "got faster">

## 記事の角度（draft）
- <thesis / angle>

## アウトライン（draft）
- [ ] <section>
- [ ] <section>

## 引用したい実コード（PR #<this-pr>）
- `crates/.../foo.rs:NN` — <why this line matters>

## 完了条件
- <topic> の記事が blog で公開される

## 参照
- <docs/ADR links>
EOF
)"
```

Match the existing issues' Japanese section headings. Title may be Japanese.

---

## Reference

| Need | Command |
| --- | --- |
| Current branch | `git rev-parse --abbrev-ref HEAD` |
| Local gate | `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` |
| GUI build | `RUSTFLAGS="-L $HOME/.local/devlibs" cargo build --features gui --bin nohrs` |
| Launch / shot / stop GUI | `./script/ui-run.sh {launch,shot <png>,stop}` |
| Watch checks | `gh pr checks --watch --interval 30` |
| Gate report | `.claude/skills/pull-request/scripts/review-status.sh [PR#]` |
| Publish screenshots | `.claude/skills/pull-request/scripts/attach-screenshots.sh <slug> <png...>` |
| PR review threads (raw) | `gh api repos/{owner}/{repo}/pulls/{n}/comments` |
| Blog issue convention | `gh issue list --label type:docs` (template: #154, #157) |

**AI reviewer bots** treated as the gate: `coderabbitai[bot]`, `cubic-dev-ai[bot]`.
Override with the `PR_REVIEW_BOTS` env var (space-separated) if the set changes.
