#!/usr/bin/env bash
# publish-github-release.sh <dir>...: mirrors a dev release's signed files (every file in each
# <dir>) to the GitHub release of TAG, byte for byte, creating it with generated notes and marking
# it latest. release.yml runs it last, after the release service published and tag-release.sh
# tagged TAG at SHA; it never creates the tag. A rerun uploads only what's missing.
#
# Environment: VERSION, TAG, SHA, GITHUB_REPOSITORY, GH_TOKEN.
. "$(dirname "$0")/lib.sh"

[[ "$VERSION" =~ ^0\.0\.0-dev-[0-9]+$ ]] || die "Only a dev 0.0.0-dev-<n> release goes to GitHub, not $VERSION."
[ "$TAG" = "v$VERSION" ] || die "The release tag is $TAG, not v$VERSION."
[ $# -gt 0 ] || die "Usage: publish-github-release.sh <dir>..."
# The newest earlier dev tag, so the generated notes cover what changed since the last build.
previous_tag() {
  gh api "repos/$GITHUB_REPOSITORY/git/matching-refs/tags/v0.0.0-dev-" --paginate --jq '.[].ref' |
    sed -n 's|^refs/tags/\(v0\.0\.0-dev-[0-9]*\)$|\1|p' | { grep -Fxv "$TAG" || true; } | sort -t- -k3 -n | tail -n 1
}

tagged=$(gh api "repos/$GITHUB_REPOSITORY/git/ref/tags/$TAG" --jq .object.sha)
[ "$tagged" = "$SHA" ] || die "Tag $TAG names $tagged, not $SHA."

if ! gh release view "$TAG" -R "$GITHUB_REPOSITORY" --json name > /dev/null 2>&1; then
  previous=$(previous_tag)
  # With no earlier dev tag, generated notes would list every PR in the repo, past GitHub's 125,000
  # character limit, so the first one names its commit instead.
  if [ -n "$previous" ]; then
    notes=(--generate-notes --notes-start-tag "$previous")
  else
    notes=(--notes "Dev build $VERSION from https://github.com/$GITHUB_REPOSITORY/commit/$SHA.")
  fi
  gh release create "$TAG" -R "$GITHUB_REPOSITORY" --verify-tag --title "Brocode $VERSION" \
    --latest=true "${notes[@]}"
fi
mirror=$(mktemp -d)
trap 'rm -rf "$mirror"' EXIT
gh release view "$TAG" -R "$GITHUB_REPOSITORY" --json assets > "$mirror/release.json"
for dir in "$@"; do
  for file in "$dir"/*; do
    name=$(basename "$file")
    [ "$name" != SHA256SUMS ] || continue
    if ! jq -e --arg name "$name" '.assets | any(.name == $name)' "$mirror/release.json" > /dev/null; then
      gh release upload "$TAG" "$file" -R "$GITHUB_REPOSITORY"
    fi
    gh release download "$TAG" -R "$GITHUB_REPOSITORY" --pattern "$name" --dir "$mirror" --clobber
    cmp -s "$file" "$mirror/$name" || die "GitHub's $name differs from the release bytes."
    rm -f "$mirror/$name"
  done
done
echo "https://github.com/$GITHUB_REPOSITORY/releases/tag/$TAG" >> "$GITHUB_STEP_SUMMARY"
