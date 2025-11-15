use room_planner::updates::{self, Asset, Release, Transport};
use std::sync::atomic::{AtomicUsize, Ordering};
struct Fake {
    requests: AtomicUsize,
    bytes: Vec<u8>,
}
impl Transport for Fake {
    fn bytes(
        &self,
        _url: &str,
        limit: u64,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Vec<u8>, String> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        if self.bytes.len() as u64 > limit {
            return Err("size limit".into());
        }
        progress(self.bytes.len() as u64);
        Ok(self.bytes.clone())
    }
}
fn release(version: &str) -> Release {
    Release {
        tag_name: format!("v{version}"),
        draft: false,
        prerelease: false,
        assets: vec![Asset {
            name: format!("RoomPlanner-v{version}.exe"),
            browser_download_url: format!(
                "https://github.com/siarheisinelnikau/room-planner/releases/download/v{version}/RoomPlanner-v{version}.exe"
            ),
            size: 1024,
            digest: Some(format!("sha256:{}", "a".repeat(64))),
        }],
    }
}
#[test]
fn semantic_versions_choose_latest_compatible_release() {
    let releases = vec![release("0.2.0"), release("0.10.0"), release("0.9.0")];
    let selected = updates::choose(releases, &"0.1.0".parse().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(selected.version.to_string(), "0.10.0");
}
#[test]
fn drafts_prereleases_foreign_urls_and_wrong_assets_are_skipped() {
    let mut draft = release("1.0.0");
    draft.draft = true;
    let mut prerelease = release("2.0.0-beta.1");
    prerelease.prerelease = true;
    let mut foreign = release("3.0.0");
    foreign.assets[0].browser_download_url = "https://example.com/fake.exe".into();
    let mut wrong = release("4.0.0");
    wrong.assets[0].name = "OtherApp.exe".into();
    assert!(
        updates::choose(
            vec![draft, prerelease, foreign, wrong],
            &"0.1.0".parse().unwrap()
        )
        .unwrap()
        .is_none()
    );
}
#[test]
fn older_and_equal_versions_do_not_trigger_install() {
    assert!(
        updates::choose(
            vec![release("0.1.0"), release("0.0.9")],
            &"0.1.0".parse().unwrap()
        )
        .unwrap()
        .is_none()
    );
}
#[test]
fn server_errors_are_not_reported_as_current() {
    let fake = Fake {
        requests: AtomicUsize::new(0),
        bytes: br#"{"message":"Not Found"}"#.to_vec(),
    };
    assert!(updates::check(&fake).is_err());
    assert_eq!(fake.requests.load(Ordering::Relaxed), 1);
}
#[test]
fn digest_mismatch_preserves_installed_file() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("RoomPlanner.exe");
    std::fs::write(&target, b"original").unwrap();
    let available = updates::choose(vec![release("0.2.0")], &"0.1.0".parse().unwrap())
        .unwrap()
        .unwrap();
    let fake = Fake {
        requests: AtomicUsize::new(0),
        bytes: vec![0; 1024],
    };
    assert!(updates::download(&fake, &available, &target, &mut |_| {}).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
}
#[test]
fn missing_integrity_metadata_blocks_download() {
    let mut r = release("0.2.0");
    r.assets[0].digest = None;
    let available = updates::choose(vec![r], &"0.1.0".parse().unwrap())
        .unwrap()
        .unwrap();
    let fake = Fake {
        requests: AtomicUsize::new(0),
        bytes: vec![0; 1024],
    };
    let temp = tempfile::tempdir().unwrap();
    assert!(
        updates::download(&fake, &available, &temp.path().join("app.exe"), &mut |_| {}).is_err()
    );
    assert_eq!(fake.requests.load(Ordering::Relaxed), 0);
}
#[test]
fn executable_requires_pe_and_x64_headers() {
    let mut bytes = vec![0; 1024];
    assert!(updates::validate_executable(&bytes).is_err());
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&128u32.to_le_bytes());
    bytes[128..132].copy_from_slice(b"PE\0\0");
    bytes[132..134].copy_from_slice(&[0x64, 0x86]);
    updates::validate_executable(&bytes).unwrap();
    bytes[132] = 0x4c;
    assert!(updates::validate_executable(&bytes).is_err());
}
#[test]
fn verified_payload_is_staged_without_touching_installation() {
    let mut bytes = vec![0; 1024];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&128u32.to_le_bytes());
    bytes[128..132].copy_from_slice(b"PE\0\0");
    bytes[132..134].copy_from_slice(&[0x64, 0x86]);
    let mut r = release("0.2.0");
    r.assets[0].digest = Some(format!("sha256:{}", updates::sha256(&bytes)));
    let available = updates::choose(vec![r], &"0.1.0".parse().unwrap())
        .unwrap()
        .unwrap();
    let fake = Fake {
        requests: AtomicUsize::new(0),
        bytes: bytes.clone(),
    };
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("old.exe");
    std::fs::write(&target, b"old").unwrap();
    let manifest = updates::download(&fake, &available, &target, &mut |_| {}).unwrap();
    let m: updates::InstallManifest = room_planner::storage::read_json(&manifest, 16384).unwrap();
    assert_eq!(std::fs::read(&m.staged).unwrap(), bytes);
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert!(!m.backup.exists());
}

#[test]
fn disabling_startup_checks_makes_no_request_but_manual_check_still_works() {
    use room_planner::{settings::Settings, ui::Workspace};
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let transport = Arc::new(Fake {
        requests: AtomicUsize::new(0),
        bytes: b"[]".to_vec(),
    });
    let (sender, receiver) = mpsc::channel();
    let mut workspace = Workspace::with_settings(
        Settings {
            startup_updates: false,
            ..Default::default()
        },
        transport.clone(),
        Arc::new(move || {
            let _ = sender.send(());
        }),
    );
    assert!(matches!(workspace.update_state(), updates::State::Idle));
    assert_eq!(transport.requests.load(Ordering::Relaxed), 0);
    workspace.check_updates();
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    workspace.poll_jobs();
    assert_eq!(transport.requests.load(Ordering::Relaxed), 1);
    assert!(matches!(workspace.update_state(), updates::State::Current));
}
#[test]
fn default_startup_check_runs_as_a_background_job_with_fake_transport() {
    use room_planner::{settings::Settings, ui::Workspace};
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let transport = Arc::new(Fake {
        requests: AtomicUsize::new(0),
        bytes: b"[]".to_vec(),
    });
    let (sender, receiver) = mpsc::channel();
    let mut workspace = Workspace::with_settings(
        Settings::default(),
        transport.clone(),
        Arc::new(move || {
            let _ = sender.send(());
        }),
    );
    receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    workspace.poll_jobs();
    assert_eq!(transport.requests.load(Ordering::Relaxed), 1);
    assert!(matches!(workspace.update_state(), updates::State::Current));
}
