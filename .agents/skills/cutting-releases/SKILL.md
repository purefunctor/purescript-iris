---
name: cutting-releases
description: "Cuts Iris GitHub releases through the version-bump PR, merge commit, tag-driven build workflow, attestations, installer tests, and generated release notes. Use when preparing, publishing, verifying, or backfilling notes for a purescript-iris release."
---

# Cutting Iris Releases

Use this workflow for stable `purescript-iris` releases. A pushed stable `v*` tag triggers `.github/workflows/release.yml`, which creates the GitHub release, builds and attests four archives, and tests the installers on Linux, macOS, and Windows. Automated `v<version>-dev.<revision>` canaries are owned by `.github/workflows/canary.yml`; do not use this manual workflow to prepare or repair them.

Merging and pushing the release tag are shared, high-impact actions. Obtain explicit approval before each unless the user has already authorized that stage. Never move or delete a published release tag to repair a failed workflow.

## Contract with installed binaries

`iris self update` in already installed binaries, and the installers, find releases by these names. Old binaries cannot be fixed after they ship, so changing any of these breaks updates for every installed release. Keep them unless the change is deliberate and announced:

- Stable tags match `v<major>.<minor>.<patch>`, and canary tags match `v<version>-dev.<12-character revision>`. The executable's `--version` prints `iris <tag without v>`.
- The latest stable release is the release GitHub marks latest, read from the `/releases/latest` redirect. Canaries are published prereleases created with `--latest=false`.
- Each release has an archive per target named `iris-<target>.tar.gz`, or `iris-<target>.zip` on Windows, for `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`, `universal-apple-darwin`, and `x86_64-pc-windows-msvc`.
- Each archive has a leading `iris-<target>/` directory containing exactly one `iris` or `iris.exe` executable, and has a GitHub build-provenance attestation.

## 1. Establish the release range

Start from a clean, current `main` and fetch tags:

```bash
git status --short --branch
git fetch origin main --tags
gh auth status
```

Set the requested version without the `v` prefix. Determine the previous stable release from GitHub rather than assuming it or selecting an automated canary tag:

```bash
version=0.0.16
tag="v$version"
previous_tag=$(gh release list --exclude-drafts --exclude-pre-releases \
  --limit 1 --json tagName --jq '.[0].tagName')
printf 'Release range: %s...%s\n' "$previous_tag" "$tag"
```

Before editing, verify that the new tag and release do not exist:

```bash
git ls-remote --exit-code --tags origin "refs/tags/$tag"
gh release view "$tag"
```

Both commands should report that the target does not exist. Stop if either exists and inspect the published state instead of overwriting it.

## 2. Prepare the version-bump commit

Create a release branch from current `main`. Update both version sources:

- `compiler-executable/iris-cli/Cargo.toml`: package `version`
- `Cargo.lock`: the `iris-cli` package `version`

Do not change the internal compiler crates, which remain independently versioned.

Verify the focused package and the user-visible version:

```bash
cargo check -p iris-cli --tests --locked
cargo run -p iris-cli --locked \
  --bin iris -- --version
git diff --check
git diff -- compiler-executable/iris-cli/Cargo.toml Cargo.lock
```

The CLI must print `iris $version`. Commit only the manifest and lockfile:

```bash
git add compiler-executable/iris-cli/Cargo.toml Cargo.lock
git commit -m "Prepare $version release"
```

## 3. Open and validate the release PR

After push approval, push the branch and open a PR titled:

```text
[meta] Prepare <version> release
```

Include the focused check and CLI version verification in the PR body. Wait for all required checks:

```bash
gh pr checks <pr-number> --watch --interval 10
```

Do not merge while a required check is pending or failing. Diagnose failures before retrying.

## 4. Merge, then tag the merge commit

After merge approval, use a merge commit:

```bash
gh pr merge <pr-number> --merge
gh pr view <pr-number> --json state,mergedAt,mergeCommit,url
git fetch origin main --tags
```

Do not tag the release-branch commit. Read the merge commit OID from the merged PR and verify all of the following:

- It has two parents.
- It is the current `origin/main`.
- `compiler-executable/iris-cli/Cargo.toml` and `Cargo.lock` contain the requested version at that commit.
- The remote release tag is still absent.

After tag-push approval, create the lightweight tag on that exact merge commit and push only the tag:

```bash
git tag "$tag" <merge-commit-oid>
git push origin "refs/tags/$tag"
```

## 5. Monitor the complete release workflow

Find the tag-triggered `Cargo Build & Release` run and watch it to completion:

```bash
gh run list --workflow release.yml --branch "$tag" --limit 1
gh run watch <run-id> --exit-status --interval 10
```

Success requires:

- The GitHub release was created.
- GNU Linux, musl Linux, universal macOS, and Windows archives were uploaded.
- Every archive received build-provenance attestation.
- Installer tests passed on Linux, macOS, and Windows.

Do not treat release creation alone as completion. The archive and installer jobs must also pass.

## 6. Generate and persist patch notes

The release action creates an empty release body. Do not use GitHub's generated release notes: they list only pull requests, and maintainers often land merge commits on `main` directly. Generate the notes from the first-parent history of `main` instead, so every merge commit in the release appears once. Pull request merges link their pull request; direct merges and commits link their commit.

After the release workflow completes, generate the notes from a full clone and update the existing release:

```bash
git fetch origin main --tags
notes_file=$(mktemp)
.agents/skills/cutting-releases/scripts/release-notes.sh \
  "$previous_tag" "$tag" > "$notes_file"

cat "$notes_file"
gh release edit "$tag" --notes-file "$notes_file"
rm -f "$notes_file"
```

The script refuses a shallow clone; run `git fetch --unshallow origin` first. Check that each entry is a change that landed on `main` in this release, and that the release PR is the last entry.

Read the release back rather than trusting the edit command:

```bash
gh release view "$tag" --json body,url \
  --jq 'if .body == "" then error("release body is empty") else {url, body: .body} end'
```

The notes must end with the full changelog for `$previous_tag...$tag`.

### Backfilling an existing release

Run the same script with the existing tag and its immediately preceding release tag. Confirm the body is empty, or contains only notes generated by GitHub or this script, before replacing it. If the release has curated notes, preserve them unless the user explicitly requests replacement.

## 7. Final verification

Confirm the release is published, notes remain present, the tag points to the intended merge commit, and all four assets exist:

```bash
gh release view "$tag" \
  --json url,isDraft,isPrerelease,publishedAt,body,assets
git ls-remote --tags origin "refs/tags/$tag"
gh run view <run-id> --json status,conclusion,url,headSha,headBranch
```

Report links to the merged PR, published release, and successful workflow. Call out any missing asset, empty notes, failed installer, or tag/commit mismatch.
