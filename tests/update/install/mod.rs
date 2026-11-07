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
        Path::new("/tmp/aidget-home/logs/relaunch.log"),
        &[],
    );

    assert!(
        script.contains("while kill -0 4242 2>/dev/null"),
        "{script}"
    );
    assert!(
        script.contains("exec '/Applications/aidget.app/Contents/MacOS/aidget' >>'/tmp/aidget-home/logs/relaunch.log' 2>&1"),
        "{script}"
    );
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
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" >>{}\n", sh_quote(&output)),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut old = Command::new("/bin/sh")
        .args(["-c", "read line"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Mark the wait loop to prove it observed the old process alive.
    let observed = root.path().join("observed");
    let script = relaunch_script(
        &executable,
        old.id(),
        &log,
        &[
            "--home".into(),
            root.path().join("custom home").into_os_string(),
        ],
    )
    .replace(
        "sleep 0.2",
        &format!("touch {}; sleep 0.2", sh_quote(&observed)),
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
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        format!("--home\n{}\n", root.path().join("custom home").display())
    );
}

#[cfg(unix)]
#[test]
fn restart_does_not_wait_for_an_exited_process_to_be_reaped() {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("restarted");
    let mut old = Command::new("/bin/sh")
        .args(["-c", "exit 0"])
        .spawn()
        .unwrap();
    // Keep the child unreaped, as a long-lived MCP parent can do after GUI exit.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let output = Command::new("ps")
            .args(["-p", &old.id().to_string(), "-o", "stat="])
            .output()
            .unwrap();
        if String::from_utf8_lossy(&output.stdout)
            .trim()
            .starts_with('Z')
        {
            break;
        }
        assert!(Instant::now() < deadline, "child did not become a zombie");
        std::thread::sleep(Duration::from_millis(20));
    }
    let script = relaunch_script(
        Path::new("/bin/sh"),
        old.id(),
        &root.path().join("relaunch.log"),
        &[
            "-c".into(),
            format!("printf started >{}", sh_quote(&marker)).into(),
        ],
    );
    let mut restart = Command::new("/bin/sh")
        .args(["-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let restarted_before_reaping = marker.exists();
    old.wait().unwrap();
    if !restarted_before_reaping {
        let _ = restart.kill();
    }
    let status = restart.wait().unwrap();
    assert!(restarted_before_reaping, "restart waited for a zombie PID");
    assert!(status.success());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "started");
}
