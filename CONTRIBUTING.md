# Contributing to aidget

This guide exists to keep review and maintenance predictable.

## Philosophy

aidget is one binary with one resident GUI process. It deliberately has no
daemon, data fetcher or scheduler: the user's Agent writes widget content, the
filesystem is the source of truth, and the GUI renders it.

Keep the core focused. A feature that adds a second resident process, a hidden
network client, a background data pipeline, or an abstraction without a real
boundary needs an explicit design discussion before implementation.

## The one rule

**You must understand your code.** You must be able to explain what it changes,
how it interacts with the rest of aidget, and what can fail.

AI-assisted work is welcome. Unreviewed agent output, code that cannot be
explained, or a patch whose tests and claims you cannot defend is not.

## Issues

Anyone may open an Issue. Use one of the repository's Issue Forms.

- Keep it short, concrete and readable. If it needs more than one screen, it is
  probably too long.
- Describe the problem in your own words. Do not submit an unreviewed wall of
  agent-generated text.
- Include a minimal reproduction, the aidget version and the platform.
- Use a throwaway data directory with `--home` for reproductions.
- Remove credentials, account identifiers, private widget contents and personal
  data from logs and screenshots.
- Explain why the problem matters and whether you want to implement the change.

Low-quality, duplicate or unactionable Issues may be closed manually. Opening an
Issue does not require approval.

## Before creating a pull request

**Pull requests are gated by an explicit contributor allowlist.** The allowlist
only controls who may contribute code; it grants no repository permissions.

If you are not yet approved:

1. Open a Contribution Proposal Issue.
2. Agree on the problem, scope and approach with an admin.
3. If the proposal is accepted, an admin replies with `lgtmi` at the start of a
   comment. The workflow adds the Issue author to
   `.github/APPROVED_CONTRIBUTORS`.
4. Open the pull request. Unapproved pull requests are closed automatically.

`lgtmi` approves the author of the Issue it is posted on and only adds that
GitHub handle to the repository's contribution allowlist. Admins may remove the
handle from that file to revoke future pull request access.

Do not publish release tags or GitHub Releases unless you are a maintainer or an
agent explicitly acting for one.

## Verification

Run the checks that cover your change:

```bash
pnpm install --frozen-lockfile
pnpm check
pnpm test
cargo fmt --check
AIDGET_CHECK_TARGET=1 cargo check --lib
cargo test
```

Use `--home /tmp/aidget-<name>` for runtime checks. Never test against the real
`~/.aidget` or a live Agent configuration.

Platform-specific behavior must be tested on that platform and named in the pull
request. A cross-target `cargo check --lib` is not a runtime test.

## Pull requests

- Keep one pull request focused on one behavior change.
- In the pull request description, list the exact commands and platforms tried.
- Link the Contribution Proposal that received `lgtmi`.
- Include a test that fails without the fix when fixing behavior.
- Update the README or `docs/` when the observable behavior or workflow changes.
- List checks that were not run instead of omitting them.

## Security

Do not open a public Issue for a security vulnerability. Follow
[SECURITY.md](SECURITY.md).
