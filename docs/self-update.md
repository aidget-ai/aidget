# Self-update

Aidget's updater runs in the desktop GUI process. It checks the project's stable
GitHub Releases, downloads the asset for the current platform, verifies its
checksum, stages it, and replaces the installation through a platform-specific
path. The main window renders the updater's snapshot; it never contacts GitHub
itself.

This document describes the behavior implemented in `src/update/` and
`src/runtime/update.rs`.

## Runtime lifecycle

GUI setup creates one `UpdateService`, registers it as shared Tauri state, and
starts the launch check. The check waits five seconds so the windows and widgets
can initialize, then reads `settings.toml` for the GitHub token and the
automatic-download preference. If the settings cannot be read, the launch check
ends with a log message and no request.

There is no poller, scheduler, or recurring timer. After the launch check,
updates are checked only when the user invokes one of the update commands.
Rust debug builds skip the launch check entirely.

Every published `UpdateSnapshot` is emitted to the main window as
`aidget://update`. This is how the window learns about a launch check that was
already running before it loaded. While a download or check is active, the
window also polls `update_state` for progress.

## Which builds may replace themselves

`build.rs` sets `AIDGET_BUILD_STAMP`; the updater uses that stamp as the running
version. A build may replace itself only when the stamp parses as a release
version and was produced by an exact tag.

| Build                                         | Example stamp      | Self-updates |
| --------------------------------------------- | ------------------ | ------------ |
| `HEAD` is exactly on a tag                    | `v0.1.3`           | yes          |
| Commits after a tag                           | `0.1.0-5-gabc1234` | no           |
| Dirty working tree recorded by `git describe` | `0.1.0-dirty`      | no           |
| No tag or usable `git describe` result        | `0.1.0-untagged`   | no           |

An exact tag wins even when the tree is dirty because the release workflow
checks out the tag and then writes the version into the manifests before
building. A build whose stamp cannot be parsed is also treated as a source
build. A semver pre-release tag is still a release build, but the update feed
only offers stable releases.

Rust debug builds have an additional guard: `cfg!(debug_assertions)` is exposed
as `UpdateSnapshot.is_debug_build` and overrides the build stamp. They never
download or install, even if the executable happens to be built at a release
tag.

The About page's displayed version comes from the normal application version;
`UpdateSnapshot.current` contains the update-specific build stamp. The `source`
state, not the displayed version string, communicates that a build cannot
replace itself.

## Release feed

Production builds read releases from the fixed `aidget-ai/aidget` repository.
Tests can pass another base URL and repository through
`UpdateService::for_build`; there is no runtime setting for redirecting an
installed application to another repository.

The check reads GitHub's paginated releases endpoint:

```
GET /repos/aidget-ai/aidget/releases?per_page=100&page=N
```

Each response is limited to 8 MiB and has a 30-second request timeout. At most
20 pages are read. Draft releases, releases marked as pre-releases, and tags
with a semver pre-release component are excluded. Invalid SemVer tags in an
otherwise eligible release fail the check instead of being skipped.

Eligible releases are sorted newest-first by semver precedence. Build metadata
does not affect precedence. Pagination stops after a short page, or once the
current build's version has been reached; this gives release notes enough
older stable releases without walking the entire feed.

A `404` whose GitHub error message is exactly `Not Found` means the repository
has no published release and settles as `no_release`. Other non-success
responses are errors. When GitHub reports a rate limit, the error includes the
wait from `x-ratelimit-reset`.

The configured GitHub token is used only on the releases request. It is
attached only when the URL is HTTPS on `api.github.com`; a token-bearing
metadata URL for any other host is refused. Asset and `SHA256SUMS` downloads
never receive the token. Network URLs must use HTTPS, with loopback HTTP
allowed for the local test server. Redirects are followed only while they stay
on HTTPS, with at most five hops.

## States

`UpdateSnapshot.state` describes the current flow:

| State         | Meaning                                                                            |
| ------------- | ---------------------------------------------------------------------------------- |
| `idle`        | No check has run in this process.                                                  |
| `checking`    | A feed request is in flight.                                                       |
| `latest`      | The newest stable release is not newer than this build.                            |
| `source`      | This build cannot replace itself, including debug builds.                          |
| `no_release`  | No stable release has been published.                                              |
| `found`       | A newer stable release exists but has not been downloaded.                         |
| `downloading` | The selected asset is being downloaded.                                            |
| `ready`       | A verified download is available to install.                                       |
| `installing`  | Replacement is running, or finished successfully and the process is about to exit. |
| `error`       | The last step failed; `error` contains the reason.                                 |

The normal transitions are:

```
idle
  └─> checking ─> latest | source | no_release | found | error
                    found ─> downloading ─> ready ─> installing
                                             error
```

`UpdateSnapshot.staged` is separate from `state`: it reports whether the service
currently holds a verified download. It can therefore remain `true` while the
state is `ready` with a failed-check error, or `error` after a failed install.
The verified file is not resumed or rediscovered after the process exits.

A successful install clears `staged` and leaves the state at `installing` until
the process exits. A failed install leaves the verified download staged so the
user can retry it.

A check that fails does not discard an already verified download. With a staged
file the state becomes `ready` and the snapshot carries the error; without one
it becomes `error`. A check that still names the staged version also settles as
`ready` instead of downloading it again. If the feed moves to an older version
than the staged file, that staged file is discarded as a channel regression.

## Update actions

The main window has one stateful action in the About version row and another
copy in the global page header outside About. The header copy is hidden on
About and in debug builds. For `latest`, `no_release`, and `source`, the action
shows the result for three seconds and then returns to **Check for updates**;
the status line continues to describe the state.

The action calls:

- `update_check` when no update is actionable. This runs the normal feed check
  with the saved automatic-download preference.
- `update_apply` for `found`, `ready`, or an `error` that still has a release.
  This checks the feed again with automatic downloading forced on. If the
  recheck yields a verified `ready` state without an error, the verified file is
  installed. A newer release is downloaded and verified before installation.

A failed recheck never installs an older staged file. It leaves the verified
download in place and returns its error. The **Update** action works even when
automatic downloading is off.

The action is disabled while the state is `checking`, `downloading`, or
`installing`. The download label includes a percentage when the backend knows
the asset size.

## Automatic download preference

`settings.toml` contains:

| Key             | Default | Meaning                                                            |
| --------------- | ------- | ------------------------------------------------------------------ |
| `auto_download` | `true`  | Start downloading as soon as a check finds a newer stable release. |

The key is part of `UiPreferences` and is saved with the other main-window
preferences. A missing key reads as `true`, and `settings::ensure` adds it to an
existing settings file. Changing the switch affects later checks; it does not
start a check by itself.

In a Rust debug build the switch is shown as off and disabled, but the saved
preference is not changed. The manual update action and the global header action
are hidden. The **Release notes** action remains available; opening it runs a
normal feed check with automatic downloading forced off so the newest five
stable release notes can be loaded.

## Download and verification

For the selected release, the updater chooses the asset whose complete public
filename exactly matches the current target. A DMG, a backup, or a similarly
named file is not accepted.

Each download gets a unique temporary directory. Preparing a replacement removes
the previous staged directory. The asset is written while a SHA-256 is computed,
with a 600-second total timeout, and progress is published through `done` and
`total`.

The expected checksum comes from:

1. The asset's `digest` when it is a valid `sha256:<hex>` value.
2. Otherwise, the release's `SHA256SUMS` asset, matched by the complete filename.

No digest and no matching `SHA256SUMS` entry is an error. A checksum mismatch
removes the file and its download directory. Only a verified download becomes
staged and installable. On Windows the downloaded directory is left for the
NSIS installer after a successful install; other platforms remove it.

## Release notes

Stable release notes are cached at
`<data-directory>/state/update/releases.json`. The cache is versioned, limited
to 8 MiB, and written atomically. A missing cache is empty; a malformed cache is
a service-initialization error; an unwritable cache makes the check fail.

The cache stores:

- Up to five of the newest stable releases, with their complete bilingual bodies.
- Maintenance-only release versions separately from visible release bodies, so
  cached ranges can be re-filtered after the installed version changes.
- The current-to-target range for a pending or installed update.

On startup, a retained range is reused when the running build is unknown or
matches either endpoint. Otherwise recent notes are filtered to the running
release. After a check:

- A release build older than the latest release gets the range from its current
  version, exclusive, through the latest version, inclusive.
- An already-latest release build reuses a retained range ending at its version;
  without one, it gets its own release notes bounded below by the previous
  stable release.
- A source or debug build gets up to five of the newest stable releases.

Release bodies with no text are omitted. The range survives restarts so notes
remain available while an update is pending and after it has been installed.

Versions marked `<!-- aidget:internal-only -->` remain in the cached release set
for range selection but are filtered from normal note entries. Mixed ranges keep
their user-visible releases. If the whole range is maintenance-only,
`UpdateNotes.maintenanceOnly` makes the dialog show one localized notice without
category headings or version badges. Retaining those versions also lets a cache
re-filter correctly after installing a middle maintenance version. Older caches
default the new fields to empty or false.

The dialog selects the interface language from the release body. English is the
text before `<!-- lang:zh -->`; Chinese is the text after it. Rendering stops at
the `### Installation` or `### 安装说明` footer. Level-two headings become
categories, and entries from different releases are merged under the same
category. When more than one release is present, each entry carries its source
version badge.

The Markdown is converted into a restricted DOM; block quotes are sanitized
before insertion. Images are reduced to their alt text. Only HTTPS links are
kept clickable, and they open through the platform URL opener.

## Platform targets and installation

Release asset names omit the version:

| Platform                                | Updater asset              | Manual update                               | Install on quit               |
| --------------------------------------- | -------------------------- | ------------------------------------------- | ----------------------------- |
| macOS Apple Silicon                     | `aidget-darwin-arm64.zip`  | Unpack and replace the `.app`, then restart | Replace the `.app`, then exit |
| macOS Intel                             | `aidget-darwin-amd64.zip`  | Unpack and replace the `.app`, then restart | Replace the `.app`, then exit |
| Windows                                 | `aidget-windows-amd64.exe` | Run NSIS with restart                       | Run NSIS without restart      |
| Linux x86_64 with `dpkg`                | `aidget-linux-amd64.deb`   | `pkexec dpkg -i`, then restart              | Skipped                       |
| Linux x86_64 without `dpkg`, with `rpm` | `aidget-linux-amd64.rpm`   | `pkexec rpm -U`, then restart               | Skipped                       |

macOS supports the two released architectures. The Windows target is
`aidget-windows-amd64.exe`; there is no runtime architecture branch. Linux
requires x86_64 and detects `dpkg` and `rpm` on `PATH`; `dpkg` wins when both
exist. A platform, architecture, or package-manager combination without an
updater asset can still check the feed, but downloading reports that the
installation cannot update itself in place.

DMGs are for manual installation only. `SHA256SUMS` covers every updater asset.
There is no AppImage updater asset.

### macOS

The running executable is resolved through symlinks and must be inside
`Contents/MacOS` of an `.app` bundle. The parent directory is probed for
writability. The updater extracts the ZIP into `.aidget-update` beside the
bundle with `ditto -x -k`, requires an `.app` at the archive root, moves the old
bundle aside, moves the new one into place, and removes the previous copy after
success. If the new bundle cannot be moved into place, the old bundle is restored
before the error is returned.

Manual updates start the new executable after the replaced process exits.
Installing on quit leaves the new bundle for the next launch.

### Windows

The downloaded executable is Tauri's NSIS installer. A manual update launches it
with `/S /UPDATE /R`; current application arguments are preserved with `/ARGS`
after `/R`. Installing on quit uses `/S /UPDATE` and does not start the
application again. The installer stops the running application before copying,
so Aidget does not wait for replacement to finish.

### Linux

The updater runs the package manager through `pkexec`:

```
pkexec <absolute-dpkg-path> -i <staged-deb>
pkexec <absolute-rpm-path> -U <staged-rpm>
```

The tool path is resolved before elevation so the elevated command runs the
package manager selected by the application. A failed package command reports
that the previous version is still installed.

Package installation is skipped during quit because the password prompt would
appear while the window disappears. A manual update schedules the saved
executable after the old process exits; the package manager owns binary
replacement, so the saved install path remains the launch path.

### Restart

For macOS and Linux manual updates, a `/bin/sh` helper waits for the old process
to exit, then `exec`s the saved executable with its original arguments. Output
is appended to `<home>/logs/relaunch.log` under the current instance's data
directory, including a custom `--home`; `logs/` is created automatically. Starting
the saved executable directly avoids activating another bundle with the same
identifier.

Zombies count as exited because they have released IPC. CLI/MCP parents reap GUI
children on a background thread.

On Windows the NSIS installer performs the restart. If scheduling the
macOS/Linux replacement fails, Aidget reports that the update is in place but
must be launched manually.

## Concurrency and quitting

Only one operation may own the update service at a time. `checking`,
`downloading`, and `installing` are all considered in flight. A second check
returns the active snapshot, and an install or download that cannot claim the
service returns without changing it.

Closing the last window does not exit this tray application. A quit while a
replacement is running is deferred and released after the install settles. A
failed install releases the deferred quit without retrying the same replacement.
`UpdateState::installing` is not the quit gate by itself because a completed
install keeps that state until the process exits; a separate in-progress flag
covers only the file-replacement window.

On macOS, Cmd+Q can reach the native exit path without an `ExitRequested` event.
The exit handler therefore waits for an active replacement to finish instead of
allowing the process to disappear between bundle renames. If a quit races the
install claim, the losing path also waits once the claim is visible.

At normal exit, `install_on_quit` installs a staged update without restarting for
macOS and Windows. It does nothing when there is no staged download, when an
install has already settled during that exit, or when Linux elevation would be
required. Installing successfully on quit means the next launch is the new
version.

## Tauri surface

The main window can call:

- `update_state`: read the current snapshot.
- `update_check`: check the feed and download when the saved preference allows.
- `update_apply`: recheck, then download and/or install the release.

The commands are in `command_names::MAIN_COMMANDS`, are granted only by
`capabilities/main.json`, and each checks that the caller is the main window.
Widget windows, which load third-party content, cannot install updates.

`UpdateSnapshot` and the release-notes types are shared Rust types exported to
TypeScript with `pnpm typegen`. `src/runtime/update.rs` owns the launch check,
settings policy, event emission, quit handling, and relaunch; the window renders
the snapshot and invokes the commands.

## Code layout

```
src/update/
├── service.rs        state machine, ownership guards, install hand-off
├── service/install.rs
│                     per-request install outcomes and cleanup
├── snapshot.rs       UpdateState and UpdateSnapshot
├── github.rs         release feed, filtering, asset selection, errors
├── fetch.rs          HTTP client, URL security, token scope
├── download.rs       streaming download and text assets
├── stage.rs          per-download directory and asset preparation
├── verify.rs         digest and SHA256SUMS verification
├── notes.rs          retained release notes and range selection
├── target.rs         platform, architecture, package-manager selection
├── version.rs        semver comparison, build stamp, tag normalization
└── install/
    ├── macos.rs      bundle resolution, ZIP extraction, swap and rollback
    ├── windows.rs    NSIS flags and launch
    └── linux.rs      dpkg/rpm through pkexec and relaunch
```

The launch check and Tauri wiring are in `src/runtime/update.rs`; the main
window's rendering and polling are in `ui/src/main/update.ts` and
`ui/src/main/release-notes.ts`.

## Verification and limits

Local checks:

```bash
pnpm install --frozen-lockfile
pnpm exec vp check
pnpm test
pnpm typegen --check
cargo fmt --check
AIDGET_CHECK_TARGET=1 cargo check --lib
cargo test
```

The update tests use a local mock server and cover release filtering and
pagination, build eligibility, state transitions, download retries, checksum
handling, asset selection, release-notes caching, the macOS bundle swap, Linux
package commands, Windows installer arguments, and relaunch ordering. They do
not contact GitHub.

CI compiles and tests the platform-gated paths on macOS, Windows, and Linux, but
the automated suite does not perform a real NSIS installation, answer a Linux
`pkexec` prompt, or install from a read-only/DMG location. macOS and Windows
release artifacts are currently unsigned, so Gatekeeper or SmartScreen may still
warn on first launch.

The updater has no:

- recurring check or background daemon;
- beta/stable channel selection, mirrors, or proxy settings;
- custom feed, `latest.json`, or runtime repository override;
- resumable download or cross-restart staging;
- Homebrew, container, or AppImage detection;
- blocked-version bookkeeping;
- bare-file replacement for `.deb`, `.rpm`, or Windows;
- rollback after a completed replacement fails to start;
- automatic installation of a Linux package during quit.
