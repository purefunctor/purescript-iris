#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: release-notes.sh <previous-tag> <tag>

Print release notes for every change on the first-parent history of main between
previous-tag and tag. Pull request merges link their pull request; changes pushed
directly to main link their commit.
USAGE
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

if [[ $# -ne 2 ]]; then
  usage >&2
  exit 2
fi

previous_tag=$1
tag=$2
repository=purefunctor/purescript-iris
repository_url="https://github.com/$repository"

if [[ $(git rev-parse --is-shallow-repository) == true ]]; then
  printf 'release-notes: shallow repository; run git fetch --unshallow origin\n' >&2
  exit 2
fi

git rev-parse --verify --quiet "$previous_tag^{commit}" >/dev/null || {
  printf 'release-notes: tag not found: %s\n' "$previous_tag" >&2
  exit 2
}

git rev-parse --verify --quiet "$tag^{commit}" >/dev/null || {
  printf 'release-notes: tag not found: %s\n' "$tag" >&2
  exit 2
}

commit_author() {
  local login
  login=$(gh api "repos/$repository/commits/$1" --jq '.author.login // empty')
  if [[ -n "$login" ]]; then
    printf '@%s' "$login"
  else
    git log -1 --format=%an "$1"
  fi
}

printf "## What's Changed\n"

while IFS=$'\t' read -r commit subject; do
  if [[ "$subject" =~ ^(.*)\ \(#([0-9]+)\)$ ]]; then
    title=${BASH_REMATCH[1]}
    number=${BASH_REMATCH[2]}
    login=$(gh pr view "$number" --repo "$repository" --json author --jq .author.login)
    printf '* %s by @%s in %s/pull/%s\n' "$title" "$login" "$repository_url" "$number"
  else
    author=$(commit_author "$commit")
    printf '* %s by %s in %s/commit/%s\n' "$subject" "$author" "$repository_url" "$commit"
  fi
done < <(git log --first-parent --reverse --format='%H%x09%s' "$previous_tag..$tag")

printf '\n\n**Full Changelog**: %s/compare/%s...%s\n' "$repository_url" "$previous_tag" "$tag"
