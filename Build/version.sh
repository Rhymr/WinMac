#!/usr/bin/env bash
# Single source of truth for Rhymr's SemVer string, derived from git history
# (never hand-maintained). See CLAUDE.md § Versioning.
#
#   MAJOR  0 until a `git tag -a vX.Y.Z` with X >= 1 exists and is reachable;
#          that tag then becomes the base and MINOR/PATCH count since it.
#   MINOR  count of every `feat:` commit reachable from HEAD (all ancestors).
#   PATCH  count of `fix:` commits since the most recent `feat:` commit.
#   +build.<N>.g<sha>[.dirty]  N = `git rev-list --count HEAD`,
#          sha = short hash, `.dirty` when the tracked tree has changes.
#   No `-prerelease` while MAJOR is 0. RHYMR_VERSION_PRERELEASE injects one.
#
# Usage:
#   Build/version.sh              -> core string, e.g. 0.137.4
#   Build/version.sh --full       -> full string, e.g. 0.137.4+build.201.gdeadbee
#   Build/version.sh --json       -> {"core":"…","full":"…","major":0,…}
set -euo pipefail

cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"

# --- base tag ---------------------------------------------------------------
base_tag=""
if base_tag=$(git describe --tags --match 'v[1-9]*.*.*' --abbrev=0 2>/dev/null); then :; else base_tag=""; fi

if [[ -n "$base_tag" ]]; then
  base_ver=${base_tag#v}
  major=${base_ver%%.*}
  range="${base_tag}..HEAD"
else
  major=0
  range="HEAD"
fi

# --- minor: all reachable feat: commits (since base tag if any) -----------
minor=$(git log "$range" --format='%s' 2>/dev/null | grep -cE '^feat(\(.+\))?!?: ' || true)
minor=${minor:-0}

# --- patch: fix: commits since the most recent feat: commit ---------------
last_feat=$(git log "$range" --format='%H %s' 2>/dev/null \
  | grep -E ' feat(\(.+\))?!?: ' | head -n1 | cut -d' ' -f1 || true)
if [[ -n "$last_feat" ]]; then
  patch=$(git log "${last_feat}..HEAD" --format='%s' | grep -cE '^fix(\(.+\))?!?: ' || true)
else
  patch=$(git log "$range" --format='%s' 2>/dev/null | grep -cE '^fix(\(.+\))?!?: ' || true)
fi
patch=${patch:-0}

# --- build metadata ------------------------------------------------------
count=$(git rev-list --count HEAD)
sha=$(git rev-parse --short=7 HEAD)
dirty=""
if ! git diff --quiet --ignore-submodules HEAD 2>/dev/null; then dirty=".dirty"; fi

core="${major}.${minor}.${patch}"
if [[ "$major" == "0" ]]; then
  pre=""
else
  pre="${RHYMR_VERSION_PRERELEASE:+-${RHYMR_VERSION_PRERELEASE}}"
fi
core="${core}${pre}"
full="${core}+build.${count}.g${sha}${dirty}"

case "${1:-}" in
  --full) echo "$full" ;;
  --json) printf '{"core":"%s","full":"%s","major":%s,"minor":%s,"patch":%s,"count":%s,"sha":"%s","dirty":%s}\n' \
            "$core" "$full" "$major" "$minor" "$patch" "$count" "$sha" "$([[ -n $dirty ]] && echo true || echo false)" ;;
  *)      echo "$core" ;;
esac
