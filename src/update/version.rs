//! Version semantics for the update check.
//!
//! A release tag may carry a leading `v`; ordering and pre-release rules are
//! `semver`'s, which the repository already depends on, so only the two questions
//! the updater asks live here.

/// Parse `v?MAJOR.MINOR.PATCH(-pre)?(+build)?`.
///
/// Anything that is not a version is rejected, which is what keeps a
/// `git describe` stamp from being mistaken for a release.
fn parse(text: &str) -> Option<semver::Version> {
    let text = text.trim();
    semver::Version::parse(text.strip_prefix('v').unwrap_or(text)).ok()
}

/// Whether `candidate` is a later release than `current`.
///
/// `cmp_precedence` rather than `>`: the semver crate's `Ord` orders build
/// metadata too, so `1.0.0+abc` sorts above `1.0.0` — but §11 says metadata
/// plays no part in precedence, and a rebuilt release is not a new one.
///
/// Unparseable input on either side means no update, so a development build can
/// never be talked into replacing itself.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse(candidate), parse(current)) {
        (Some(candidate), Some(current)) => {
            candidate.cmp_precedence(&current) == std::cmp::Ordering::Greater
        }
        _ => false,
    }
}

/// Whether this build came from a release tag rather than a working tree.
///
/// The stamp is the crate version for a release and a `git describe` string
/// (`0.1.0-5-gabc1234`, `0.1.0-dirty`) for anything else. A pre-release tag is
/// still a tag, so only the two describe shapes rule a build out.
pub fn is_released(version: &str) -> bool {
    let Some(parsed) = parse(version) else {
        return false;
    };
    let pre = parsed.pre.as_str();
    !pre.contains("dirty") && !looks_like_describe(pre)
}

/// Whether a pre-release part is `git describe`'s `<commits>-g<hash>`.
fn looks_like_describe(pre: &str) -> bool {
    let Some((commits, rest)) = pre.split_once('-') else {
        return false;
    };
    let Some(hash) = rest.strip_prefix('g') else {
        return false;
    };
    !commits.is_empty()
        && commits.bytes().all(|byte| byte.is_ascii_digit())
        && !hash.is_empty()
        && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The version this build reports everywhere else.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// What this build actually is: [`current_version`] for a release, `git describe`
/// for one built from a working tree.
pub fn build_stamp() -> &'static str {
    env!("AIDGET_BUILD_STAMP")
}

/// Drop the `v` a release tag carries, so a version reads the same everywhere.
pub fn normalised(tag: &str) -> String {
    let tag = tag.trim();
    tag.strip_prefix('v').unwrap_or(tag).to_string()
}
