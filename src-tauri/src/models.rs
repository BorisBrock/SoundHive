//! Types shared with the frontend.

use serde::{Deserialize, Serialize};

/// A folder whose sounds are in the library.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub sound_count: i64,
    /// Unix seconds of the last finished scan.
    pub last_scan: Option<i64>,
    /// False if the folder could not be found during the last scan (e.g. an unplugged drive).
    pub available: bool,
}

/// One sound file with what the scan found out about it.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: i64,
    pub folder_id: i64,
    pub path: String,
    /// Folder of the file relative to its library folder, `/`-separated; empty at the top.
    pub dir: String,
    /// File name without extension.
    pub name: String,
    /// Lower-case extension without dot.
    pub ext: String,
    pub size: i64,
    /// Unix seconds.
    pub modified: i64,
    /// Seconds; 0 if unknown.
    pub duration: f64,
    pub sample_rate: i64,
    pub channels: i64,
    /// Bits per sample of PCM formats; 0 for compressed ones.
    pub bits: i64,
    /// Peak level in dBFS.
    pub peak_db: f64,
    /// Average (RMS) level in dBFS.
    pub rms_db: f64,
    /// Zero crossings per second: a rough measure of brightness / noisiness.
    pub zcr: f64,
    /// Seconds from the sound's start to (almost) its peak.
    pub attack: f64,
    /// Why the sound counts as a loop: `marked` (loop points in the file), `name` or `seamless`
    /// (its end flows into its start); null for one-shots.
    pub loop_kind: Option<String>,
    /// Title / description / comment from the file's metadata.
    pub description: String,
    pub favorite: bool,
    pub play_count: i64,
    /// Unix seconds.
    pub last_played: Option<i64>,
    /// Unix seconds when the scan first found the file.
    pub added: i64,
    /// Tags from the scan (minus those the user removed).
    pub auto_tags: Vec<String>,
    /// Tags the user added.
    pub user_tags: Vec<String>,
    /// Why the file could not be analysed, if it could not.
    pub error: Option<String>,
}

/// Peak envelope of a sound, `WAVEFORM_BUCKETS` values in 0..=255.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    pub id: i64,
    pub peaks: Vec<u8>,
}

/// A keyword rule for automatic tagging: files whose name or folder contains one of the
/// keywords get the tag.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagRule {
    pub tag: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    /// Program (and arguments) used for "Open in editor", e.g. Audacity. Empty: look for Audacity.
    pub editor: String,
    /// Play a sound as soon as it is selected.
    pub autoplay: bool,
    /// 0..=1
    pub volume: f64,
    /// Rescan all folders (quickly, only changed files are analysed) when the app starts.
    pub scan_on_start: bool,
    /// Tag sounds with the names of the folders they are in.
    pub folder_tags: bool,
    pub tag_rules: Vec<TagRule>,
    /// Folder that "Copy to project" copies files into.
    pub export_folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "midnight".into(),
            editor: String::new(),
            autoplay: true,
            volume: 0.8,
            scan_on_start: true,
            folder_tags: false,
            tag_rules: crate::tagging::default_rules(),
            export_folder: String::new(),
        }
    }
}

/// Progress of a library scan, pushed to the frontend a few times per second.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// `listing`, `analyzing`, `done` or `cancelled`.
    pub phase: String,
    pub done: u32,
    pub total: u32,
    /// File being worked on, for display.
    pub current: String,
    /// Summary when finished, e.g. "12 new, 3 changed, 1 removed".
    pub summary: String,
}

/// What the player is doing, polled by the frontend while a sound plays.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    pub sound_id: Option<i64>,
    pub playing: bool,
    /// Seconds into the sound (wraps around for loops).
    pub position: f64,
    pub looping: bool,
}

/// A project's sound folder (of a game, a video, …) whose files ("slots") get replaced with library sounds.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub slot_count: i64,
    pub approved_count: i64,
    /// False if the folder is not there (e.g. an unplugged drive).
    pub available: bool,
}

/// Where a slot is on its way from makeshift sound to the real one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    /// Still holds its original file.
    Placeholder,
    /// Holds a library sound that is being tried out in the project.
    Trying,
    /// Done: the sound fits.
    Approved,
}

impl SlotStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SlotStatus::Placeholder => "placeholder",
            SlotStatus::Trying => "trying",
            SlotStatus::Approved => "approved",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "trying" => SlotStatus::Trying,
            "approved" => SlotStatus::Approved,
            _ => SlotStatus::Placeholder,
        }
    }
}

/// One sound file of a project.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    pub id: i64,
    pub project_id: i64,
    /// Path relative to the project folder, `/`-separated.
    pub rel_path: String,
    /// File name without extension.
    pub name: String,
    /// Folder relative to the project folder, `/`-separated; empty at the top.
    pub dir: String,
    /// Lower-case extension without dot.
    pub ext: String,
    /// Seconds; 0 if unknown.
    pub duration: f64,
    pub sample_rate: i64,
    pub channels: i64,
    /// Peak envelope of the file as it is now, `WAVEFORM_BUCKETS` values in 0..=255; empty if
    /// it could not be analysed.
    pub peaks: Vec<u8>,
    pub error: Option<String>,
    pub status: SlotStatus,
    /// The library file SoundHive last put in; null while the slot holds its original.
    pub source_path: Option<String>,
    /// Whether the original is backed up, i.e. can be restored.
    pub backed_up: bool,
    /// Library files tried in this slot, the latest first.
    pub tries: Vec<SlotTry>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SlotTry {
    pub source_path: String,
    /// Unix milliseconds.
    pub tried: i64,
}
