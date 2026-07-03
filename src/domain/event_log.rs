use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
use time::{format_description::well_known::Iso8601, OffsetDateTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppEventKind {
    #[serde(rename = "STATE_CHANGE")]
    StateChange,
    #[serde(rename = "PROMPT_FIRED")]
    PromptFired,
    #[serde(rename = "CAMERA_UNAVAILABLE")]
    CameraUnavailable,
    #[serde(rename = "CAMERA_RESUMED")]
    CameraResumed,
    #[serde(rename = "SNOOZE_START")]
    SnoozeStart,
    #[serde(rename = "SNOOZE_END")]
    SnoozeEnd,
    #[serde(rename = "WARNING_LEVEL_CHANGED")]
    WarningLevelChanged,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AppEvent {
    pub ts: String,
    pub kind: AppEventKind,
    #[serde(flatten)]
    pub data: Map<String, Value>,
}

#[derive(Debug)]
pub struct EventLog {
    config_dir: PathBuf,
    lock: Mutex<()>,
}

impl EventLog {
    pub fn new(config_dir: impl AsRef<Path>) -> Self {
        Self {
            config_dir: config_dir.as_ref().to_path_buf(),
            lock: Mutex::new(()),
        }
    }

    pub fn append(&self, kind: AppEventKind, payload: Value) -> io::Result<()> {
        let _guard = self.lock.lock().expect("event log mutex poisoned");
        fs::create_dir_all(&self.config_dir)?;
        let mut event = match payload {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        event.insert("ts".to_string(), Value::String(timestamp_iso()));
        event.insert(
            "kind".to_string(),
            serde_json::to_value(kind).expect("event kind serializes"),
        );
        let line = serde_json::to_string(&event)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.events_path())?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    pub fn events(&self) -> io::Result<Vec<AppEvent>> {
        let path = self.events_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(path)?;
        let events = content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }
                serde_json::from_str::<AppEvent>(trimmed).ok()
            })
            .collect();
        Ok(events)
    }

    fn events_path(&self) -> PathBuf {
        self.config_dir.join("events.jsonl")
    }
}

fn timestamp_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&Iso8601::DEFAULT)
        .expect("UTC timestamp formatting should be infallible")
}
