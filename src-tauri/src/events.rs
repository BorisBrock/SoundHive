//! Events pushed to the frontend.

use serde::Serialize;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::models::ScanProgress;
use crate::scan::ScanListener;

#[derive(Debug, Clone, Serialize, specta::Type, Event)]
pub struct ScanProgressed(pub ScanProgress);

/// Sounds or folders changed; the frontend reloads them.
#[derive(Debug, Clone, Serialize, specta::Type, Event)]
pub struct LibraryChanged;

pub struct TauriListener(pub AppHandle);

impl ScanListener for TauriListener {
    fn progress(&self, progress: ScanProgress) {
        let _ = ScanProgressed(progress).emit(&self.0);
    }

    fn library_changed(&self) {
        let _ = LibraryChanged.emit(&self.0);
    }
}
