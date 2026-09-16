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
  version: "1.1.0"
---

# Pull request: open, verify, and pass review

End-to-end flow for turning the current change into a pull request that passes
this repo's automated quality gate, fixing review feedback in a loop until green.
When green, it also flags changes worth a blog post and offers to file a blog
issue (Phase 6).

The quality gate has three layers:

1. **Local gate** you run before pushing: `cargo fmt`, `cargo clippy`, `cargo build`, `cargo test`.
2. **GitHub Actions** — `ci.yml` (`fmt`, `typos`, `config schema`, `clippy`,
   `test`, `build`, `msrv`, `cargo-deny`, coverage with `--fail-under`
   thresholds), and `web.yml` for `web/**` changes. The two are mirror images:
   `ci.yml` ignores `web/**`, `docs/**` and `**.md`, `web.yml` runs only for
   `web/**`, so a docs-only PR legitimately shows very few checks.
   `machete.yml` is **not** part of the gate — it is advisory and runs weekly
   on a schedule, never on a pull request.
3. **AI reviewers** that run as PR status checks and post review threads:
   - `CodeRabbit` (`coderabbitai[bot]`)
   - `cubic · AI code reviewer` (`cubic-dev-ai[bot]`)

"Green" = every check passes **and** no unresolved review threads from those
bots. The loop below drives toward that state.

## When to use

- The user wants a change opened as a PR and taken through to merge-ready.
- The user wants existing review comments addressed and pushed until checks pass.
- Any user-visible (UI) change that should ship with visual evidence.

## Environment — `gh` CLI or GitHub MCP tools

This skill runs in two environments that reach GitHub in incompatible ways.
**Determine which one you are in before Phase 3**, because roughly half the
commands below do not exist in the other:

```bash
command -v gh      # no output => remote environment, gh is unavailable
```

- **Local / terminal sessions** — `gh` is installed. Use the `gh` commands as
  written; `review-status.sh` works and is the fastest way to read the gate.
- **Claude Code on the web / remote sessions** — `gh` is **not installed** and
  there is no direct GitHub API access. Use the `mcp__github__*` tools instead.
  `review-status.sh` shells out to `gh`, so it cannot run there; reproduce it
  with the MCP calls below (Phase 4 step 1 spells out the sequence).

| Need | `gh` | MCP tool |
| --- | --- | --- |
| Find the PR for a branch | `gh pr view` | `list_pull_requests` (`head: "<owner>:<branch>"`) |
| Read a PR | `gh pr view --json …` | `pull_request_read` (`method: "get"`) |
| PR diff | `gh pr diff` | `pull_request_read` (`method: "get_diff"`) |
| Check status | `gh pr checks` | `pull_request_read` (`method: "get_check_runs"`) |
| Failing job logs | `gh run view --log-failed` | `get_job_logs` (`run_id` + `failed_only: true`, `return_content: true`) |
| Review threads | `gh api graphql …` | `pull_request_read` (`method: "get_review_comments"`) |
| Open a PR | `gh pr create` | `create_pull_request` |
| Edit PR title/body | `gh pr edit` | `update_pull_request` |
| Comment on a PR | `gh pr comment` | `add_issue_comment` |
| Reply in a review thread | `gh api …/replies` | `add_reply_to_pull_request_comment` |
| Resolve a thread | (graphql) | `resolve_review_thread` |
| File an issue | `gh issue create` | `issue_write` (`method: "create"`) |

There is no MCP equivalent of `gh pr checks --watch`: poll
`pull_request_read` (`method: "get_check_runs"`) instead, leaving ~30s between
polls. Do not `sleep` in a loop to wait — if PR events are subscribed, they wake
the session on their own.

`git` itself behaves identically in both environments, so the commit and push
steps never change.

## Prerequisites (check, don't assume)

- **Local only:** `gh auth status` is logged in. If not, stop and ask the user to
  `gh auth login`. In the remote environment there is nothing to check — the MCP
  tools carry their own auth, and a failure there means the repository is out of
  scope, not that a login is missing.
- Working tree changes are the ones intended for this PR (`git status`, `git diff`).
- For UI verification: `script/ui-run.sh setup` has been run once and `xdotool`
  is installed (see `docs/agent-ui-verification.md`). If they're missing and the
  change is UI-facing, ask the user to install them rather than skipping evidence.

The two helper scripts live next to this file:

- `scripts/review-status.sh [PR#]` — prints checks + unresolved AI threads, exits
  `0` green / `1` blocked / `2` error. **Requires `gh`**; exits `2` with a pointer
  to the MCP path when it is missing.
- `scripts/attach-screenshots.sh <slug> <png...>` — publishes images to the
  `pr-assets` branch and prints Markdown embeds. Pure `git`, so it works in both
  environments.

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
2. If a PR for this branch already exists, update it instead of opening a new
   one — `gh pr view`, or `list_pull_requests` with
   `head: "<owner>:<branch>"` and `state: "open"`.
3. **Attach screenshots** (UI changes only) before writing the body, so you have
   the embed URLs:
   ```bash
   .claude/skills/pull-request/scripts/attach-screenshots.sh ui-change /tmp/before.png /tmp/after.png
   ```
   Paste the printed `![...](...)` lines into the PR body under the `## Testing`
   heading. (Images go to the `pr-assets` branch, so they never appear in the code
   diff the AI reviewers read.)
4. Open the PR honoring **PR hygiene** (CLAUDE.md):
   - Imperative, correctly-capitalized title, no conventional-commit prefix
     (`fix:`/`feat:`), no trailing punctuation. Optionally prefix with the crate
     name when one crate is the clear scope (e.g. `git_ui: Add history view`).
   - Body ends with a `Release Notes:` section — blank line after the heading,
     one bullet: `- Added ...` / `- Fixed ...` / `- Improved ...`, or `- N/A` for
     docs-only / non-user-facing changes.

   The base branch is `develop`, not `main`.

   ```bash
   gh pr create --base develop --title "<title>" --body "$(cat <<'EOF'
   ## Summary

   <what changed and why>

   ## Changes

   - <notable change>

   ## Testing

   - [x] `cargo fmt --all --check`
   - [x] `cargo clippy --all-targets -- -D warnings`
   - [x] `cargo test`

   <screenshot embeds, or "N/A" for non-UI changes>

   Release Notes:

   - <Added/Fixed/Improved ...  or  N/A>
   EOF
   )"
   ```

   Without `gh`, the same PR via `create_pull_request`:

   ```jsonc
   {
     "owner": "noh-rs", "repo": "nohrs",
     "base": "develop", "head": "<branch>",
     "title": "<title>",
     "body": "## Summary\n\n…\n\n## Changes\n\n- …\n\n## Testing\n\n- [x] …\n\nRelease Notes:\n\n- <…>"
   }
   ```

   `.github/PULL_REQUEST_TEMPLATE.md` is only auto-filled for PRs opened through
   the web UI — both paths above bypass it, so reproduce its sections yourself:
   `## Summary`, `## Changes`, `## Testing` (tick the three gate checkboxes you
   actually ran), then `## Release Notes`. Put screenshot embeds under
   `## Testing`.

## Phase 4 — Review loop (drive to green)

Repeat until `review-status.sh` exits `0`, capped at **3 fix iterations** (see
Phase 5 for what to do if still blocked).

1. **Wait for the gate to settle**, then read it.

   With `gh`:
   ```bash
   gh pr checks --watch --interval 30      # blocks until checks finish (or fail)
   .claude/skills/pull-request/scripts/review-status.sh
   ```
   - Exit `0` → green. Go to "Done".
   - Exit `1` → blocked. The output lists failing checks and every unresolved
     AI review thread (`[bot] path:line: comment`). Continue.
   - Exit `2` → query error (no PR / auth / `gh` missing). Resolve and retry.

   Without `gh`, do the same two reads by hand and apply the same verdict
   (green = both clean):
   - `pull_request_read` (`method: "get_check_runs"`) — any `conclusion` that is
     not `success`/`neutral`/`skipped` is a failure; a `status` still
     `queued`/`in_progress` means poll again rather than declaring green.
     For a failed Actions check, `get_job_logs` (`failed_only: true`,
     `return_content: true`) returns the actual error.
   - `pull_request_read` (`method: "get_review_comments"`) — a thread counts
     against the gate when it is unresolved **and** its first comment's author is
     `coderabbitai[bot]` or `cubic-dev-ai[bot]` (the GraphQL form of these logins
     drops the `[bot]` suffix, so match both spellings).
2. **Address every unresolved thread on its merits.** For each:
   - If the comment is correct, fix the code. Re-run the relevant part of the
     **local gate** (Phase 1) so you don't re-break clippy/tests.
   - If it's a false positive or out of scope, reply on the thread explaining why,
     rather than silently ignoring it. Use:
     ```bash
     gh pr comment <PR#> --body "..."        # general reply
     # or reply inline to a specific review comment via the API if needed.
     ```
     Without `gh`: `add_reply_to_pull_request_comment` (pass the thread's first
     comment id) for an inline reply, `add_issue_comment` for a general one.
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

- **Green:** Tell the user the PR is passing — link (`gh pr view --web` URL, or
  the `html_url` from `pull_request_read`), a one-line summary of what changed,
  and what was verified (with the screenshot links for UI work). Do **not** merge
  unless the user explicitly asks.
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

Without `gh`, the same issue via `issue_write` (`method: "create"`), passing
`labels: ["type:docs", "area:docs", "area:web"]` as an array rather than the
comma-joined string the CLI takes. `issue_write` is deliberately **not** in
`.claude/settings.json`, so it prompts — filing a blog issue already requires
the user's explicit go-ahead, and the prompt is a second check on that.

Match the existing issues' Japanese section headings. Title may be Japanese.

---

## Reference

Works in both environments:

| Need | Command |
| --- | --- |
| Which environment am I in | `command -v gh` (empty ⇒ MCP path) |
| Current branch | `git rev-parse --abbrev-ref HEAD` |
| Local gate | `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` |
| GUI build | `RUSTFLAGS="-L $HOME/.local/devlibs" cargo build --features gui --bin nohrs` |
| Launch / shot / stop GUI | `./script/ui-run.sh {launch,shot <png>,stop}` |
| Publish screenshots | `.claude/skills/pull-request/scripts/attach-screenshots.sh <slug> <png...>` |

`gh`-only (see the Environment table above for the MCP equivalents):

| Need | Command |
| --- | --- |
| Watch checks | `gh pr checks --watch --interval 30` |
| Gate report | `.claude/skills/pull-request/scripts/review-status.sh [PR#]` |
| PR review threads (raw) | `gh api repos/{owner}/{repo}/pulls/{n}/comments` |
| Blog issue convention | `gh issue list --label type:docs` (template: #154, #157) |

**AI reviewer bots** treated as the gate: `coderabbitai[bot]`, `cubic-dev-ai[bot]`.
Override with the `PR_REVIEW_BOTS` env var (space-separated) if the set changes.
