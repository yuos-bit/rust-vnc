#!/usr/bin/env bash
# Decide whether this push should produce a release and compute its version.
#
# Rules:
#   * no vMAJOR.MINOR.PATCH tag yet  -> release 0.1.0
#   * at least RELEASE_EVERY new commits since the latest tag -> release
#     (patch + 1)
#   * explicit manual force -> release regardless of the interval
#
# Outputs (also written to $GITHUB_OUTPUT when present):
#   run, version, tag, version_code, reason
set -euo pipefail

every="${RELEASE_EVERY:-10}"
force="${FORCE_RELEASE:-false}"

latest="$(git tag --list 'v[0-9]*' --sort=-v:refname | head -n1 || true)"
major=0
minor=1
patch=0
run=false
reason=""

if [ -z "$latest" ]; then
  run=true
  reason="no previous release tag; first release starts at v0.1.0"
else
  if [[ ! "$latest" =~ ^v([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
    echo "latest tag '$latest' is not vMAJOR.MINOR.PATCH" >&2
    exit 1
  fi
  major="${BASH_REMATCH[1]}"
  minor="${BASH_REMATCH[2]}"
  patch="${BASH_REMATCH[3]}"
  count="$(git rev-list --count "${latest}..HEAD")"

  if [ "$force" = "true" ]; then
    run=true
    reason="manual force (${count} commits since ${latest})"
  elif [ "$count" -ge "$every" ]; then
    run=true
    reason="${count} commits since ${latest} (threshold ${every})"
  else
    reason="${count}/${every} commits since ${latest}; waiting for more commits"
  fi
  # Any release increments the patch component: v0.1.0 -> v0.1.1.
  patch=$((10#$patch + 1))
fi

version="${major}.${minor}.${patch}"
version_code=$((10#$major * 1000000 + 10#$minor * 1000 + 10#$patch))
tag="v${version}"

emit() {
  printf '%s=%s\n' "$1" "$2"
  if [ -n "${GITHUB_OUTPUT:-}" ]; then
    printf '%s=%s\n' "$1" "$2" >> "$GITHUB_OUTPUT"
  fi
}

emit run "$run"
emit version "$version"
emit tag "$tag"
emit version_code "$version_code"
emit reason "$reason"

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### Release decision"
    echo
    echo "- run: \`${run}\`"
    echo "- next version: \`${tag}\` (versionCode \`${version_code}\`)"
    echo "- reason: ${reason}"
  } >> "$GITHUB_STEP_SUMMARY"
fi
