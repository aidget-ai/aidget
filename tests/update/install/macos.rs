use std::path::{Path, PathBuf};

use aidget::update::install::macos::{bundle_of, install};
use tempfile::TempDir;

#[test]
fn a_bundle_is_recognised_from_the_executable_inside_it() {
    let root = TempDir::new().unwrap();
    let bundle = fake_bundle(root.path(), "aidget.app", "old");
    let exe = bundle.join("Contents/MacOS/aidget");

    assert_eq!(bundle_of(&exe).unwrap(), bundle.canonicalize().unwrap());
}

#[test]
fn an_executable_outside_a_bundle_has_none() {
    let root = TempDir::new().unwrap();

    assert!(bundle_of(&root.path().join("aidget")).is_none());
    assert!(bundle_of(&root.path().join("Other/Contents/MacOS/aidget")).is_none());
}

#[test]
fn installing_replaces_the_bundle_and_keeps_no_previous_copy() {
    let root = TempDir::new().unwrap();
    let bundle = fake_bundle(root.path(), "aidget.app", "old");
    let zip = zip_of(root.path(), "new");

    install(&zip, &bundle).unwrap();

    assert_eq!(payload(&bundle), "new");
    assert!(!root.path().join(".aidget-update").exists());
}

#[test]
fn a_bundle_that_does_not_contain_an_app_is_refused_and_left_alone() {
    let root = TempDir::new().unwrap();
    let bundle = fake_bundle(root.path(), "aidget.app", "old");
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let other = empty.join("something");
    std::fs::create_dir_all(&other).unwrap();
    let zip = root.path().join("empty.zip");
    ditto(&["-c", "-k", "--keepParent"], &other, &zip);

    assert!(install(&zip, &bundle).is_err());
    assert_eq!(payload(&bundle), "old");
}

/// A minimal `.app` whose `payload.txt` holds `content`.
fn fake_bundle(parent: &Path, name: &str, content: &str) -> PathBuf {
    let bundle = parent.join(name);
    let macos = bundle.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::write(bundle.join("Contents/payload.txt"), content).unwrap();
    std::fs::write(macos.join("aidget"), content).unwrap();
    bundle
}

/// A zip holding one `.app` with `content` as its payload.
fn zip_of(parent: &Path, content: &str) -> PathBuf {
    let source = parent.join("source");
    std::fs::create_dir_all(&source).unwrap();
    let app = fake_bundle(&source, "aidget.app", content);
    let zip = parent.join("update.zip");
    // Releases must keep the .app wrapper, which `ditto` only does with
    // `--keepParent`.
    ditto(&["-c", "-k", "--keepParent"], &app, &zip);
    zip
}

fn ditto(args: &[&str], from: &Path, to: &Path) {
    let status = std::process::Command::new("ditto")
        .args(args)
        .arg(from)
        .arg(to)
        .status()
        .unwrap();
    assert!(status.success(), "ditto {args:?} failed");
}

fn payload(bundle: &Path) -> String {
    std::fs::read_to_string(bundle.join("Contents/payload.txt")).unwrap()
}

#[test]
fn a_symlink_launch_resolves_the_real_bundle_before_installation() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    let bundle = fake_bundle(root.path(), "it's aidget.app", "old");
    let exe = bundle.join("Contents/MacOS/aidget");
    let link = root.path().join("launch link");
    symlink(&exe, &link).unwrap();
    assert_eq!(bundle_of(&link), Some(bundle.canonicalize().unwrap()));
}

#[test]
fn native_direct_and_symlink_launch_capture_the_same_real_bundle() {
    use aidget::update::install::{macos::prepare, Installation};
    use std::os::unix::fs::symlink;
    use std::process::Command;
    let root = TempDir::new().unwrap();
    let bundle = fake_bundle(root.path(), "it's a native probe.app", "old");
    let executable = bundle.join("Contents/MacOS/aidget");
    let source = root.path().join("probe.rs");
    std::fs::write(
        &source,
        "fn main() { println!(\"{}\", std::env::current_exe().unwrap().display()); }",
    )
    .unwrap();
    assert!(Command::new("rustc")
        .arg(source)
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap()
        .success());
    let link = root.path().join("external launch");
    symlink(&executable, &link).unwrap();
    let linked_bundle = root.path().join("linked.app");
    symlink(&bundle, &linked_bundle).unwrap();
    for launch in [
        &executable,
        &link,
        &linked_bundle.join("Contents/MacOS/aidget"),
    ] {
        let output = Command::new(launch).output().unwrap();
        assert!(output.status.success());
        let reported = String::from_utf8(output.stdout).unwrap();
        let prepared = prepare(Path::new(reported.trim())).unwrap();
        let Installation::Mac {
            bundle: captured,
            executable: saved,
        } = prepared
        else {
            panic!("wrong platform")
        };
        assert_eq!(captured, bundle.canonicalize().unwrap());
        assert_eq!(saved, executable.canonicalize().unwrap());
        println!("native launch {} => {}", launch.display(), saved.display());
    }
    assert!(prepare(&root.path().join("missing")).is_err());
    let outside = root.path().join("outside");
    std::fs::write(&outside, "outside").unwrap();
    assert!(prepare(&outside).is_err());
    let broken = root.path().join("broken");
    symlink(root.path().join("gone"), &broken).unwrap();
    assert!(prepare(&broken).is_err());
    assert_eq!(payload(&bundle), "old");
    let prepared = prepare(&link).unwrap();
    let zip = zip_of(root.path(), "new");
    prepared.apply(&zip, false).unwrap();
    assert_eq!(payload(&bundle), "new");
}
