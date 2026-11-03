# Releasing aidget

## Release contract

Releases are built and published by `.github/workflows/release.yml` from an
annotated `vMAJOR.MINOR.PATCH` or `vMAJOR.MINOR.PATCH-beta.N` tag in
`aidget-ai/aidget`.

- The tag must point to a reviewed commit that is already on `main`.
- The version must be strictly newer than the current latest release.
- No source version bump commit is created.
- The workflow injects the tag version into `Cargo.toml`, `package.json` and
  `tauri.conf.json` before checks and packaging.
- The workflow stages a draft release only after every platform builds, then
  publishes it once all assets and notes exist. Stable releases become Latest;
  beta releases are marked Pre-release and never become Latest.

## Required repository secret

Configure `DEEPSEEK_API_KEY` as a repository Actions secret. It is used only by
the release notes job: first to generate an English changelog from the commit
subjects since the previous release, then to translate that changelog into
Simplified Chinese.

There are no signing or updater secrets in the current process.

## Create a release

Start from current `main` and confirm the reviewed commit is part of it:

```bash
git switch main
git pull --ff-only
git fetch --tags origin

REVIEWED_SHA=<reviewed-main-commit>
git merge-base --is-ancestor "$REVIEWED_SHA" origin/main
```

Choose the next version and confirm it is newer than the latest published tag:

```bash
TAG=v0.2.0
if LATEST=$(gh api repos/aidget-ai/aidget/releases/latest --jq .tag_name); then
  node scripts/release/version.mjs newer "$TAG" "$LATEST"
elif [ "$(jq -r '.status // empty' <<< "$LATEST")" != 404 ]; then
  echo "Failed to determine latest release: $LATEST" >&2
  exit 1
fi
```

Create and push an annotated tag on the reviewed commit:

```bash
git tag -a "$TAG" "$REVIEWED_SHA" -m "aidget $TAG"
git push origin "$TAG"
```

Watch the Release workflow:

```bash
gh run watch --repo aidget-ai/aidget
```

For packaging trials, start with `v0.1.1-beta.0` and increment the beta number
whenever a code or workflow fix is required. Keep each failed tag. A beta tests
the same checks, packaging, bilingual notes and asset uploads as a stable release.

## Release assets

Every successful release contains:

- `aidget-darwin-arm64.dmg`
- `aidget-darwin-amd64.dmg`
- `aidget-darwin-arm64.zip`
- `aidget-darwin-amd64.zip`
- `aidget-windows-amd64.exe`
- `aidget-linux-amd64.AppImage`
- `aidget-linux-amd64.deb`
- `aidget-linux-amd64.rpm`
- `SHA256SUMS`

DeepSeek generates the English notes from commit subjects since the latest
published stable release. When there is no stable release, it uses the full
history, including the root commit. Failed tags do not set the changelog boundary.
A second DeepSeek call adds a Simplified Chinese translation below
`<!-- lang:zh -->`. Fixed unsigned installation instructions and contributor
thanks are appended to each language after generation and translation. Contributors
come from the same commit range: authors, coauthors, explicit reporter credits and
referenced issue/PR authors. Maintainer `micooz`, bots and coding agents are
excluded; only public names and known GitHub profile links are shown. GitHub lookup
failures abort publication rather than silently dropping names. The notes step
uses `GH_TOKEN` with read access to issues and pull requests.

Assets use `aidget-<platform>-<arch>.<ext>`: `darwin/windows/linux`, `arm64/amd64`.
The macOS ZIP retains its `.app` directory. Names no longer include versions;
`SHA256SUMS` covers the exact public filenames. Beta.1 and older clients require
one manual installation of beta.2 or later to recognize these new names. Published
releases and their original assets are left unchanged.

## Failure and retry rules

- A missing or failed `DEEPSEEK_API_KEY` fails the release before a public
  release is created. Fix the secret and rerun the workflow.
- A failed build leaves no public release. A transient runner or service failure
  may be retried on the same unpublished tag.
- A failure that needs a code or workflow change requires a new commit on
  `main` and a new patch tag, or the next beta number during prerelease trials.
  Never move or delete the failed tag.
- A draft release left by a partial publish is replaced on rerun. A published
  release is immutable and cannot be replaced by this workflow.
- Never hand-upload or replace assets after a release is public.

## Signing and updates

The current macOS and Windows artifacts are unsigned. macOS users may see
Gatekeeper warnings, and Windows users may see SmartScreen warnings.

The app checks the latest stable GitHub Release and verifies update downloads
against `SHA256SUMS`. macOS uses the `.zip` assets for application replacement.
Beta releases do not appear in the stable update check.

RPM beta packages use the core version as RPM `Version` and
`0.beta.N.1` as RPM `Release`, which sorts before the stable release's `1`.
The packaged executable retains the full beta version. Public asset filenames
do not include the version.

The first live release is also the first end-to-end test of GitHub-hosted macOS,
Windows and Linux packaging. Until it runs, treat those packaging paths as
unverified.
