# Self-update

Design and acceptance criteria for Aidget's one-click self-update (Issue #2).

A few seconds after launch, and whenever **Check for updates** is pressed, the
running version is compared with the latest GitHub release. When it is newer the
release notes are shown, and one click downloads, verifies and installs it.
**Download updates automatically** decides whether the download starts on its own.
There is no poller, scheduler or recurring timer: `CONTRIBUTING.md` requires a
design discussion before adding one, and the approved proposal asks for a launch
check plus buttons.

## Repository and token scope

Every build reads releases from `aidget-ai/aidget`. Tests inject a local base URL
and repository through `UpdateService::for_build`; there is no runtime or
compile-time product setting that can redirect the updater to another repository.

## Decisions

**Read GitHub Releases directly.** No feed adapter, `latest.json` or own server:
`releases/latest` already excludes drafts and pre-releases, and its assets carry
name, URL, size and `digest`. An intermediate model would only earn its keep with a
second feed, and there is none.

**Reuse the client the repository already has.** `reqwest` (blocking, `rustls`),
`sha2` and `semver` come with the community pages, so the updater adds no
dependency; every call is blocking and runs inside `spawn_blocking`.
`community::http::Http` is not reused directly because it answers only "missing or
unavailable" and hides the status and the headers, while the updater needs both — a
404 means nothing is published, and `x-ratelimit-*` says how long to wait.
`update::fetch` builds its own client from the same settings. `semver` compares
with `cmp_precedence`, not `>`, because its `Ord` orders build metadata and a
rebuilt release would read as a new one.

**The configured GitHub token is used only for the metadata check.** Unauthenticated
traffic gets 60 requests an hour per address, which behind a proxy exit node turns
a launch check into "rate limit reached". `github_api_get` refuses to attach a
token unless the URL is HTTPS `api.github.com`. Installer and `SHA256SUMS` requests
never accept a token, so neither the first `github.com` download request nor the
cross-host redirect to object storage carries authorization.

**The commands are reachable from the main window only.** They are listed in
`command_names::MAIN_COMMANDS` and granted in `capabilities/main.json`, and
`command_access::authorize` refuses anything outside the window's group before its
arguments are read. A widget window loads third-party content and must not be able
to install an update.

**Assets match the complete public filename.** The updater constructs
`aidget-<platform>-<arch>.<ext>` and requires exact equality. Product, platform,
architecture and package type must all match; a similarly named file or a backup
cannot be selected.

**Build identity is decided by the tag.** `build.rs` emits `AIDGET_BUILD_STAMP`:

| HEAD                            | Stamp              | May replace itself |
| ------------------------------- | ------------------ | ------------------ |
| on a tag (the release workflow) | `v0.1.3`           | yes                |
| commits after a tag             | `0.1.0-5-gabc1234` | no                 |
| uncommitted changes, no tag     | `0.1.0-dirty`      | no                 |

The tag decides, not a clean tree: the release workflow writes the version into
the manifests after checking out the tag. A runtime repository override is
rejected because it would let any local process redirect the updater.

**Install kind comes from the package manager on the machine.** Linux uses the
package this machine's package manager can install: `dpkg` if `PATH` has it,
otherwise `rpm`, otherwise the build cannot update itself in place. `dpkg` wins
where both are present because `rpm` on a Debian machine is usually only used to
read `.rpm` files.

**Windows: the installer installs.** A running `.exe` cannot be swapped, and the
release carries an NSIS installer, so the flags are Tauri's own: `/S` (silent),
`/UPDATE`, and `/R` when the app should come back. A silent installer stops the
running app before copying, so nothing waits for it.

**Packages upgrade through `pkexec`.** `dpkg -i` / `rpm -U` need root, and the app
has no terminal: the command runs through `pkexec`, which asks the desktop's polkit
agent, with the tool passed as the absolute path `PATH` resolved. A package install
never starts on quit, where the password prompt would appear as the window
disappears — the rule Magpie applies too.

**`state` says what is happening, `staged` what is available.** The window derives
the available action from both fields. `UpdateSnapshot.staged` is read from the
download on disk whenever a snapshot is read, so a failed check, download or
install does not hide a verified file that can still be retried.

**"Up to date", "nothing published" and "this build cannot replace itself" are
three states, and none is an error.** A build from a working tree reports
`Source`; a repository with no release (404 on `releases/latest`) reports
`NoRelease`. Any other 404 keeps its status, so a renamed repository does not read
as "nothing to do".

The About page's version row keeps reporting the crate version, as
`docs/development.md` promises for the CLI, doctor, the MCP server, the main window
and the bundle alike. The `Source` state is what says a build cannot replace
itself; the version string is not asked to carry that.

**HTTPS, loopback excepted.** A plaintext asset URL means the feed is not GitHub or
was tampered with, so it is refused. Loopback is allowed because that is the local
server the tests use, and what it serves still has to match a published checksum.

**One check at a time.** A second check while one is in flight reads the running
one instead of starting another, and an install refuses to start while anything
else is in flight.

## States

```
idle → checking → latest
                → no_release
                → source
                → found ──(automatic or click)──> downloading → ready → installing → (restart)
                → error
```

`ready` is also reached from a failed check when a download is already on disk, and
from a check that finds the same version: the file is still installable either way.

## Settings

| Key             | Default | Meaning                                      |
| --------------- | ------- | -------------------------------------------- |
| `auto_download` | `true`  | Download as soon as a newer release is found |

A missing key reads as on, so a `settings.toml` written before the updater existed
behaves like a fresh one. It is part of `UiPreferences` and saved by
`set_preferences` with the theme and the language; a toggle sends the whole payload
back, so it cannot overwrite the other preferences with defaults. The Update
action works with automatic downloading off.

The About version row and the global page header share one stateful action.
Checking produces a three-second status on About. Once an update exists, the
action reads `Update`; clicking it calls `update_apply`, which fetches the latest
release again. A matching staged version installs directly, while a newer version
is downloaded and verified before installation. A failed recheck never installs
an older staged archive. The global copy is hidden on About because the version
row already provides the same action.

## Platform scope

Release assets use `aidget-<platform>-<arch>.<ext>` without a version in the
filename. `scripts/release/assets.mjs` validates the complete set.

| Platform            | Update asset               | Installation                                    |
| ------------------- | -------------------------- | ----------------------------------------------- |
| macOS Apple Silicon | `aidget-darwin-arm64.zip`  | Unpack beside the bundle, rename it in, restart |
| macOS Intel         | `aidget-darwin-amd64.zip`  | Unpack beside the bundle, rename it in, restart |
| Windows x64         | `aidget-windows-amd64.exe` | Run the NSIS installer, exit                    |
| Linux x64 DEB       | `aidget-linux-amd64.deb`   | `pkexec dpkg -i`, restart                       |
| Linux x64 RPM       | `aidget-linux-amd64.rpm`   | `pkexec rpm -U`, restart                        |

DMGs remain available for manual installation. macOS ZIPs retain the complete
`.app` wrapper, and `SHA256SUMS` covers every update asset. A build without a
supported package manager reports that it cannot update in place. AppImage is no
longer published; beta.1 and beta.2 AppImage users must manually migrate to DEB
or RPM because the next release has no AppImage asset to update from.

## Code layout

```
src/update/
├── mod.rs            module tree
├── service.rs        state machine, guards, install hand-off
├── snapshot.rs       UpdateState and UpdateSnapshot, the window's contract
├── github.rs         releases/latest, asset selection, failure wording
├── fetch.rs          the GitHub client: deadlines, HTTPS-only redirects, token
├── version.rs        semver questions, build stamp, tag normalisation
├── download.rs       streaming download with its SHA-256, asset text reads
├── verify.rs         digest and SHA256SUMS handling
├── stage.rs          which asset, and where it lands
└── install/
    ├── mod.rs        per-platform dispatch and relaunch
    ├── macos.rs      unpack, swap with rollback, start the new one
    ├── windows.rs    NSIS flags and launch
    └── linux.rs      deb/rpm through pkexec
```

The Tauri endpoints live in `src/runtime/commands.rs` with every other command;
the launch check, the settings the policy needs and install-on-quit live in
`src/runtime/update.rs`. The window renders from `update_state` and never talks to
GitHub itself.

`tests/` mirrors `src/`: `tests/update/*.rs` and `tests/update/install/*.rs`, each
directory with its `mod.rs`, registered in `tests/main.rs`. Frontend tests live in
`ui/tests/` and mirror `ui/src/`.

## Traps

- **A launch check cannot be discovered by polling.** The window reads the snapshot
  when it loads, which is before the check starts, and `idle` stops the poll. Every
  published snapshot is announced on `aidget://update` instead, so the window hears
  about a check nobody asked for.
- **Relaunch waits for the old process on every platform.** On Linux the new
  instance finds the old one over IPC, asks it to reopen, and exits, leaving nothing
  running; both platforms now wait on the pid before starting the replacement.
- **Watching `.git/HEAD` alone misses commits.** Declare the branch and tag refs
  plus `packed-refs`; otherwise a cached build stamp can survive the commit that
  should change it. A build with no tag and no describe reports
  `<version>-untagged`.
- **A Tauri command that performs network and file work must be `async`.** A
  synchronous command runs on the main thread, where blocking work stalls the
  window. `update_apply` publishes download progress while the replacement runs.
- **Do not relaunch with `open -a`.** LaunchServices resolves an application by its
  bundle identifier, so with another copy running it activates that one and the
  bundle just swapped in never starts. The replaced executable is started directly.
  Its output goes to a file in the temp directory; a detached shell's goes nowhere.
- **Incremental builds cache the stamp.** `build.rs` reruns only for what it
  declares, so a rebuild after a checkout can keep the previous `git describe`
  result. Touch `build.rs` when the stamp matters.
- **Acceptance builds must be made at the release tag.** Any later commit reports
  as a build from source.
- **Download tests use the current platform asset.** The mock server fills
  `__ASSET__` from `target::current()` so platform selection is tested rather than
  bypassed.
- **A check must not discard a downloaded update.** Neither a failed feed nor a feed
  naming nothing newer says anything about a file that already downloaded and
  verified. Magpie keeps `ready` in both cases for the same reason.
- **`ditto -c -k` alone loses the `.app` wrapper.** Releases must pass
  `--keepParent`, or the archive holds `Contents/` at its root and there is nothing
  to swap in.
- **A macOS cross-target check stops in `aws-lc-sys`.** The `reqwest` entry is
  target-specific on non-macOS Apple platforms and is not the toolchain used by a
  Windows build.
- **Unauthenticated quota is per address and shared behind a proxy.** A `403` there
  is usually the quota, so the answer is reported with `x-ratelimit-reset` and the
  window says how long to wait.
- **An async Tauri command cannot hold `State`.** It requires `'static`, so the
  service is cloned (it shares one `Arc` snapshot) before the `await`.
- **`UpdateState::Installing` is not the quit gate.** A completed replacement keeps
  that state until the process exits, so deferring on it would also block the
  relaunch exit. A separate `installing` flag covers only the file-swap window: a
  quit that arrives there is recorded in `ExitRequested` and released once the
  install settles; a failed install exits without retrying the same replacement.
- **macOS Cmd+Q never sends `ExitRequested`.** The native quit reaches `Exit`
  directly, so `install_on_quit` waits for an active replacement to settle instead
  of treating the busy service as `NotStarted` and letting the process die between
  the two bundle renames. If the quit races the claim, the `NotStarted` arm
  re-checks the gate and waits too. The wait ends when replacement finishes; a
  native quit can still win the race against the relaunch script, leaving the new
  version installed for the next launch without an automatic restart.

## Verification

```bash
pnpm install --frozen-lockfile
pnpm exec vp check
pnpm test
pnpm typegen --check
cargo fmt --check
AIDGET_CHECK_TARGET=1 cargo check --lib
cargo test
```

All three platforms run the same suite in CI (macos-latest, ubuntu-22.04,
windows-2022), so the `cfg(target_os = "windows")` and `cfg(target_os = "linux")`
install paths are compiled and tested, not only the macOS one. A cross-target
`cargo check --lib` from macOS is not possible: `aws-lc-sys`, pulled in by the
repository's `reqwest`, needs an MSVC C toolchain.

**Unit tests** (`tests/update/`) cover version comparison and the describe-stamp
rule, release JSON parsing, per-platform asset selection, the digest and
`SHA256SUMS` paths, the state machine including its refusals, download failures and
retries, the macOS swap, and the `pkexec` command lines. They run against a local
mock server, never against GitHub.

The current automated suite does not exercise these platform operations end to
end:

- Windows installer launch: CI compiles and tests the path, no NSIS install was
  executed on Windows. UAC elevation, SmartScreen and file locking during a silent
  install are likewise unverified.
- Linux `.deb` / `.rpm` end to end: the commands are asserted, the prompt they
  need cannot be answered in CI.
- Running from a read-only folder or straight from a DMG: `ensure_writable` writes
  the advice, it was not run against such a folder.

## Out of scope

Custom feed service or `latest.json`; beta/stable channels; mirrors or proxy
configuration; Homebrew or container detection; blocked-version bookkeeping;
bare-file replacement for `.deb` / `.rpm` / Windows; automatic rollback after a
failed install; a user-configured update repository.
