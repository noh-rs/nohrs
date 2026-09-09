#!/usr/bin/env bash
# Aggregate the PR review gate into one machine-readable report:
#   - CI / status checks (gh pr checks)
#   - Unresolved review threads from the AI reviewers (CodeRabbit, cubic)
#
# Usage:
#   review-status.sh [PR_NUMBER]
# If PR_NUMBER is omitted it is resolved from the current branch.
#
# Exit codes:
#   0  green   -> all checks passed AND no unresolved AI review threads
#   1  blocked -> at least one check failing/pending OR unresolved threads remain
#   2  error   -> could not query GitHub (no PR for branch, auth, etc.)
#
# The bots we treat as the AI review gate. Extend if more reviewers are added.
set -uo pipefail

REVIEW_BOTS_DEFAULT="coderabbitai[bot] cubic-dev-ai[bot]"
REVIEW_BOTS="${PR_REVIEW_BOTS:-$REVIEW_BOTS_DEFAULT}"

pr="${1:-}"

meta() {
  # Prints: <number>\t<owner>\t<repo>
  local fields
  if [ -n "$pr" ]; then
    fields=$(gh pr view "$pr" --json number,headRepositoryOwner,headRepository 2>/dev/null) || return 1
  else
    fields=$(gh pr view --json number,headRepositoryOwner,headRepository 2>/dev/null) || return 1
  fi
  printf '%s' "$fields" | python3 -c '
import sys, json
d = json.load(sys.stdin)
owner = (d.get("headRepositoryOwner") or {}).get("login", "")
repo  = (d.get("headRepository") or {}).get("name", "")
num = d.get("number", "")
print("\t".join((str(num), owner, repo)))
' 2>/dev/null
}

info=$(meta) || { echo "error: no PR found for this branch (open one first)" >&2; exit 2; }
PR_NUM=$(printf '%s' "$info" | cut -f1)
OWNER=$(printf '%s' "$info" | cut -f2)
REPO=$(printf '%s' "$info" | cut -f3)
[ -n "$PR_NUM" ] && [ -n "$OWNER" ] && [ -n "$REPO" ] \
  || { echo "error: could not resolve owner/repo/number" >&2; exit 2; }

echo "== PR #$PR_NUM ($OWNER/$REPO) =="

# --- 1. CI / status checks -------------------------------------------------
echo
echo "-- checks --"
checks_out=$(gh pr checks "$PR_NUM" 2>&1)
checks_rc=$?
echo "$checks_out"
# gh pr checks: rc 0 = all passing, 8 = still pending, other nonzero = failing.
checks_green=0
if [ "$checks_rc" -eq 0 ]; then checks_green=1; fi

# --- 2. Unresolved AI review threads --------------------------------------
echo
echo "-- unresolved review threads --"
threads_json=$(gh api graphql -F owner="$OWNER" -F repo="$REPO" -F pr="$PR_NUM" -f query='
query($owner:String!, $repo:String!, $pr:Int!) {
  repository(owner:$owner, name:$repo) {
    pullRequest(number:$pr) {
      reviewThreads(first: 100) {
        nodes {
          isResolved
          isOutdated
          path
          line
          comments(first: 1) {
            nodes { author { login } body }
          }
        }
      }
    }
  }
}' 2>/dev/null) || { echo "error: graphql query failed" >&2; exit 2; }

# Python emits the human-readable listing followed by a final
# "__UNRESOLVED_COUNT__=N" line. We must FAIL FAST if it errors: defaulting the
# count to 0 on parse failure would report a false "green" and unblock the gate.
parsed=$(printf '%s' "$threads_json" | REVIEW_BOTS="$REVIEW_BOTS" python3 -c '
import sys, json, os

def norm(name):
    # GraphQL author.login omits the trailing "[bot]" that the REST API adds;
    # normalize so the configured names match regardless of which form is used.
    return name[:-5] if name.endswith("[bot]") else name

bots = {norm(b) for b in os.environ.get("REVIEW_BOTS", "").split()}
data = json.load(sys.stdin)
threads = (((data.get("data") or {}).get("repository") or {})
           .get("pullRequest") or {}).get("reviewThreads", {}).get("nodes", [])
count = 0
for t in threads:
    if t.get("isResolved"):
        continue
    comments = (t.get("comments") or {}).get("nodes") or []
    author = (comments[0].get("author") or {}).get("login", "") if comments else ""
    if bots and norm(author) not in bots:
        continue
    count += 1
    body = (comments[0].get("body", "") if comments else "").strip().replace("\n", " ")
    outdated = " (outdated)" if t.get("isOutdated") else ""
    loc = f'"'"'{t.get("path","?")}:{t.get("line","?")}'"'"'
    print(f"  [{author}] {loc}{outdated}: {body[:160]}")
print(f"__UNRESOLVED_COUNT__={count}")
') || { echo "error: failed to parse review threads" >&2; exit 2; }

printf '%s\n' "$parsed" | grep -v '^__UNRESOLVED_COUNT__='
unresolved_count=$(printf '%s\n' "$parsed" | sed -n 's/^__UNRESOLVED_COUNT__=//p')
[[ "$unresolved_count" =~ ^[0-9]+$ ]] \
  || { echo "error: parser did not report a thread count" >&2; exit 2; }
[ "$unresolved_count" -eq 0 ] && echo "  (none)"

# --- verdict ---------------------------------------------------------------
echo
if [ "$checks_green" -eq 1 ] && [ "$unresolved_count" -eq 0 ]; then
  echo "VERDICT: green (checks pass, no unresolved AI review threads)"
  exit 0
fi
echo "VERDICT: blocked (checks_green=$checks_green, unresolved_threads=$unresolved_count)"
exit 1
