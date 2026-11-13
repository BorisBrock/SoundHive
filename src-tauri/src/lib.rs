mod analysis;
mod commands;
mod convert;
mod db;
mod error;
mod events;
mod models;
mod player;
mod project;
mod scan;
mod system;
mod tagging;

use std::sync::Mutex;

use tauri::Manager;
use tauri_specta::{Builder, ErrorHandlingMode, collect_commands, collect_events};

const BINDINGS_PATH: &str = "../src/lib/bindings.ts";
const DB_FILE: &str = "soundhive.db";

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::get_settings,
            commands::save_settings,
            commands::default_tag_rules,
            commands::editor_name,
            commands::list_folders,
            commands::add_folder,
            commands::remove_folder,
            commands::rescan,
            commands::cancel_scan,
            commands::is_scanning,
            commands::list_sounds,
            commands::get_sound,
            commands::get_waveforms,
            commands::set_favorite,
            commands::add_tag,
            commands::remove_tag,
            commands::rename_tag,
            commands::play,
            commands::stop,
            commands::toggle_pause,
            commands::seek,
            commands::set_volume,
            commands::playback_state,
            commands::copy_files,
            commands::open_in_editor,
            commands::open_default,
            commands::reveal_sound,
            commands::open_folder,
            commands::copy_to_folder,
            commands::start_drag,
            commands::list_projects,
            commands::add_project,
            commands::remove_project,
            commands::refresh_project,
            commands::writable_formats,
            commands::replace_slot,
            commands::restore_slot,
            commands::set_slot_approved,
            commands::play_slot,
            commands::reveal_slot,
        ])
        .events(collect_events![events::ScanProgressed, events::LibraryChanged])
        .constant("WAVEFORM_BUCKETS", analysis::WAVEFORM_BUCKETS)
        .error_handling(ErrorHandlingMode::Throw)
        // IDs are SQLite rowids, far below 2^53.
        .dangerously_cast_bigints_to_number()
}

#[cfg_attr(not(debug_assertions), allow(dead_code))]
fn export_bindings(builder: &Builder<tauri::Wry>) {
    builder
        .export(specta_typescript::Typescript::default().header("// @ts-nocheck\n"), BINDINGS_PATH)
        .expect("failed to export TypeScript bindings");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();
    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // Remembers window size and position between runs.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join(DB_FILE);
            let conn = db::open(&db_path)?;
            let scan_on_start = db::get_settings(&conn).map(|s| s.scan_on_start).unwrap_or(true);
            let listener = std::sync::Arc::new(events::TauriListener(app.handle().clone()));
            let scanner = scan::Scanner::new(db_path, listener);
            if scan_on_start {
                scanner.start(scan::Scope::All);
            }
            app.manage(commands::Db(Mutex::new(conn)));
            app.manage(scanner);
            app.manage(player::Player::new());
            app.manage(system::FileClipboard::new());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    /// Regenerates `src/lib/bindings.ts`; CI fails if the committed file is stale.
    #[test]
    fn export_bindings() {
        super::export_bindings(&super::specta_builder());
    }
}
