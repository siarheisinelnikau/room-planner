use crate::storage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub const REPOSITORY: &str = "siarheisinelnikau/room-planner";
pub const RELEASE_API: &str =
    "https://api.github.com/repos/siarheisinelnikau/room-planner/releases?per_page=100";
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    #[serde(default)]
    pub digest: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug)]
pub struct Available {
    pub version: semver::Version,
    pub asset: Asset,
    pub checksum: Option<Asset>,
}
#[derive(Clone, Debug)]
pub enum State {
    Idle,
    Checking,
    Current,
    Available(Available),
    Downloading { received: u64, total: u64 },
    Ready(PathBuf),
    Failed { key: &'static str, detail: String },
}

pub trait Transport: Send + Sync {
    fn bytes(
        &self,
        url: &str,
        limit: u64,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Vec<u8>, String>;
}
pub struct Https;
impl Transport for Https {
    fn bytes(
        &self,
        url: &str,
        limit: u64,
        progress: &mut dyn FnMut(u64),
    ) -> Result<Vec<u8>, String> {
        if !url.starts_with("https://") {
            return Err("HTTPS is required".into());
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(120)))
            .timeout_connect(Some(Duration::from_secs(15)))
            .build()
            .into();
        let mut response = agent
            .get(url)
            .header(
                "User-Agent",
                concat!("RoomPlanner/", env!("CARGO_PKG_VERSION")),
            )
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| format!("GitHub request failed: {e}"))?;
        let mut reader = response.body_mut().as_reader();
        let mut bytes = Vec::new();
        let mut chunk = [0; 64 * 1024];
        let mut last = 0;
        loop {
            let n = reader.read(&mut chunk).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if bytes.len() as u64 + n as u64 > limit {
                return Err("Download exceeds size limit".into());
            }
            bytes.extend_from_slice(&chunk[..n]);
            if bytes.len() as u64 >= last + 1024 * 1024 {
                last = bytes.len() as u64;
                progress(last);
            }
        }
        progress(bytes.len() as u64);
        Ok(bytes)
    }
}
fn official(asset: &Asset, version: &semver::Version) -> bool {
    let prefix = format!("https://github.com/{REPOSITORY}/releases/download/");
    let Some(rest) = asset.browser_download_url.strip_prefix(&prefix) else {
        return false;
    };
    let mut parts = rest.split('/');
    let tag = parts.next().unwrap_or_default();
    let file = parts.next().unwrap_or_default();
    semver::Version::parse(tag.trim_start_matches('v'))
        .ok()
        .as_ref()
        == Some(version)
        && file == asset.name
        && parts.next().is_none()
}
pub fn choose(
    releases: Vec<Release>,
    installed: &semver::Version,
) -> Result<Option<Available>, String> {
    let mut result = None;
    for release in releases {
        if release.draft || release.prerelease {
            continue;
        }
        let Ok(version) = semver::Version::parse(release.tag_name.trim_start_matches('v')) else {
            continue;
        };
        if !version.pre.is_empty() || version <= *installed {
            continue;
        }
        let name = format!("RoomPlanner-v{version}.exe");
        // This distribution currently offers one Windows x64 executable only.
        let Some(asset) = release
            .assets
            .iter()
            .find(|a| {
                a.name == name && a.size > 0 && a.size <= MAX_DOWNLOAD && official(a, &version)
            })
            .cloned()
        else {
            continue;
        };
        let checksum = release
            .assets
            .iter()
            .find(|a| a.name == format!("{name}.sha256") && a.size <= 4096 && official(a, &version))
            .cloned();
        if result
            .as_ref()
            .is_none_or(|a: &Available| version > a.version)
        {
            result = Some(Available {
                version,
                asset,
                checksum,
            });
        }
    }
    Ok(result)
}
pub fn check(transport: &dyn Transport) -> Result<Option<Available>, String> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("Automatic installation currently supports Windows x64 only".into());
    }
    let bytes = transport.bytes(RELEASE_API, 4 * 1024 * 1024, &mut |_| {})?;
    let releases: Vec<Release> = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    choose(
        releases,
        &semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap(),
    )
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn expected_hash(available: &Available, transport: &dyn Transport) -> Result<String, String> {
    let hash = if let Some(digest) = available
        .asset
        .digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
    {
        digest.to_owned()
    } else {
        let checksum = available
            .checksum
            .as_ref()
            .ok_or("The release has no SHA-256 integrity information")?;
        if !official(checksum, &available.version) {
            return Err("Invalid official checksum URL".into());
        }
        let bytes = transport.bytes(&checksum.browser_download_url, 4096, &mut |_| {})?;
        let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let fields: Vec<_> = text.split_whitespace().collect();
        if fields.len() > 2
            || fields.is_empty()
            || fields
                .get(1)
                .is_some_and(|name| name.trim_start_matches('*') != available.asset.name)
        {
            return Err("Invalid checksum file".into());
        }
        fields[0].to_owned()
    };
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid SHA-256 digest".into());
    }
    Ok(hash.to_lowercase())
}
pub fn validate_executable(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() < 256 || bytes.get(..2) != Some(b"MZ") {
        return Err("Update is not a Windows executable".into());
    }
    let offset = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    if bytes.get(offset..offset + 4) != Some(b"PE\0\0")
        || bytes.get(offset + 4..offset + 6) != Some(&[0x64, 0x86])
    {
        return Err("Update is not a Windows x64 executable".into());
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallManifest {
    pub target: PathBuf,
    pub staged: PathBuf,
    pub backup: PathBuf,
    pub hash: String,
    pub parent_pid: u32,
    #[serde(default)]
    pub reopen: Option<PathBuf>,
}
pub fn download(
    transport: &dyn Transport,
    available: &Available,
    target: &Path,
    progress: &mut dyn FnMut(u64),
) -> Result<PathBuf, String> {
    if !official(&available.asset, &available.version) {
        return Err("Invalid official release URL".into());
    }
    let hash = expected_hash(available, transport)?;
    let bytes = transport.bytes(
        &available.asset.browser_download_url,
        MAX_DOWNLOAD,
        progress,
    )?;
    if bytes.len() as u64 != available.asset.size || sha256(&bytes) != hash {
        return Err("Size or SHA-256 verification failed".into());
    }
    validate_executable(&bytes)?;
    let target = target.canonicalize().map_err(|e| e.to_string())?;
    let parent = target.parent().ok_or("Invalid installation path")?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let directory = parent.join(format!(
        ".roomplanner-update-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).map_err(|e| e.to_string())?;
    let staged = directory.join("verified.exe");
    storage::atomic_write(&staged, &bytes)?;
    let helper = directory.join("helper.exe");
    fs::copy(&target, &helper).map_err(|e| e.to_string())?;
    let manifest = InstallManifest {
        backup: directory.join("previous.exe"),
        target,
        staged,
        hash,
        parent_pid: std::process::id(),
        reopen: None,
    };
    let path = directory.join("install.json");
    storage::save_json(&path, &manifest)?;
    Ok(path)
}
pub fn launch_helper(manifest: &Path, reopen: Option<PathBuf>) -> Result<(), String> {
    let mut plan: InstallManifest = storage::read_json(manifest, 16 * 1024)?;
    plan.reopen = reopen;
    storage::save_json(manifest, &plan)?;
    let directory = manifest.parent().ok_or("Invalid update directory")?;
    std::process::Command::new(directory.join("helper.exe"))
        .arg("--apply-update")
        .arg(manifest)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn validate_manifest(path: &Path, m: &InstallManifest) -> Result<(), String> {
    let helper = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let directory = path
        .parent()
        .ok_or("Invalid update manifest")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if helper.parent() != Some(directory.as_path())
        || helper.file_name().is_none_or(|n| n != "helper.exe")
        || directory.parent() != m.target.parent()
        || !directory
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with(".roomplanner-update-")
        || m.staged != directory.join("verified.exe")
        || m.backup != directory.join("previous.exe")
        || !m.target.is_absolute()
        || m.parent_pid == 0
    {
        return Err("Unsafe update manifest paths".into());
    }
    let bytes = fs::read(&m.staged).map_err(|e| e.to_string())?;
    if sha256(&bytes) != m.hash {
        return Err("Staged update changed".into());
    }
    validate_executable(&bytes)
}
#[cfg(windows)]
pub fn apply_update(path: &Path) -> Result<(), String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    let m: InstallManifest = storage::read_json(path, 16 * 1024)?;
    validate_manifest(path, &m)?;
    // SAFETY: The handle is owned here and closed on all paths. No process is terminated.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, m.parent_pid);
        if !process.is_null() {
            let status = WaitForSingleObject(process, 60_000);
            CloseHandle(process);
            if status != WAIT_OBJECT_0 {
                return Err("The app did not exit; installation was cancelled".into());
            }
        }
    }
    storage::replace_file(&m.target, &m.backup).map_err(|e| e.to_string())?;
    if let Err(error) = storage::replace_file(&m.staged, &m.target) {
        if let Err(recovery) = storage::replace_file(&m.backup, &m.target) {
            return Err(format!(
                "{error}; restoration failed: {recovery}. The previous executable is preserved at {}",
                m.backup.display()
            ));
        }
        return Err(error.to_string());
    }
    let marker = path.with_extension("started");
    let mut command = std::process::Command::new(&m.target);
    command.arg("--update-ready").arg(&marker);
    if let Some(project) = &m.reopen {
        command.arg("--reopen").arg(project);
    }
    match command.spawn() {
        Ok(mut child) => {
            // A successful first presentation acknowledges startup. Keep the old binary for recovery.
            for _ in 0..100 {
                if marker.exists() {
                    return Ok(());
                }
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    storage::replace_file(&m.backup, &m.target).map_err(|e| e.to_string())?;
                    let mut old = std::process::Command::new(&m.target);
                    if let Some(project) = &m.reopen {
                        old.arg("--reopen").arg(project);
                    }
                    let _ = old.spawn();
                    return Err(
                        "The update exited before startup; the previous version was restored"
                            .into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err("The update has not acknowledged startup; the previous executable remains in the update folder".into())
        }
        Err(e) => {
            storage::replace_file(&m.backup, &m.target).map_err(|r| r.to_string())?;
            let mut old = std::process::Command::new(&m.target);
            if let Some(project) = &m.reopen {
                old.arg("--reopen").arg(project);
            }
            let _ = old.spawn();
            Err(e.to_string())
        }
    }
}
#[cfg(not(windows))]
pub fn apply_update(_path: &Path) -> Result<(), String> {
    Err("Portable installation is available on Windows only".into())
}
