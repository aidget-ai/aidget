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
