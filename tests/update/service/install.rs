use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

#[test]
fn overlapping_install_does_not_authorize_a_second_exit_or_touch_staging() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("update.zip");
    std::fs::write(&path, "verified").unwrap();
    let service = UpdateService::for_build("unused", "owner/repo", "1.0.0");
    *service.staged.lock().unwrap() = Some(Staged {
        path: path.clone(),
        version: "2.0.0".into(),
    });
    let (entered, waiting) = mpsc::channel();
    let (resume, resumed) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let worker = service.clone();
    let thread = std::thread::spawn(move || {
        runtime().block_on(worker.install_with(move |_| {
            count.fetch_add(1, Ordering::SeqCst);
            entered.send(()).unwrap();
            resumed.recv().unwrap();
            Ok(Installation::Windows)
        }))
    });
    waiting.recv().unwrap();
    let exits = AtomicUsize::new(0);
    let refused = runtime().block_on(service.install_with(|_| panic!("second installer")));
    crate::runtime::update::complete_install(refused, |_| {
        exits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    let quit = service.install_blocking();
    assert!(path.exists());
    resume.send(()).unwrap();
    let completed = thread.join().unwrap();
    crate::runtime::update::complete_install(completed, |_| {
        exits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        exits.load(Ordering::SeqCst),
        1,
        "refused request authorized exit"
    );
    assert_eq!(quit.snapshot().state, UpdateState::Installing);
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn failed_install_is_retryable_but_completion_stays_exclusive() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("update.zip");
    std::fs::write(&path, "verified").unwrap();
    let service = UpdateService::for_build("unused", "owner/repo", "1.0.0");
    *service.staged.lock().unwrap() = Some(Staged {
        path: path.clone(),
        version: "2.0.0".into(),
    });
    let failed =
        runtime().block_on(service.install_with(|_| Err(Error::Other("cancelled".into()))));
    assert!(matches!(failed, InstallOutcome::Failed(_)));
    assert!(path.exists());
    assert!(failed.snapshot().staged);
    let exits = AtomicUsize::new(0);
    crate::runtime::update::complete_install(failed, |_| {
        exits.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    let retry = runtime().block_on(service.install_with(|_| Ok(Installation::Windows)));
    assert!(matches!(retry, InstallOutcome::Completed(..)));
    assert!(!path.exists());
    crate::runtime::update::complete_install(retry, |_| {
        Err(Error::Other("restart scheduling failed".into()))
    })
    .unwrap_err();
    assert!(matches!(
        service.install_blocking(),
        InstallOutcome::NotStarted(_)
    ));
    assert_eq!(exits.load(Ordering::SeqCst), 0);
}

#[test]
fn missing_staging_does_not_run_installer_or_authorize_exit() {
    let service = UpdateService::for_build("unused", "owner/repo", "1.0.0");
    let failed = runtime().block_on(service.install_with(|_| panic!("missing staging")));
    assert!(matches!(failed, InstallOutcome::Failed(_)));
    crate::runtime::update::complete_install(failed, |_| panic!("failed request exited")).unwrap();
}

#[test]
fn refused_download_does_not_release_install_ownership_even_without_feed_metadata() {
    let service = UpdateService::for_build("unused", "owner/repo", "1.0.0");
    service.snapshot.lock().unwrap().state = UpdateState::Installing;
    let snapshot = runtime().block_on(service.download(None));
    assert_eq!(snapshot.state, UpdateState::Installing);
}

#[test]
fn missing_staging_failed_outcome_keeps_error_after_another_operation_begins() {
    let service = UpdateService::for_build("unused", "owner/repo", "1.0.0");
    let active = service.clone();
    service.announce(move |snapshot| {
        if snapshot.state == UpdateState::Error {
            assert!(active.begin());
        }
    });
    let outcome = service.install_blocking();
    service.announce(|_| {});
    assert_eq!(service.snapshot().state, UpdateState::Checking);
    assert!(service.snapshot().error.is_none());
    let InstallOutcome::Failed(failed) = outcome else {
        panic!("expected failure")
    };
    assert_eq!(failed.state, UpdateState::Error);
    // The quit consumer unwraps this per-request error after publication.
    assert_eq!(failed.error.unwrap(), "no downloaded update to install");
}

#[test]
fn installer_failed_outcome_keeps_error_after_another_operation_begins() {
    let root = tempfile::tempdir().unwrap();
    let service = UpdateService::for_build("unused", "owner/repo", "1.0.0");
    let path = root.path().join("update.zip");
    std::fs::write(&path, "verified").unwrap();
    *service.staged.lock().unwrap() = Some(Staged {
        path: path.clone(),
        version: "2.0.0".into(),
    });
    let active = service.clone();
    service.announce(move |snapshot| {
        if snapshot.state == UpdateState::Error {
            assert!(active.begin());
        }
    });
    let outcome = runtime()
        .block_on(service.install_with(|_| Err(Error::Other("installer cancelled".into()))));
    service.announce(|_| {});
    assert_eq!(service.snapshot().state, UpdateState::Checking);
    assert!(service.snapshot().error.is_none());
    assert!(path.exists());
    let InstallOutcome::Failed(failed) = outcome else {
        panic!("expected failure")
    };
    assert_eq!(failed.state, UpdateState::Error);
    assert_eq!(failed.error.unwrap(), "installer cancelled");
}
