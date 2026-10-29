# Self-update

This document covers the design, implementation scope, and acceptance criteria
for Aidget's one-click self-update feature (Issue #2).

## Approach

- The app queries `GET https://api.github.com/repos/aidget-ai/aidget/releases/latest` at startup and on demand.
- Version, release notes, asset list and checksums come directly from the GitHub
  Release JSON. No separate feed service or `latest.json` is introduced.
- The update flow and state machine follow Magpie's design. The platform
  installation step adapts to Aidget's actual release formats.

### Checksum verification

1. Use `digest` from the GitHub asset object when present (`sha256:<hex>`).
2. Fall back to the `SHA256SUMS` release asset when `digest` is absent or null.
3. Refuse installation if neither is available.

### Which repository a build reads

| Build                          | Reads                         |
| ------------------------------ | ----------------------------- |
| `pnpm dev`, `pnpm build:debug` | `kalinplus/aidget` (the fork) |
| `pnpm build`, release workflow | `aidget-ai/aidget` (upstream) |

The switch is `cfg!(debug_assertions)` in `src/update/github.rs`, not a runtime
setting, so a release binary is physically unable to read the fork. A debug
build follows the fork so an update can be tested end to end without publishing
a release upstream.

Neither value is user-configurable, and the update never accepts a URL from
outside the release metadata it fetched.

---

## Decisions

### Read GitHub Releases directly

No feed adapter, `latest.json` or own server. GitHub's `releases/latest` already
excludes drafts and pre-releases, and its asset objects carry name, URL, size and
`digest`. An intermediate model would only earn its keep with a second feed, and
there is none. Magpie aggregates its own feed because it publishes outside
GitHub; Aidget publishes exactly what the app consumes.

### HTTP client: the one the repository already has

`reqwest` (blocking, `rustls`), `sha2` and `semver` are already dependencies of the
community feature, so the updater adds none of its own: a second HTTP client next
to the one that is already linked would be dead weight to argue for. Every call is
blocking and runs inside `spawn_blocking`, the way `main_widget_cached_preview`
already does.

`community::http::Http` is not reused directly: it answers "missing or
unavailable" and hides the status and the headers, while the updater needs both —
a 404 on `releases/latest` means nothing is published, and the rate-limit headers
say how long to wait. `update::fetch` builds its own client from the same settings
instead of widening that one.

`semver` compares with `cmp_precedence`, not `>`: the crate's `Ord` orders build
metadata as well, so `1.0.0+abc` would sort above `1.0.0`, and a rebuilt release
would read as a new one.

### A configured GitHub token is used for the check

The main window already stores an optional GitHub token for the community pages,
described in the app's own guide as sent only to `api.github.com`. The update check
sends it too: 60 requests an hour without one is one shared address's whole quota,
which is what turns a launch check into "GitHub rate limit reached" on a proxy
exit node. `reqwest` drops the `Authorization` header on the cross-host redirect
to asset storage, so the token reaches the API host only.

### Commands are reachable from the main window only

Release lookup, download and install are registered in `command_names::MAIN_COMMANDS`
and granted in `capabilities/main.json`, like every other main-window command. A
widget window loads third-party content and must not be able to install an update:
the ACL generator turns the inventory into one `allow-<command>` permission per
entry, and `command_access::authorize` refuses anything outside the window's group
before arguments are read.

### Exact asset matching

Magpie looks up a whole asset name in `rel.Assets[AppAsset()]`. Aidget's names
embed the version, so the match runs from the architecture token to the end of
the name (`_aarch64.app.zip`). Matching a token anywhere in the name was
rejected: a version such as `1.0.x64` would look like an x64 build.

### Build identity is compile time

`build.rs` emits `AIDGET_BUILD_STAMP` from `git describe --tags --always
--dirty`. `CARGO_PKG_VERSION` alone cannot tell a working tree from a release,
because both report a clean semver. A runtime environment override was rejected:
it would let any local process point the updater at another repository.

### Install kind from the executable

Following Magpie's `Homebrew()` (a path check) and its `APPIMAGE` variable: an
AppImage is recognised by `APPIMAGE` or by its own `.AppImage` name, and is
replaced at the file its runtime names rather than at `/tmp/.mount_*`. Outside an
AppImage, the distribution package is the one this machine's package manager can
install: `dpkg` if `PATH` has it, otherwise `rpm`, otherwise the build cannot
update itself in place. `dpkg` wins where both are present because `rpm` is
commonly installed on Debian-family systems only to read `.rpm` files.

### Windows: the installer installs

Windows cannot swap a running `.exe`, and the release carries an NSIS installer
rather than a bare executable, so the install is the installer's job. The flags
are Tauri's own: `/S` (silent), `/UPDATE`, and `/R` when the app should come back
by itself. A silent installer stops the running app before copying, so nothing
waits for it and a failure past that point cannot be reported in the window.
Tauri's `ShellExecuteW` does the same for the same reason.

### `state` says what is happening, `staged` what is available

The window decides which buttons to offer. Reading that off `state` made every
failure hide work that was still possible: a failed check hid a downloaded
update, a failed download offered no retry, and a failed install hid a verified
file that was still on disk and only needed another try. `UpdateSnapshot.staged`
carries the one local fact the buttons need, derived from the download on disk
whenever a snapshot is read, so state transitions cannot take it away.

### A source build is not "up to date"

`version::is_released` reads the build stamp: a `git describe` string, or a tree
with local changes, did not come from a tag. Such a build cannot swap itself for
a release without throwing away the developer's own work, so `settle` reports
`Source` rather than `Latest`. Collapsing the two would claim a version
comparison that never decided anything, and Magpie separates them the same way.

### Nothing published is a state, not a failure

`releases/latest` answers 404 for a repository with no release. That is where
upstream is before its first tag, and the window showed it as "update failed",
which is untrue: nothing was attempted and nothing broke. `github::latest`
returns `Option`, `None` for that exact body, and the service settles on
`NoRelease` — grey text, no fault banner, no red. Any other 404 keeps its status,
so a renamed repository does not silently read as "nothing to do".

### HTTPS, loopback excepted

An asset URL comes from the feed, which is fetched over HTTPS, so a plaintext
asset URL means the feed is not GitHub or was tampered with; `download` refuses
it. Loopback is the exception, because that is the local server the tests point
the service at, and the bytes still have to match a published checksum.

### Linux packages are installed through `pkexec`

`dpkg -i` / `rpm -U` need root, and the app is a desktop program with no
terminal. The command is run through `pkexec`, which asks the desktop's own
polkit agent, and the tool is passed as the absolute path `PATH` resolved: what
runs as root should be the program this build chose. A package install is never
started on quit, where the password prompt would appear as the window disappears
— the same rule Magpie applies.

### One check at a time

`begin()` refuses a second check while one is in flight, as Magpie does. Without
it, the launch check and a click would race, and later a download could start
twice.

### No scheduler

One check shortly after launch plus the Check button. The approved proposal says
so, and `CONTRIBUTING.md` requires a design discussion before adding a resident
process, poller or scheduler.

### deb/rpm are supported, not excluded

They upgrade through the system package manager via `pkexec`, which is the
correct way to update a packaged install. They are recorded as **not tested
locally**, which is a different statement from unsupported.

---

## Traps

Recorded so they are not rediscovered.

- **Asset names are not free-form.** `scripts/release/assets.mjs` builds
  `aidget_<version>_<arch>.<ext>` and rejects unknown extensions. Adding
  `.app.zip` needed a change there, not only in the workflow. The version has no
  leading `v`, because `parseTag` strips it.
- **`reqwest` in `Cargo.lock` means nothing.** It is a target-specific dependency
  for Apple platforms other than macOS and is not compiled here; the tree had no
  TLS crate at all.
- **`is_newer` and `is_released` answer different questions.**
  `0.1.0-3-gabc1234` parses fine as a pre-release, so it compares normally; it is
  `is_released` that refuses to let it update.
- **`git describe` is unreliable in CI.** Release checkouts are shallow and carry
  no tags, so `--always` yields a bare SHA. The stamp must fall back to the crate
  version for anything that is not a describe stamp, or CI builds would consider
  themselves builds-from-source.
- **Tests must not depend on the working tree.** Whether the tree is dirty decides
  whether `Found` is reachable, so the build identity is injected through
  `UpdateService::for_build` instead of read from the environment.
- **A mock server that never answers is a trap.** The HTTP client's timeout is
  30 s and the runtime waits for blocking tasks when it drops, so such a test
  turns a 0.3 s suite into 30 s. Answer late instead of never.
- **A release stamps itself by the tag it was built at, not by a clean tree.**
  The workflow writes the version into the manifests after checking out the tag,
  which dirties the tree, so `git describe --dirty` reports `v0.1.3-dirty` for the
  very builds that are releases. Judging by cleanliness marked every published
  build as one from source — and a release that cannot update itself is the one
  outcome the updater must never ship. `git describe --tags --exact-match` decides
  instead, and the dirty fallback stays for everything else. A consequence worth
  knowing while testing: once the repository has tags, every later commit reads as
  a build from source, so an acceptance build has to be made at a tag.
- **Do not relaunch with `open -a`.** LaunchServices resolves an application by
  its bundle identifier, so with another copy of aidget running — from an older
  test folder, say — `open -a <bundle>` activates that one and the bundle just
  swapped in never starts; the user is left looking at the version they thought
  they had replaced. The replaced executable is started directly instead, from the
  path this process ran from. Its output goes to a file in the temp directory,
  because a detached shell's would otherwise go nowhere.
- **Incremental builds cache the stamp.** `build.rs` only reruns for what it
  declares, so a rebuild after a checkout can keep the previous `git describe`
  result. Touch `build.rs` when the stamp matters — it cost two confusing rounds
  during acceptance.
- **A download test has to name this machine's asset.** The first CI run failed
  every test that downloaded on Linux and Windows while macOS passed, and the
  cause was not the client: the mock releases were named
  `aidget_v99.0.0_aarch64.app.zip` on every platform, so the service correctly
  refused to download a macOS asset on a machine that installs a `.deb` or a
  `-setup.exe`. The mock server now fills `__ASSET__` with the name
  `target::current()` yields, so the tests exercise the same selection the app
  does. A test that hard-codes one platform's asset proves nothing elsewhere.
- **A Tauri command that spawns must be `async`.** A synchronous command runs on
  the main thread, which has no Tokio runtime, so `tokio::spawn` there panics and
  the panic aborts the process (`failed to initiate panic`). `update_download`
  returns the `Downloading` snapshot at once and finishes in the background, so
  it is declared `async` like `update_check` and `update_install`; the manual
  Download button was the only path that reached it from a synchronous command.
- **A check must not discard a downloaded update.** Neither a failed feed nor a
  feed naming nothing newer says anything about a file that already downloaded
  and verified, so both keep the state on `ready` — the message alongside in the
  first case, the downloaded version in the second. Magpie keeps `ready` in both
  cases for the same reason, and the Integrations page keeps its last answer when
  a refresh fails.
- **Unauthenticated GitHub quota is per address, and shared behind a proxy.**
  The feed allows 60 requests an hour; a proxy exit node serves many people, so
  a `403` there is usually the quota, not a bad request. The answer is reported
  with `x-ratelimit-reset` so the window can say how long to wait.
- **A non-success answer is not a file.** A rejected asset request used to be
  written to disk and only caught by the checksum; the status is now checked
  before anything is streamed.
- **A failed download must take its directory with it.** Each download gets
  `<tmp>/aidget-update-<uuid>`; removing only the file on failure left empty
  folders behind on every attempt.
- **`ditto -c -k` alone loses the `.app` wrapper.** Releases must pass
  `--keepParent`, or the unpacked archive has `Contents/` at its root and the
  updater has nothing to swap in.
- **An async Tauri command cannot hold `State`.** It requires `'static`, so the
  service is cloned (shared `Arc` snapshot) before the `await` rather than made a
  global.

---

## Update state machine

```
checking → latest
         → found       (new version detected, auto-download off)
         → downloading → ready → installing → (restart)
         → error
```

Checking happens once shortly after launch (about 5 s) and whenever the user
clicks "Check for updates". There is deliberately no poller, scheduler or
recurring timer: the approved proposal states one check at startup plus a
manual button, and `CONTRIBUTING.md` requires a design discussion before adding
any background process or scheduler.

---

## Settings

| Key            | Default | Meaning                                            |
| -------------- | ------- | -------------------------------------------------- |
| `autoDownload` | `true`  | Download automatically when a new version is found |

A manual "Check for updates" button and a manual "Download update" button are
always available. The manual download button also works when `autoDownload` is
off, matching Magpie's behaviour where an explicit click always downloads.

`autoDownload` defaults to `true` for parity with Magpie. Settings persist in the
existing `~/.aidget/settings.toml`.

---

## Platform scope

The release workflow names assets `aidget_<version>_<arch>.<ext>`, where the
version carries no `v` (`parseTag` strips it); see `scripts/release/assets.mjs`,
which validates the exact list and rejects unknown extensions.

| Platform           | Update asset                   | Installation action                         |
| ------------------ | ------------------------------ | ------------------------------------------- |
| macOS arm64        | `aidget_X.Y.Z_aarch64.app.zip` | Extract, verify, replace `.app`, restart    |
| macOS x64          | `aidget_X.Y.Z_x64.app.zip`     | Same                                        |
| Windows x64        | `aidget_X.Y.Z_x64-setup.exe`   | Launch NSIS installer, exit current process |
| Linux x64 AppImage | `aidget_X.Y.Z_amd64.AppImage`  | Replace AppImage in place, restart          |
| Linux x64 deb      | `aidget_X.Y.Z_amd64.deb`       | `pkexec dpkg -i`, restart                   |
| Linux x64 rpm      | `aidget_X.Y.Z_amd64.rpm`       | `pkexec rpm -U`, restart                    |

Only the macOS rows work today: the Windows and Linux installation actions land
with phase 6 below, and `install::apply` reports that in-place update is not
available there until then.

The `.dmg` files remain for first-time installs. The `.app.zip` assets are
added to the release workflow specifically for in-app updates.

The `SHA256SUMS` file covers all update assets.

Asset selection matches the part of the name from the architecture token to the
end, the way Magpie looks up a full asset name. Only the version in the middle
is left unread, so a naming change on the release side cannot break the lookup:

- architecture: `aarch64` for Apple Silicon, `x64` for Intel and Windows x64,
  `amd64` for Linux
- suffix: `.app.zip`, `-setup.exe`, `.AppImage`, `.deb`, `.rpm`

A Linux build outside an AppImage is left unresolved until the installer lands:
which of `.deb` / `.rpm` applies is decided together with the upgrade command.

deb/rpm installation requires root. The app downloads and verifies the new
package, then invokes `pkexec dpkg -i` (deb) or `pkexec rpm -U` (rpm) so the
system package manager performs the upgrade — this is the correct way to
update a packaged install, not a bypass of it. AppImage updates need no root.

---

## Code layout

```
src/update/
├── mod.rs          update service, state machine, IPC commands
├── github.rs       GitHub releases/latest fetch and asset selection
├── version.rs      semver parsing and comparison
├── download.rs     HTTP download with progress
├── verify.rs       SHA-256 check, SHA256SUMS parser
└── install/
    ├── mod.rs
    ├── macos.rs    extract app.zip, verify, replace .app, relaunch
    ├── windows.rs  launch NSIS installer, exit
    └── linux.rs    AppImage replace; deb/rpm via pkexec dpkg/rpm
```

Tests follow the repository convention: every `src/` module has a matching file
under `tests/`, with a `mod.rs` per directory, mirroring the source tree:

```
tests/update/
├── mod.rs
├── github.rs
├── version.rs
├── download.rs
├── verify.rs
└── install/
    ├── mod.rs
    ├── macos.rs
    ├── windows.rs
    └── linux.rs
```

`tests/update/mod.rs` must be registered in `tests/main.rs` like the other
module trees. Frontend tests live in `ui/tests/` and mirror `ui/src/`.

Tauri commands exposed to the frontend:

```
update_state     → the last UpdateSnapshot, no network
update_check     → run a check, and download too when automatic downloading is on
update_download  → start a download (ignores the automatic setting); answers on
                   `downloading` and finishes in the background, so it must be
                   `async`: a synchronous command spawns on the main thread, where
                   there is no Tokio runtime and the panic aborts the process
update_install   → install the ready update, then start the new version and exit
```

`autoDownload` is not a command: it is part of `UiPreferences`, saved by
`set_preferences` with the theme and the language. The About page sends the whole
payload back, so a toggle cannot overwrite the other preferences with defaults.

The frontend never fetches GitHub directly. All update logic lives in the
backend.

---

## Version semantics

- Strip a leading `v` from `tag_name` before parsing.
- Compare as semver; pre-releases sort below the release they lead to.
- The default `releases/latest` endpoint already excludes drafts and
  pre-releases; no channel switching is implemented.

### Which builds may update themselves

The running build is identified by `AIDGET_BUILD_STAMP`, emitted by `build.rs`
from `git describe --tags --always --dirty`:

| Build                                                     | Stamp              | Offers an update |
| --------------------------------------------------------- | ------------------ | ---------------- |
| checked out at a clean tag, or a CI checkout without tags | `0.1.0`            | yes              |
| commits after the last tag                                | `0.1.0-5-gabc1234` | no               |
| uncommitted changes                                       | `0.1.0-dirty`      | no               |

So a build from a working tree reports the latest release as already current and
never replaces itself. Verifying the install path therefore needs a build that
counts as a release: commit, run the build on the now-clean tree, and run the
resulting bundle.

Local verification against the fork uses `pnpm build:debug`, not `pnpm build`:
a release build reads upstream, which has no releases, while a debug build reads
the fork. The build stamp comes from git, not from the profile, so a debug build
on a clean tree still counts as a release.

---

## What is explicitly out of scope for v0.1.0

- Custom feed service or `latest.json`
- beta / stable update channels
- Mirror or proxy configuration
- Homebrew path detection
- Container environment detection
- Blocked-version recording
- deb/rpm bare-file replacement (we use the system package manager instead)
- Windows bare-binary replacement (NSIS installer is used instead)
- Automatic rollback on failed installation
- Allowing users to set an arbitrary update repository in the UI

---

## Release workflow changes

1. Zip the `.app` with `ditto -c -k --keepParent`, so extracting restores the
   `aidget.app` wrapper. Without `--keepParent` the archive holds the bundle's
   contents instead and the updater refuses it.
2. Teach `scripts/release/assets.mjs` to name `.app.zip` sources
   (`aidget_<version>_aarch64.app.zip`, `_x64.app.zip`) and add both to its
   `expected` list.
3. Build the zipped `.app` for macOS arm64 and x64 in
   `.github/workflows/release.yml`.
4. Update the asset-count assertion (`-eq 7` → the new total).
5. `SHA256SUMS` already covers every collected asset; no change needed there.

---

## Testing

### Unit tests (all platforms, run locally on macOS)

Covered by Rust tests under `tests/update/`:

- Semver parsing, ordering, and edge cases (`dev`, dirty, pre-release)
- GitHub Release JSON parsing (draft, pre-release, missing asset, null digest)
- Asset selection per platform and architecture
- SHA-256 verification: correct, mismatch, missing SHA256SUMS entry
- State machine transitions and `autoDownload` / manual-download interactions
- Error and retry paths

Tests use a local mock HTTP server; no real GitHub requests.

### Fork release (integration)

Use `kalinplus/aidget` as the test update source. Publish a test release there
to validate the end-to-end check → download → verify cycle without touching
`aidget-ai/aidget` or its users.

The release workflow cannot publish to a fork as-is: `scripts/release/notes.mjs`
requires `DEEPSEEK_API_KEY`, and a fork has no such secret. A test release can
instead be built locally and uploaded with `gh release create`, which is enough
for the updater: it needs a tag, an asset whose name ends `_<arch>.app.zip`, and
the digest GitHub computes on upload.

Downloading release assets from GitHub is often blocked without a proxy on a
mainland connection. `reqwest` reads the standard proxy variables, so start the app
with `https_proxy=...` when testing the download path. The API answers directly
while assets do not, so `NO_PROXY=api.github.com` splits the two: the check goes
out unproxied and the asset comes through the proxy, without spending the
unauthenticated quota.

### macOS end-to-end (manual, primary acceptance gate)

Must pass on macOS arm64 against a fork release:

- [ ] New version detected; release notes displayed
- [ ] `found` state with auto-download off; download starts on click
- [ ] Download progress visible
- [ ] `ready` state after successful checksum verification
- [ ] Install and restart produces a running app at the new version
- [ ] Tampered digest → installation refused
- [ ] Missing macOS asset → `error` state
- [ ] App running from a non-writable location → clear error, no crash
- [ ] App running directly from a DMG → no attempt to replace a read-only bundle

### Linux (where available)

If a GUI environment is available: AppImage download, verify, replace, and
restart in a user-writable directory without root.

If only SSH access: run `cargo test` for the update module; verify AppImage
asset selection and path/permission logic.

deb/rpm: confirm asset selection picks the right `.deb` or `.rpm` and the
install command is `pkexec dpkg -i` / `pkexec rpm -U`. End-to-end install with
root + GUI + polkit is not tested locally; unit tests cover command
construction.

### Windows

No Windows machine is available. The following are covered by CI and unit tests:

- Windows x64 asset selection
- NSIS installer launch logic (unit-tested)
- Cross-platform `cargo check --lib`
- Full Windows bundle in the release workflow

The following are **not tested** and must be noted in the pull request:

- Windows native install and restart
- UAC elevation
- SmartScreen interaction
- File-locking behaviour during NSIS install

### CI

The existing check workflow covers macOS (full Rust tests) and Linux (library
check). The release workflow builds and packages all four targets. No additional
CI steps are required beyond the release workflow asset additions.

---

## Implementation phases

Each phase is independently verifiable; nothing merges on a later phase's
behaviour.

| Phase                  | Content                                                                                                                         | Verification                                                                                                                                                                                                                                                                                                                                         |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Skeleton            | `src/update/` module, state machine, Tauri commands, minimal Updates row on the About page; the check returns a stubbed version | Click through `pnpm dev` and see `checking → found`                                                                                                                                                                                                                                                                                                  |
| 2. Real check          | Semver comparison, GitHub `releases/latest` parsing, platform asset selection, startup check plus manual button                 | Unit tests against a local mock server, then one request against a real release                                                                                                                                                                                                                                                                      |
| 3. Download and verify | Download with progress, `digest` / `SHA256SUMS` verification, `autoDownload` persisted in `settings.toml`                       | Unit tests for success, mismatch and interruption                                                                                                                                                                                                                                                                                                    |
| 4. macOS install       | Extract `.app.zip`, replace the bundle, relaunch through a detached helper; a downloaded update also installs on quit           | Build two bundles locally and upgrade for real against a local server                                                                                                                                                                                                                                                                                |
| 5. Release workflow    | Add `.app.zip` assets and update the asset-count assertion                                                                      | Publish a test release on the fork and run the macOS upgrade again                                                                                                                                                                                                                                                                                   |
| 6. Windows and Linux   | NSIS installer launch, AppImage replacement, deb/rpm via `pkexec`                                                               | Unit tests; the Linux and Windows modules also compile on macOS, so the logic is type-checked there. `cargo check --lib` on those targets is left to CI: locally, `aws-lc-sys` (pulled in by the repository's `reqwest`) needs an MSVC toolchain for a Windows target and GTK headers for a Linux one. Untested items are listed in the pull request |
| 7. Delivery            | Update docs, walk the acceptance checklist, prepare the pull request                                                            | Evidence for each item in Issue #2                                                                                                                                                                                                                                                                                                                   |

Phase 1 comes first as a walking skeleton; the release workflow is deliberately
moved after the macOS installer because a local `.app.zip` plus a local HTTP
server is enough to exercise the whole path, while a fork release needs a full
four-platform build each round.

---

## Acceptance checklist

Checked against the implementation in this branch. `not run` means it was not
executed anywhere, not that it failed; the pull request repeats that list.

**Functionality**

- [x] Startup check (once, ~5 s after launch) — `update check Ready from
      kalinplus/aidget in 5.2s` in a debug build started 5 s earlier
- [x] Manual check — same log line after a click
- [x] `autoDownload` on / off — on: one startup check reaches `Ready`; off:
      reaches `Found` and stops (`auto_download = false` in `settings.toml`)
- [x] Manual download button (works with `autoDownload` off) — clicked through
      after the command layer was rebuilt: `update download Downloading` →
      `Ready`
- [x] Release notes rendered — carried in the snapshot and shown under the
      update row (unit tested; not confirmed by eye)
- [x] Download progress — `update download Downloading` then `Ready`, with the
      percentage drawn from `done` / `total`
- [x] SHA-256 verification — pass: a real 5 MB asset installed;
      fail: unit test `a_mismatched_checksum_fails_and_leaves_nothing_staged`
- [x] Retry after download error — `a_failed_download_leaves_the_release_downloadable_again`,
      and the button is offered from `staged`, not from `state`
- [x] `latest` state when already up to date — a release build whose version
      equals the feed's reports `Latest`
- [x] A build from a working tree reports `Source`, not `Latest`, and offers
      nothing to download
- [x] A repository with no release at all reports `NoRelease`, not a failure —
      a release build against upstream logs `update check NoRelease from
      aidget-ai/aidget`
- [x] Restart after install — macOS 0.1.0 → 0.1.1 → 0.1.2, bundle version and
      `CFBundleShortVersionString` both moved, and the relaunched window reported
      the new version
- [x] No recurring timer, poller or scheduler — one `tokio::time::sleep` at
      launch and nothing else; every later check comes from the button

**Platforms**

- [x] macOS arm64 full end-to-end (manual)
- [x] macOS x64 asset selection (unit test)
- [x] Windows x64 asset selection (unit test); installer flags (unit test)
- [ ] Windows x64 installer launch — `not run`: no Windows machine, and a
      cross-target `cargo check` stops in `ring` for want of an MSVC toolchain
- [x] Linux x64 AppImage replacement (unit test, real file swap in a temp dir)
- [ ] Linux x64 AppImage end-to-end — `not run`
- [x] Linux x64 deb/rpm asset selection and `pkexec` command (unit tests)
- [ ] Linux x64 deb/rpm end-to-end — `not run`

**Security**

- [x] Production repository hardcoded; not user-configurable — one constant,
      chosen at compile time
- [x] HTTPS only — `download::ensure_secure`, unit tested; loopback is the only
      exception and exists for the test server
- [x] No install without successful checksum — `verify` runs before `staged` is
      set, and `install` refuses an empty `staged`
- [x] No arbitrary download URL accepted — the only source is the feed's asset
      object, and it must be HTTPS
- [x] Test repository never compiled into a production binary — `#[cfg]`, and a
      release binary has zero occurrences of `kalinplus/aidget` where a debug one
      has one

**Pull request**

- [ ] Lists exact commands and platforms tested
- [ ] Explicitly names checks that were not run
- [ ] Links Issue #2
- [x] Updates this document if scope changes during implementation

**Added during implementation, not in the original checklist**

- [ ] The four commands answer the main window only; a widget window is refused
      before its arguments are read — by construction (`MAIN_COMMANDS`), not yet
      exercised by a test
- [x] `autoDownload` survives a toggle: theme stayed `light` and language stayed
      `en` across the switch

- [x] A failed check keeps a downloaded update installable — Magpie does the same
      and the Integrations page keeps its last answer
- [x] A check that finds the same version keeps it installable
- [x] Retry after an install failure, without a needless network round trip —
      `UpdateSnapshot.staged`
- [x] No password prompt on quit for `.deb` / `.rpm`
