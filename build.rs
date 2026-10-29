use std::process::Command;

#[allow(dead_code)]
#[path = "src/runtime/command_names.rs"]
mod command_names;

/// What this build is, for the updater's "only releases replace themselves" rule.
///
/// Three cases, most specific first: HEAD on a tag is that release; a working tree
/// reports `git describe`, so `0.1.0-5-gabc1234` and `0.1.0-dirty` are
/// recognisable as builds from source; anything else falls back to the crate
/// version, which the release workflow injects from the tag.
fn build_stamp() -> String {
    // A release is built at a tag, and the workflow writes the version into the
    // manifests before building — which dirties the tree. So an exact tag decides,
    // not a clean working directory: judging by cleanliness would mark every
    // published build as one from source, and a release that cannot update itself
    // is the one thing the updater must not ship.
    if let Some(tag) = exact_tag() {
        return tag;
    }
    let base = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    described()
        .filter(|stamp| stamp.contains("-g") || stamp.ends_with("-dirty"))
        .unwrap_or(base)
}

/// The tag HEAD sits on, when it sits on one.
fn exact_tag() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .current_dir(std::env::var("CARGO_MANIFEST_DIR").ok()?)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tag = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!tag.is_empty()).then_some(tag)
}

fn described() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .current_dir(std::env::var("CARGO_MANIFEST_DIR").ok()?)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stamp = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!stamp.is_empty()).then_some(stamp)
}

/// Cross-target type-check hook.
///
/// `AIDGET_CHECK_TARGET=1` skips `tauri_build`, whose Windows resource step
/// needs `llvm-rc` and whose Linux step needs GTK dev packages, so a host can
/// type-check another platform's `cfg` branches with `cargo check --lib`. Real
/// builds never set it and stay strict.
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-env-changed=AIDGET_CHECK_TARGET");
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Before the cross-target early return: the updater's "only a release may
    // replace itself" rule reads this, and a type check should see it too.
    println!("cargo:rustc-env=AIDGET_BUILD_STAMP={}", build_stamp());
    if std::env::var_os("AIDGET_CHECK_TARGET").is_some() {
        return;
    }
    println!("cargo:rerun-if-changed=ui/src");
    println!("cargo:rerun-if-changed=widgets");
    println!("cargo:rerun-if-changed=ui/assets");
    println!("cargo:rerun-if-changed=ui/widget.html");
    println!("cargo:rerun-if-changed=ui/main.html");
    println!("cargo:rerun-if-changed=ui/vite.config.ts");

    // The UI is embedded via include_dir, so every rebuild must refresh
    // ui/dist. `.taurignore` keeps those writes from retriggering the watcher.
    // On Windows `pnpm` is a `.cmd` batch shim, which `CreateProcess` will not
    // resolve from a bare program name.
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "pnpm"]);
        command
    };
    #[cfg(not(windows))]
    let mut command = Command::new("pnpm");
    let status = command
        .args(["run", "build:ui"])
        .current_dir(&manifest)
        .status()
        .expect("failed to run `pnpm run build:ui` for the UI");
    assert!(status.success(), "pnpm run build:ui failed");
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(command_names::ALL_COMMANDS)),
    )
    .expect("cannot build application command permissions");

    embed_windows_test_manifest();
}

#[cfg(windows)]
fn embed_windows_test_manifest() {
    // Tauri only embeds this manifest into application binaries. Windows test
    // harnesses need it too, otherwise the loader fails before main().
    let manifest =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("windows-test-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg-tests=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg-tests=/MANIFESTINPUT:{}",
        manifest.display()
    );
}

#[cfg(not(windows))]
fn embed_windows_test_manifest() {}
