use crate::storage;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};

static ERROR_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn initialize_error_reporting(path: PathBuf) {
    let _ = ERROR_PATH.set(path);
    std::panic::set_hook(Box::new(|panic| {
        let report = format!(
            "RoomPlanner {}\nProcess: {}\n{panic}\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::process::id(),
            std::backtrace::Backtrace::force_capture()
        );
        let _ = write_failure(&report);
        eprintln!("{report}");
    }));
}
pub fn write_failure(message: &str) -> Result<PathBuf, String> {
    let path = ERROR_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| crate::settings::Settings::path().with_file_name("startup-error.log"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    storage::atomic_write(&path, message.as_bytes())?;
    Ok(path)
}
pub fn default_error_path() -> PathBuf {
    crate::settings::Settings::path().with_file_name("startup-error.log")
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Counters {
    pub layouts: u64,
    pub rasters: u64,
    pub presents: u64,
    pub raster_micros: u64,
    pub present_micros: u64,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub state: &'static str,
    pub elapsed_ms: u64,
    pub counters: Counters,
}
pub struct Session {
    pub directory: PathBuf,
    pub started: Instant,
    pub phase: usize,
    pub snapshots: Vec<Snapshot>,
}
impl Session {
    pub const DEADLINES: [u64; 5] = [2, 7, 12, 14, 19];
    pub fn new(directory: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            started: Instant::now(),
            phase: 0,
            snapshots: vec![],
        })
    }
    pub fn deadline(&self) -> Option<Instant> {
        Self::DEADLINES
            .get(self.phase)
            .map(|&s| self.started + Duration::from_secs(s))
    }
    pub fn due(&self) -> bool {
        self.deadline().is_some_and(|time| time <= Instant::now())
    }
    pub fn snapshot(&mut self, state: &'static str, counters: &Counters) {
        self.snapshots.push(Snapshot {
            state,
            elapsed_ms: self.started.elapsed().as_millis() as u64,
            counters: counters.clone(),
        });
    }
    pub fn save(&self) -> Result<(), String> {
        #[derive(Serialize)]
        struct Report<'a> {
            version: &'static str,
            pid: u32,
            backend: &'static str,
            snapshots: &'a [Snapshot],
        }
        storage::save_json(
            &self.directory.join("runtime.json"),
            &Report {
                version: env!("CARGO_PKG_VERSION"),
                pid: std::process::id(),
                backend: "CPU rasterization / softbuffer GDI",
                snapshots: &self.snapshots,
            },
        )
    }
}

pub fn diagnostic_directory(args: &[std::ffi::OsString]) -> Result<Option<PathBuf>, String> {
    if let Some(i) = args.iter().position(|arg| arg == "--diagnose") {
        let path = Path::new(
            args.get(i + 1)
                .ok_or("Missing diagnostic output directory")?,
        );
        if !path.is_absolute() {
            return Err("The diagnostic output directory must be absolute".into());
        }
        return Ok(Some(path.to_owned()));
    }
    Ok(None)
}
