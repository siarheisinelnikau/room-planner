use crate::{geometry, model::Document};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_PROJECT: u64 = 4 * 1024 * 1024;
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub fn backup_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".bak");
    p.into()
}

pub fn read_json<T: DeserializeOwned>(path: &Path, limit: u64) -> Result<T, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = vec![];
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("File exceeds size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub fn read_document(path: &Path) -> Result<Document, String> {
    let d: Document = read_json(path, MAX_PROJECT)?;
    geometry::validate(&d).map_err(|e| e.to_string())?;
    Ok(d)
}
pub struct LoadedDocument {
    pub document: Document,
    pub recovered: bool,
}
pub fn load_document(path: &Path) -> Result<LoadedDocument, String> {
    match read_document(path) {
        Ok(document) => Ok(LoadedDocument {
            document,
            recovered: false,
        }),
        Err(primary) => match read_document(&backup_path(path)) {
            Ok(document) => Ok(LoadedDocument {
                document,
                recovered: true,
            }),
            Err(_) => Err(primary),
        },
    }
}
pub fn save_document(path: &Path, d: &Document) -> Result<(), String> {
    geometry::validate(d).map_err(|e| e.to_string())?;
    // Never replace a good recovery copy with a corrupt primary file.
    if path.exists() && read_document(path).is_ok() {
        atomic_write(
            &backup_path(path),
            &fs::read(path).map_err(|e| e.to_string())?,
        )?;
    }
    save_json(path, d)
}
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    atomic_write(path, &bytes)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or("Missing file name")?
        .to_string_lossy();
    let temp = parent.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> io::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        replace_file(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())
}
#[cfg(windows)]
pub fn replace_file(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let s: Vec<_> = source.as_os_str().encode_wide().chain([0]).collect();
    let t: Vec<_> = target.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: Both paths are owned, null-terminated UTF-16 for the duration of the call.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            s.as_ptr(),
            t.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(not(windows))]
pub fn replace_file(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}
