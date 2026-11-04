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

## Automatic patch releases

Every push to `main` that passes the complete Check workflow triggers Auto tag.
It tags only the checked SHA when it is still the remote main HEAD. Older
checked commits and commits already carrying a supported release tag, including
a manual beta, are skipped. Changes only to `.github/APPROVED_CONTRIBUTORS`
remain excluded from Check and do not trigger a release.

Auto tag creates an annotated stable tag with a patch number above the highest
existing release tag, then pushes it using the `aidget-bot` installation token.
For example, `v0.1.3` produces `v0.1.4`; either `v0.1.4-beta.2` or an unpublished
`v0.1.4` produces `v0.1.5`. Failed and cancelled tags reserve their numbers.
The first release tag must be created manually. Minor, major and beta releases
also use the manual procedure below.

Automatic tag allocation is serialized. A conflicting push fails without
overwriting a tag or choosing another number automatically. Re-running Check
on an already tagged SHA does not allocate another version.

## Release cancellation

After validating a new tag, Release cancels active Release runs for lower
versions, including manual beta releases. It never cancels equal or higher
versions, so a delayed old run or an old rerun cannot cancel a newer release.
Once a higher supported tag exists, lower tags skip the pipeline. The workflow
refreshes tags again before creating a draft and before publishing it.

Cancellation starts only after the new main commit has passed Check and its
automatic tag has been pushed. A failed Check does not interrupt the preceding
release. Manual tags trigger the same validation and cancellation process.
Version numbers may have gaps. All tags and published releases remain intact.

A cancelled build normally leaves only a tag, without a Release entry. If it
reaches staging, it may leave an unpublished draft with partial uploads; drafts
are visible only to repository collaborators with write access. A successful
publication cannot be undone by cancellation; that complete release remains
public, and a later stable release becomes Latest.

## Required repository configuration

Configure `DEEPSEEK_API_KEY` as a repository Actions secret. It is used only by
the release notes job: first to generate an English changelog from the commit
subjects since the previous release, then to translate that changelog into
Simplified Chinese.

There are no signing or updater secrets in the current process.

Auto tag reuses the `AIDGET_BOT_CLIENT_ID` and `AIDGET_BOT_APP_ID` variables
and `AIDGET_BOT_PRIVATE_KEY` secret from contributor approval. The App needs
Contents write access. A tag pushed with the default `GITHUB_TOKEN` would not
trigger Release. The Release metadata job uses its own `GITHUB_TOKEN` with
Actions write access to cancel older runs; build and notes jobs do not receive
that permission.

## Create a manual release

Start from current `main` and confirm the reviewed commit is part of it:

```bash
git switch main
git pull --ff-only
git fetch --tags origin

REVIEWED_SHA=<reviewed-main-commit>
git merge-base --is-ancestor "$REVIEWED_SHA" origin/main
```

Choose a version above every existing supported release tag, including failed
and cancelled tags. Confirm it is also newer than the latest published tag:

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

## Pipeline caching

The Check workflow warms release dependency caches on every push to `main`.
It covers macOS Apple Silicon, macOS Intel, Windows x64 and Linux x64 with the
same `release-deps` cache key used by the Release build. A cache hit skips the
warm-up compile; a miss runs a release build without packaging and saves its
dependencies. Each matrix job first performs a `lookup-only` probe. On an exact
hit it also skips cache restoration, pnpm setup/install, Linux package installation
and compilation; only a miss performs those steps.

Release builds and verification restore the `main` cache but do not save
tag-scoped caches. A cold first run still needs a full compile. Use per-job
timestamps and cache hits to identify regressions before removing platform
checks or assets.

Verification, packaging and bilingual notes start together after tag metadata
is validated. The publish job waits for all three and only creates the draft
after the four build artifacts and `release-notes` artifact exist.

## Release assets

Every successful release contains:

- `aidget-darwin-arm64.dmg`
- `aidget-darwin-amd64.dmg`
- `aidget-darwin-arm64.zip`
- `aidget-darwin-amd64.zip`
- `aidget-windows-amd64.exe`
- `aidget-linux-amd64.deb`
- `aidget-linux-amd64.rpm`
- `SHA256SUMS`

DeepSeek's `deepseek-flash` model generates the English notes from commit
subjects since the latest published stable release. When there is no stable
release, it uses the full history, including the root commit. Failed tags do not
set the changelog boundary.
A second DeepSeek call adds a Simplified Chinese translation below
`<!-- lang:zh -->`. Fixed unsigned installation instructions and contributor
thanks are appended to each language after generation and translation. Contributors
come from the same commit range: authors, coauthors, explicit reporter credits and
referenced issue/PR authors. Maintainer `micooz`, bots and coding agents are
excluded; only public names are shown, without hyperlinks. GitHub lookup
failures abort publication rather than silently dropping names. The notes step
uses `GH_TOKEN` with read access to issues and pull requests.

Generated notes start directly with level-2 category headings such as
`## Features` and `## Bug Fixes`; there is no document title, `Changelog`
wrapper, or other heading level.

Assets use `aidget-<platform>-<arch>.<ext>`: `darwin/windows/linux`, `arm64/amd64`.
The macOS ZIP retains its `.app` directory. Names no longer include versions;
`SHA256SUMS` covers the exact public filenames. Beta.1 and older clients require
one manual installation of beta.2 or later to recognize these new names. Published
releases and their original assets are left unchanged.

AppImage is no longer built or published. Users who installed beta.1 or beta.2
from an AppImage must manually migrate to the DEB or RPM package; those old
builds cannot self-update to a release without a matching AppImage asset.

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
- A superseded tag cannot be retried once a higher supported tag exists. Use a
  new reviewed main commit and a higher version instead.
- Never hand-upload or replace assets after a release is public.

## Signing and updates

The current macOS and Windows artifacts are unsigned. macOS users may see
Gatekeeper warnings, and Windows users may see SmartScreen warnings.

The app checks the latest stable GitHub Release and verifies update downloads
against `SHA256SUMS`. macOS uses the `.zip` assets for application replacement;
Linux uses DEB or RPM through the package manager already on the machine. Beta
releases do not appear in the stable update check.

RPM beta packages use the core version as RPM `Version` and
`0.beta.N.1` as RPM `Release`, which sorts before the stable release's `1`.
The packaged executable retains the full beta version. Public asset filenames
do not include the version.
