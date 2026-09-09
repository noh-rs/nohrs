#!/usr/bin/env bash
# Publish UI verification screenshots (or any image/mp4) to a dedicated assets
# branch and print Markdown embeds you can paste into the PR body. This keeps
# the PR's code diff clean (the AI reviewers never see the binaries) while the
# images still render inline because the repo is public.
#
# Usage:
#   attach-screenshots.sh <slug> <file1.png> [file2.png ...]
#     <slug>  short label for this batch, e.g. "pr-105" or "grid-view".
#
# Env:
#   PR_ASSETS_BRANCH   assets branch name (default: pr-assets)
#
# Prints, on success, a Markdown block of `![slug-name](raw-url)` lines on stdout.
# All diagnostics go to stderr. Exit nonzero on any failure.
set -uo pipefail

slug="${1:-}"
shift || true
[ -n "$slug" ] && [ "$#" -ge 1 ] || {
  echo "usage: attach-screenshots.sh <slug> <file1> [file2 ...]" >&2; exit 2; }
# slug is interpolated into the published path; restrict it to a safe charset so
# values like "../.." or "a/b" can't escape the screenshots/ subdirectory.
[[ "$slug" =~ ^[A-Za-z0-9._-]+$ ]] || {
  echo "error: slug must match ^[A-Za-z0-9._-]+$ (no path separators)" >&2; exit 2; }

ASSET_BRANCH="${PR_ASSETS_BRANCH:-pr-assets}"

# Validate inputs up front so we never create a worktree or push a bad batch.
# Files are stored by basename, so two inputs sharing one would overwrite each
# other and yield wrong embeds — reject the batch rather than silently lose one.
declare -A seen_basenames=()
for f in "$@"; do
  [ -f "$f" ] || { echo "error: not a file: $f" >&2; exit 2; }
  base=$(basename "$f")
  [ -z "${seen_basenames[$base]:-}" ] \
    || { echo "error: duplicate filename in batch: $base" >&2; exit 2; }
  seen_basenames[$base]=1
done

origin_url=$(git remote get-url origin 2>/dev/null) \
  || { echo "error: no 'origin' remote" >&2; exit 2; }
# Derive owner/repo from either https or ssh remote forms.
slug_path=$(printf '%s' "$origin_url" | sed -E 's#^.*github\.com[:/]##; s#\.git$##')
owner=$(printf '%s' "$slug_path" | cut -d/ -f1)
repo=$(printf '%s' "$slug_path" | cut -d/ -f2)
[ -n "$owner" ] && [ -n "$repo" ] \
  || { echo "error: could not parse owner/repo from $origin_url" >&2; exit 2; }

# A per-run subdir keeps repeated uploads from colliding. $RANDOM is fine here:
# we only need uniqueness, not unpredictability.
dest_dir="screenshots/${slug}/$$-$RANDOM"

tmp=$(mktemp -d) || { echo "error: mktemp failed" >&2; exit 2; }
cleanup() { git worktree remove --force "$tmp" >/dev/null 2>&1 || rm -rf "$tmp"; }
trap cleanup EXIT

git fetch --quiet origin "$ASSET_BRANCH" 2>/dev/null
if git ls-remote --exit-code --heads origin "$ASSET_BRANCH" >/dev/null 2>&1; then
  git worktree add --quiet -B "$ASSET_BRANCH" "$tmp" "origin/$ASSET_BRANCH" \
    || { echo "error: failed to check out $ASSET_BRANCH" >&2; exit 1; }
else
  # git 2.43: --orphan takes the branch name via -b, not as a positional arg.
  git worktree add --quiet --orphan -b "$ASSET_BRANCH" "$tmp" \
    || { echo "error: failed to create orphan $ASSET_BRANCH" >&2; exit 1; }
fi

mkdir -p "$tmp/$dest_dir" || { echo "error: mkdir failed" >&2; exit 1; }

embeds=""
for f in "$@"; do
  base=$(basename "$f")
  cp "$f" "$tmp/$dest_dir/$base" || { echo "error: copy failed: $f" >&2; exit 1; }
  raw="https://raw.githubusercontent.com/${owner}/${repo}/${ASSET_BRANCH}/${dest_dir}/${base}"
  embeds+="![${slug}-${base}](${raw})"$'\n'
done

(
  cd "$tmp" || exit 1
  git add -A "$dest_dir" || exit 1
  git commit --quiet -m "Add screenshots for ${slug}" || exit 1
  git push --quiet origin "HEAD:${ASSET_BRANCH}" || exit 1
) || { echo "error: failed to commit/push screenshots" >&2; exit 1; }

echo "published to branch '$ASSET_BRANCH' under $dest_dir" >&2
printf '%s' "$embeds"
