#!/usr/bin/env bash
# opencode-rune-pr-plan.sh: computes side-by-side app naming, bundle ID, version, and release tag
# for .github/workflows/opencode-rune-pr-build.yml.
set -euo pipefail

: "${PR_NUMBER:?Set PR_NUMBER}"
: "${HEAD_REF:?Set HEAD_REF}"
: "${HEAD_SHA:?Set HEAD_SHA}"
: "${BASE_SHA:?Set BASE_SHA}"
: "${GITHUB_RUN_NUMBER:?Set GITHUB_RUN_NUMBER}"

[[ "$PR_NUMBER" =~ ^[1-9][0-9]*$ ]] || { echo "::error::Invalid PR_NUMBER: $PR_NUMBER" >&2; exit 1; }
[[ "$HEAD_SHA" =~ ^[0-9a-f]{40}$ ]] || { echo "::error::Invalid HEAD_SHA: $HEAD_SHA" >&2; exit 1; }
[[ "$BASE_SHA" =~ ^[0-9a-f]{40}$ ]] || { echo "::error::Invalid BASE_SHA: $BASE_SHA" >&2; exit 1; }

branch_slug=$(printf '%s' "$HEAD_REF" \
  | tr '/:' '--' \
  | tr -cs 'A-Za-z0-9._-' '-' \
  | sed -E 's/^-+//; s/-+$//; s/-{2,}/-/g' \
  | cut -c1-32 \
  | sed -E 's/-+$//')
[ -n "$branch_slug" ] || branch_slug="pr-$PR_NUMBER"

app_name="Brocode · $branch_slug"
bundle_id="com.apibrother.brocode.pr.$PR_NUMBER"
url_scheme="opencode-rune-pr-$PR_NUMBER"
version="0.0.0-pr-$PR_NUMBER-$GITHUB_RUN_NUMBER"
channel="pr-$PR_NUMBER"
file_stem="OpenCode-Rune-PR-$PR_NUMBER"
release_tag="opencode-rune-pr-$PR_NUMBER"

{
  echo "pr-number=$PR_NUMBER"
  echo "head-sha=$HEAD_SHA"
  echo "short-sha=${HEAD_SHA:0:9}"
  echo "base-sha=$BASE_SHA"
  echo "branch-slug=$branch_slug"
  echo "app-name=$app_name"
  echo "bundle-id=$bundle_id"
  echo "url-scheme=$url_scheme"
  echo "version=$version"
  echo "channel=$channel"
  echo "file-stem=$file_stem"
  echo "release-tag=$release_tag"
} >> "$GITHUB_OUTPUT"

echo "Planned PR #$PR_NUMBER test build: $app_name.app ($bundle_id, $version) @ ${HEAD_SHA:0:9}." >> "$GITHUB_STEP_SUMMARY"
