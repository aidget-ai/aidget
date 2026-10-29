# Self-update

Design, scope and acceptance record for Aidget's one-click self-update (Issue #2).

A few seconds after launch, and whenever **Check for updates** is pressed, the
running version is compared with the latest GitHub release. When it is newer the
release notes are shown, and one click downloads, verifies and installs it.
**Download updates automatically** decides whether the download starts on its own.
There is no poller, scheduler or recurring timer: `CONTRIBUTING.md` requires a
design discussion before adding one, and the approved proposal asks for a launch
check plus buttons.

## Which repository a build reads

| Build                              | Reads                         |
| ---------------------------------- | ----------------------------- |
| `pnpm dev`, `pnpm build:debug`     | `kalinplus/aidget` (the fork) |
| `pnpm build`, the release workflow | `aidget-ai/aidget` (upstream) |

`#[cfg(debug_assertions)]` decides, not a runtime setting, so a release binary
cannot read the fork and cannot be pointed at another repository. Neither value is
user-configurable, and the only download URL the updater accepts comes from the
release metadata it fetched.

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

**The configured GitHub token is used for the check.** The main window already
stores one for the community pages, sent only to `api.github.com`. Unauthenticated
traffic gets 60 requests an hour per address, which behind a proxy exit node turns
a launch check into "rate limit reached". `reqwest` drops the `Authorization`
header on the cross-host redirect to asset storage, so the token reaches the API
host only.

**The commands are reachable from the main window only.** They are listed in
`command_names::MAIN_COMMANDS` and granted in `capabilities/main.json`, and
`command_access::authorize` refuses anything outside the window's group before its
arguments are read. A widget window loads third-party content and must not be able
to install an update.

**Assets match from the architecture token to the end of the name.** Aidget's names
embed the version, so `_aarch64.app.zip` is what identifies the file; matching a
token anywhere in the name was rejected because a version such as `1.0.x64` would
look like an x64 build.

**Build identity is decided by the tag.** `build.rs` emits `AIDGET_BUILD_STAMP`:

| HEAD                            | Stamp              | May replace itself |
| ------------------------------- | ------------------ | ------------------ |
| on a tag (the release workflow) | `v0.1.3`           | yes                |
| commits after a tag             | `0.1.0-5-gabc1234` | no                 |
| uncommitted changes, no tag     | `0.1.0-dirty`      | no                 |

The tag decides, not a clean tree: the release workflow writes the version into
the manifests after checking out the tag, which dirties the tree. Judging by
cleanliness marked every published build as one from source. A runtime override
was rejected too: it would let any local process point the updater elsewhere.

**Install kind comes from the executable and the platform.** An AppImage is
recognised by `APPIMAGE` or by its own name, and is replaced at the file its
runtime names rather than at `/tmp/.mount_*`. Outside one, the package is the one
this machine's package manager can install: `dpkg` if `PATH` has it, otherwise
`rpm`, otherwise the build cannot update itself in place. `dpkg` wins where both
are present because `rpm` on a Debian machine is usually only used to read `.rpm`
files.

**Windows: the installer installs.** A running `.exe` cannot be swapped, and the
release carries an NSIS installer, so the flags are Tauri's own: `/S` (silent),
`/UPDATE`, and `/R` when the app should come back. A silent installer stops the
running app before copying, so nothing waits for it.

**Packages upgrade through `pkexec`.** `dpkg -i` / `rpm -U` need root, and the app
has no terminal: the command runs through `pkexec`, which asks the desktop's polkit
agent, with the tool passed as the absolute path `PATH` resolved. A package install
never starts on quit, where the password prompt would appear as the window
disappears — the rule Magpie applies too.

**`state` says what is happening, `staged` what is available.** The window decides
which buttons to offer. Reading that off `state` made every failure hide work that
was still possible: a failed check hid a downloaded update, a failed download
offered no retry, a failed install hid a verified file that only needed another
try. `UpdateSnapshot.staged` is derived from the download on disk whenever a
snapshot is read, so a state transition cannot take it away.

**"Up to date", "nothing published" and "this build cannot replace itself" are
three states, and none is an error.** A build from a working tree reports
`Source`; a repository with no release (404 on `releases/latest`) reports
`NoRelease`. Any other 404 keeps its status, so a renamed repository does not read
as "nothing to do".

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
back, so it cannot overwrite the other preferences with defaults. The Download
button works with the setting off, which is what makes turning it off useful.

## Platform scope

Release assets are `aidget_<version>_<arch><suffix>`, the version without `v`
(`scripts/release/assets.mjs` validates the exact list and rejects unknown
extensions).

| Platform           | Asset                         | Installation                                    |
| ------------------ | ----------------------------- | ----------------------------------------------- |
| macOS arm64 / x64  | `..._aarch64.app.zip`, `_x64` | Unpack beside the bundle, rename it in, restart |
| Windows x64        | `..._x64-setup.exe`           | Run the NSIS installer, exit                    |
| Linux x64 AppImage | `..._amd64.AppImage`          | Replace the file in place, restart              |
| Linux x64 deb      | `..._amd64.deb`               | `pkexec dpkg -i`, restart                       |
| Linux x64 rpm      | `..._amd64.rpm`               | `pkexec rpm -U`, restart                        |

Architecture tokens: `aarch64`, `x64`, `amd64`. `.dmg` stays for first-time
installs; the zipped `.app` exists for in-app updates. `SHA256SUMS` covers every
update asset. A repository with a package manager this build cannot use reports
that it cannot update itself in place rather than guessing.

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
    └── linux.rs      AppImage replacement, deb/rpm through pkexec
```

The Tauri endpoints live in `src/runtime/commands.rs` with every other command;
the launch check, the settings the policy needs and install-on-quit live in
`src/runtime/update.rs`. The window renders from `update_state` and never talks to
GitHub itself.

`tests/` mirrors `src/`: `tests/update/*.rs` and `tests/update/install/*.rs`, each
directory with its `mod.rs`, registered in `tests/main.rs`. Frontend tests live in
`ui/tests/` and mirror `ui/src/`.

## Traps

Recorded so they are not rediscovered.

- **A Tauri command that spawns must be `async`.** A synchronous command runs on the
  main thread, which has no Tokio runtime, so `tokio::spawn` panics and the panic
  aborts the process. `update_download` answers with the `Downloading` snapshot at
  once and finishes in the background.
- **Do not relaunch with `open -a`.** LaunchServices resolves an application by its
  bundle identifier, so with another copy running it activates that one and the
  bundle just swapped in never starts. The replaced executable is started directly.
  Its output goes to a file in the temp directory; a detached shell's goes nowhere.
- **Incremental builds cache the stamp.** `build.rs` reruns only for what it
  declares, so a rebuild after a checkout can keep the previous `git describe`
  result. Touch `build.rs` when the stamp matters.
- **Once the repository has tags, every later commit reads as a build from
  source.** An acceptance build has to be made at a tag.
- **A download test has to name this machine's asset.** A mock release named
  `..._aarch64.app.zip` on every platform makes the service correctly refuse to
  download a macOS asset on a machine that installs a `.deb`. The mock server fills
  `__ASSET__` from `target::current()`.
- **A check must not discard a downloaded update.** Neither a failed feed nor a feed
  naming nothing newer says anything about a file that already downloaded and
  verified. Magpie keeps `ready` in both cases for the same reason.
- **`ditto -c -k` alone loses the `.app` wrapper.** Releases must pass
  `--keepParent`, or the archive holds `Contents/` at its root and there is nothing
  to swap in.
- **`reqwest` in `Cargo.lock` means nothing.** It is target-specific for non-macOS
  Apple platforms and is not compiled here; `aws-lc-sys` is what a cross-target
  Windows check stops in.
- **A non-success answer is not a file.** A rejected asset request used to be
  written to disk and only caught by the checksum; the status is checked first.
- **A failed download takes its directory with it.** Each download gets
  `<tmp>/aidget-update-<uuid>`; removing only the file left empty folders behind.
- **Unauthenticated quota is per address and shared behind a proxy.** A `403` there
  is usually the quota, so the answer is reported with `x-ratelimit-reset` and the
  window says how long to wait.
- **An async Tauri command cannot hold `State`.** It requires `'static`, so the
  service is cloned (it shares one `Arc` snapshot) before the `await`.

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
retries, the macOS swap, the AppImage replacement against real files, and the
`pkexec` command lines. They run against a local mock server, never against GitHub.

**End to end on macOS arm64**, against releases published in the fork with a
throwaway `AIDGET_HOME`:

| Step                | Evidence                                                     |
| ------------------- | ------------------------------------------------------------ |
| Launch check        | `update check Found … in 0.7s`, then `Ready`                 |
| Manual check        | the same line after the button                               |
| Auto-download off   | `Found` and stops; the button downloads                      |
| Progress            | `update download Downloading` → `Ready`                      |
| Checksum            | the asset matches the release `digest`                       |
| Install and restart | 0.1.0 → 0.1.3, and the relaunched window reports the new one |
| Toggle              | theme and language survive the preference write              |
| Up to date          | `update check Latest`                                        |
| Nothing published   | `update check NoRelease` against upstream                    |
| Build from source   | `update check Source`, with the About page naming the build  |
| Rate limit          | `GitHub rate limit reached; try again in about 9 minute(s)`  |

Two environment notes for repeating this: the release workflow cannot publish to a
fork (`scripts/release/notes.mjs` needs `DEEPSEEK_API_KEY`), so a test release is
built locally and uploaded with `gh release create`; and where GitHub is only
reachable through a proxy, `https_proxy=… NO_PROXY=api.github.com` sends the check
directly and the asset through the proxy without spending the quota.

**Not run**, and repeated in the pull request:

- Windows installer launch: CI compiles and tests the path, no NSIS install was
  executed on Windows. UAC elevation, SmartScreen and file locking during a silent
  install are likewise unverified.
- Linux AppImage end to end, and `.deb` / `.rpm` end to end: the commands are
  asserted, the prompt they need cannot be answered in CI.
- A widget window being refused: guaranteed by the command inventory, not exercised
  by a test.
- Running from a read-only folder or straight from a DMG: `ensure_writable` writes
  the advice, it was not run against such a folder.
- The release notes row on screen: the notes reach the window (tested), the row was
  not confirmed by eye.

## Out of scope

Custom feed service or `latest.json`; beta/stable channels; mirrors or proxy
configuration; Homebrew or container detection; blocked-version bookkeeping;
bare-file replacement for `.deb` / `.rpm` / Windows; automatic rollback after a
failed install; a user-configured update repository.
