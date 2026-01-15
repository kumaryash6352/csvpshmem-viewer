use anyhow::Result;
use egui::ahash::HashMap;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct RawEvent {
    #[serde(rename = "Time")]
    pub time: f64,
    #[serde(rename = "Function")]
    pub function: String,
    #[serde(rename = "Duration_Sec")]
    pub duration_sec: f64,
    #[serde(rename = "Target_PE")]
    pub target_pe: i32,
    #[serde(rename = "Bytes_RX")]
    pub bytes_rx: u64,
    #[serde(rename = "Bytes_TX")]
    pub bytes_tx: u64,
    #[serde(rename = "Stacktrace")]
    pub stacktrace: String,
    #[serde(rename = "Extra", default)]
    pub extra: Option<String>,
    #[serde(rename = "Symboltrace", default)]
    pub symboltrace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub source_pe: u32,
    pub raw: RawEvent,
}

#[derive(Debug, Default)]
pub struct ProfileData {
    pub events: Vec<Event>,
    pub pe_count: u32,
    pub pe_hostnames: HashMap<u32, String>,
    pub min_time: f64,
    pub max_time: f64,
}

impl ProfileData {
    pub fn load_from_dir(dir: &Path) -> Result<Self> {
        let mut events = Vec::new();
        let mut max_pe = 0;
        let mut pe_hostnames = HashMap::default();

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("pperf.") && name.ends_with(".csv") {
                    // split pperf.XXX.csv
                    let parts: Vec<&str> = name.split('.').collect();
                    if parts.len() == 3 {
                        if let Ok(pe_id) = parts[1].parse::<u32>() {
                            if pe_id > max_pe {
                                max_pe = pe_id;
                            }
                            let loaded_events = Self::load_file(&path, pe_id)?;
                            // first event is the initialize (hopefully)
                            let initialize = loaded_events.first().expect("at least one event");
                            pe_hostnames.insert(
                                pe_id,
                                initialize
                                    .raw
                                    .extra
                                    .clone()
                                    .expect("hostname to be Extra of first event"),
                            );
                            events.extend(loaded_events);
                        }
                    }
                }
            }
        }

        // probably would be faster to use some sort of
        // merging algorithm but \shrug
        events.sort_by(|a, b| {
            a.raw
                .time
                .partial_cmp(&b.raw.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let min_time = events.first().map(|e| e.raw.time).unwrap_or(0.0);
        let max_time = events
            .iter()
            .map(|e| e.raw.time + e.raw.duration_sec)
            .fold(0.0, f64::max);

        Ok(Self {
            events,
            pe_count: max_pe + 1,
            pe_hostnames,
            min_time,
            max_time,
        })
    }

    fn load_file(path: &PathBuf, source_pe: u32) -> Result<Vec<Event>> {
        let mut rdr = csv::ReaderBuilder::new()
            .trim(csv::Trim::All)
            .from_path(path)?;

        let mut events = Vec::new();
        for result in rdr.deserialize() {
            let raw: RawEvent = result?;
            events.push(Event { source_pe, raw });
        }
        Ok(events)
    }
}
