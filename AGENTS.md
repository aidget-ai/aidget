# Coding Rules

1. Fail fast with explicit errors: do not add fallback logic unless explicitly requested.
2. Let exceptions and errors bubble up as early as possible: do not handle errors inside the business layer.
3. Use effective tests: first prove that the bug or issue exists by making the test fail. Do not only write tests that pass, since they prove nothing.
4. Document externally exposed types, interfaces, functions, and classes, and add functional comments to key logic branches.
5. Reuse utility functions from `utils` whenever possible. If a general-purpose function does not exist but can be extracted and reused, add it to `utils` and reuse it there.
6. When writing unit tests, strictly mirror the source structure in `tests` directory. Each unit test file must test only the functionality of its corresponding source file.
7. Do not add unit tests that match stylesheets or source files with regular expressions; styles should be verified through component behavior and the toolchain.
8. Always use source types from the owning library or module. Do not create local duplicate types, and do not assert values as locally substituted types just to work around TypeScript issues.
9. Avoid abstraction layers that do not simplify the code or maintain a real boundary. Prefer calling the owning module or service directly instead of passing values through broad wrapper objects layer by layer.
10. Do not add explicit return types to functions unless TypeScript cannot infer them correctly or an explicit annotation is required to preserve a public contract.
11. Do not extract one-off helper functions unless they maintain a real boundary, hide meaningful complexity, or are expected to be reused. Prefer inlining simple one-use logic.
12. Do not revert files that are already staged in Git.
13. Keep source files under 500 lines where practical; split them as needed to keep files short and focused. This limit does not apply to unit test files.
14. When write commit message, follow `.github/COMMIT_MESSAGE_INSTRUCTIONS.md`.

# Collaboration workflow

- Keep one pull request to one behavior change. Name the exact commands and
  platforms that were tested, plus anything that was not tested.
  check out, commit from, clean, or delete another session's worktree or logs.
- CI must pass and a reviewer other than the author must review the current
  head. Record the reviewed SHA before merge; a pushed head needs a new review.
- Follow [docs/development.md](docs/development.md) for local checks and the
  version model.

# Release workflow

- Follow [docs/releasing.md](docs/releasing.md).
- A release is an annotated `vMAJOR.MINOR.PATCH` or
  `vMAJOR.MINOR.PATCH-beta.N` tag on a reviewed commit that is already on
  `main`. Create the tag only after the commit is on `main` and `main` is green.
- Do not commit release version bumps. The tag is the release version; the
  workflow injects it into the three manifests before checks and packaging.
- Never move, delete or reuse a failed or published tag. A release that needs a
  code or workflow change gets a new commit and a new patch tag. Retry an
  unpublished tag only for a transient runner or service failure.
- `DEEPSEEK_API_KEY` is required for the bilingual release notes. Never commit
  it or print it in logs.
- A release is complete only when the workflow has published the seven platform
  packages listed in `docs/releasing.md` plus `SHA256SUMS`. Stable releases are
  marked Latest; beta releases are marked Pre-release and must not become Latest.
- Do not hand-upload, replace or remove assets on a published release.
- macOS and Windows artifacts are currently unsigned. Do not claim notarization
  or code signing.
- Every build checks updates only from `aidget-ai/aidget`. Do not add a runtime
  or compile-time product setting that redirects updates to another repository.
  Test repositories must be injected through `UpdateService::for_build`.
- The user's GitHub token may be sent only to HTTPS `api.github.com` for Release
  metadata. Installer and `SHA256SUMS` downloads must remain anonymous from the
  first `github.com` request through object-storage redirects.
