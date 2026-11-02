mod linux;
#[cfg(target_os = "macos")]
mod macos;
mod windows;

use std::path::Path;

use aidget::update::install::{relaunch_script, sh_quote};

#[test]
fn a_relaunch_waits_for_this_process_and_starts_the_new_binary() {
    let script = relaunch_script(
        Path::new("/Applications/aidget.app/Contents/MacOS/aidget"),
        4242,
        Path::new("/tmp/aidget-relaunch.log"),
    );

    assert!(
        script.contains("while kill -0 4242 2>/dev/null"),
        "{script}"
    );
    assert!(
        script.contains("exec '/Applications/aidget.app/Contents/MacOS/aidget' >>'/tmp/aidget-relaunch.log' 2>&1"),
        "{script}"
    );
    // A newline would end the wait loop and start the binary at once, which is the
    // race this script exists to avoid.
    assert!(!script.contains('\n'), "{script}");
}

#[test]
fn a_path_with_a_quote_survives_the_shell() {
    assert_eq!(
        sh_quote(Path::new("/tmp/it's here/aidget")),
        r"'/tmp/it'\''s here/aidget'"
    );
}

#[cfg(unix)]
#[test]
fn restart_script_executes_quoted_saved_path_once_after_owned_process_exits() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("it's the new executable");
    let output = root.path().join("start count");
    let log = root.path().join("it's a log");
    std::fs::write(
        &executable,
        format!("#!/bin/sh\nprintf 'started\\n' >>{}\n", sh_quote(&output)),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut old = Command::new("/bin/sh")
        .args(["-c", "read line"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // A marker after the first successful kill proves the restart observed a live
    // owned process; the exec itself still uses the production script.
    let observed = root.path().join("observed");
    let script = relaunch_script(&executable, old.id(), &log).replace(
        "do sleep",
        &format!("do touch {}; sleep", sh_quote(&observed)),
    );
    let mut restart = Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !observed.exists() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(!output.exists());
    drop(old.stdin.take());
    old.wait().unwrap();
    assert!(restart.wait().unwrap().success());
    assert_eq!(std::fs::read_to_string(output).unwrap(), "started\n");
}
