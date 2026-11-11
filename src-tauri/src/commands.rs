//! IPC commands exposed to the frontend.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{AppHandle, Manager, State};

use crate::db;
use crate::error::{AppResult, invalid};
use crate::models::*;
use crate::player::{PlayRequest, Player};
use crate::project;
use crate::scan::{self, Scanner, Scope};
use crate::system::{self, FileClipboard};

/// The UI's database connection; scans use their own.
pub struct Db(pub Mutex<Connection>);

type DbState<'a> = State<'a, Db>;

fn with_db<T>(db: &DbState<'_>, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
    let conn = db.0.lock().unwrap();
    f(&conn)
}

// ---- settings --------------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn get_settings(db: DbState<'_>) -> AppResult<Settings> {
    with_db(&db, db::get_settings)
}

/// Saves the settings; re-tags the library if the tag rules changed.
#[tauri::command]
#[specta::specta]
pub async fn save_settings(db: DbState<'_>, app: AppHandle, settings: Settings) -> AppResult<()> {
    let retag = with_db(&db, |conn| {
        let old = db::get_settings(conn)?;
        db::save_settings(conn, &settings)?;
        Ok(old.tag_rules != settings.tag_rules || old.folder_tags != settings.folder_tags)
    })?;
    if retag {
        with_db(&db, |conn| scan::retag(conn, &settings, None))?;
        use tauri_specta::Event;
        let _ = crate::events::LibraryChanged.emit(&app);
    }
    Ok(())
}

/// The built-in tag rules, for "Reset to defaults".
#[tauri::command]
#[specta::specta]
pub fn default_tag_rules() -> Vec<TagRule> {
    crate::tagging::default_rules()
}

/// Name of the program "Open in editor" starts, or null if none is set or found.
#[tauri::command]
#[specta::specta]
pub async fn editor_name(editor: String) -> Option<String> {
    system::editor_name(&editor)
}

// ---- folders and scanning --------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn list_folders(db: DbState<'_>) -> AppResult<Vec<Folder>> {
    with_db(&db, db::list_folders)
}

/// Adds a folder and scans it.
#[tauri::command]
#[specta::specta]
pub async fn add_folder(db: DbState<'_>, scanner: State<'_, Arc<Scanner>>, path: String) -> AppResult<()> {
    let id = with_db(&db, |conn| db::add_folder(conn, &path))?;
    scanner.start(Scope::Folders(HashSet::from([id])));
    Ok(())
}

/// Removes a folder and its sounds from the library (the files stay where they are).
#[tauri::command]
#[specta::specta]
pub async fn remove_folder(db: DbState<'_>, scanner: State<'_, Arc<Scanner>>, id: i64) -> AppResult<()> {
    if scanner.is_running() {
        return Err(invalid("Wait for the scan to finish, or cancel it, before removing a folder."));
    }
    with_db(&db, |conn| db::remove_folder(conn, id))
}

/// Scans the given folders, or all if none are given.
#[tauri::command]
#[specta::specta]
pub async fn rescan(scanner: State<'_, Arc<Scanner>>, folder_ids: Option<Vec<i64>>) -> AppResult<()> {
    scanner.start(match folder_ids {
        Some(ids) => Scope::Folders(ids.into_iter().collect()),
        None => Scope::All,
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_scan(scanner: State<'_, Arc<Scanner>>) -> AppResult<()> {
    scanner.cancel();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn is_scanning(scanner: State<'_, Arc<Scanner>>) -> AppResult<bool> {
    Ok(scanner.is_running())
}

// ---- sounds and tags -------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn list_sounds(db: DbState<'_>) -> AppResult<Vec<Sound>> {
    with_db(&db, db::list_sounds)
}

#[tauri::command]
#[specta::specta]
pub async fn get_sound(db: DbState<'_>, id: i64) -> AppResult<Sound> {
    with_db(&db, |conn| db::get_sound(conn, id))
}

#[tauri::command]
#[specta::specta]
pub async fn get_waveforms(db: DbState<'_>, ids: Vec<i64>) -> AppResult<Vec<Waveform>> {
    with_db(&db, |conn| db::waveforms(conn, &ids))
}

#[tauri::command]
#[specta::specta]
pub async fn set_favorite(db: DbState<'_>, ids: Vec<i64>, favorite: bool) -> AppResult<()> {
    with_db(&db, |conn| db::set_favorite(conn, &ids, favorite))
}

/// Adds a tag to sounds; returns it normalised (lower case, single spaces).
#[tauri::command]
#[specta::specta]
pub async fn add_tag(db: DbState<'_>, ids: Vec<i64>, tag: String) -> AppResult<String> {
    with_db(&db, |conn| db::add_tag(conn, &ids, &tag))
}

#[tauri::command]
#[specta::specta]
pub async fn remove_tag(db: DbState<'_>, ids: Vec<i64>, tag: String) -> AppResult<()> {
    with_db(&db, |conn| db::remove_tag(conn, &ids, &tag))
}

#[tauri::command]
#[specta::specta]
pub async fn rename_tag(db: DbState<'_>, from: String, to: String) -> AppResult<()> {
    with_db(&db, |conn| db::rename_tag(conn, &from, &to))
}

// ---- playback --------------------------------------------------------------------------------

/// Plays a sound from `start` seconds, stopping whatever plays.
#[tauri::command]
#[specta::specta]
pub async fn play(
    db: DbState<'_>,
    player: State<'_, Player>,
    id: i64,
    start: f64,
    looping: bool,
    volume: f64,
) -> AppResult<()> {
    let sound = with_db(&db, |conn| db::get_sound(conn, id))?;
    player.play(PlayRequest {
        sound_id: id,
        path: PathBuf::from(&sound.path),
        start,
        looping,
        volume: volume as f32,
        duration: sound.duration,
    })?;
    if start == 0.0 {
        with_db(&db, |conn| db::record_play(conn, id))?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn stop(player: State<'_, Player>) -> AppResult<()> {
    player.stop();
    Ok(())
}

/// Pauses or resumes; returns whether it plays now.
#[tauri::command]
#[specta::specta]
pub async fn toggle_pause(player: State<'_, Player>) -> AppResult<bool> {
    Ok(player.toggle_pause())
}

#[tauri::command]
#[specta::specta]
pub async fn seek(player: State<'_, Player>, position: f64) -> AppResult<()> {
    player.seek(position)
}

#[tauri::command]
#[specta::specta]
pub async fn set_volume(player: State<'_, Player>, volume: f64) -> AppResult<()> {
    player.set_volume(volume as f32);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn playback_state(player: State<'_, Player>) -> AppResult<PlaybackState> {
    Ok(player.state())
}

// ---- getting sounds out ----------------------------------------------------------------------

fn paths(db: &DbState<'_>, ids: &[i64]) -> AppResult<Vec<String>> {
    if ids.is_empty() {
        return Err(invalid("No sound selected."));
    }
    with_db(db, |conn| db::sound_paths(conn, ids))
}

/// Puts the sound files on the clipboard, to paste into a file manager, DAW or editor.
#[tauri::command]
#[specta::specta]
pub async fn copy_files(
    db: DbState<'_>,
    clipboard: State<'_, FileClipboard>,
    ids: Vec<i64>,
) -> AppResult<()> {
    clipboard.copy_files(&paths(&db, &ids)?)
}

/// Opens the sounds in the external editor (Audacity by default).
#[tauri::command]
#[specta::specta]
pub async fn open_in_editor(db: DbState<'_>, ids: Vec<i64>) -> AppResult<()> {
    let editor = with_db(&db, db::get_settings)?.editor;
    system::open_in_editor(&editor, &paths(&db, &ids)?)
}

/// Opens a sound in the system's default app for its type.
#[tauri::command]
#[specta::specta]
pub async fn open_default(db: DbState<'_>, id: i64) -> AppResult<()> {
    system::open_default(&paths(&db, &[id])?[0])
}

/// Shows a sound in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn reveal_sound(db: DbState<'_>, id: i64) -> AppResult<()> {
    system::reveal(&paths(&db, &[id])?[0])
}

#[tauri::command]
#[specta::specta]
pub async fn open_folder(path: String) -> AppResult<()> {
    system::open_folder(&path)
}

/// Copies the sounds into a folder (e.g. a game's assets); returns the new paths.
#[tauri::command]
#[specta::specta]
pub async fn copy_to_folder(db: DbState<'_>, ids: Vec<i64>, folder: String) -> AppResult<Vec<String>> {
    system::copy_to_folder(&paths(&db, &ids)?, &folder)
}

/// Drags the sound files out of the window, into other apps.
#[tauri::command]
#[specta::specta]
pub async fn start_drag(db: DbState<'_>, app: AppHandle, ids: Vec<i64>) -> AppResult<()> {
    let files: Vec<PathBuf> = paths(&db, &ids)?.into_iter().map(PathBuf::from).collect();
    let window = app.get_webview_window("main").ok_or_else(|| invalid("No window"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(system::start_drag(&window, files));
    })
    .map_err(|e| invalid(e.to_string()))?;
    rx.recv().map_err(|e| invalid(e.to_string()))?
}

// ---- target projects -------------------------------------------------------------------------

fn data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    app.path().app_data_dir().map_err(|e| invalid(e.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn list_projects(db: DbState<'_>) -> AppResult<Vec<Project>> {
    with_db(&db, project::list_projects)
}

/// Adds a sound folder (of a game, a video, …) as a project; returns its id.
#[tauri::command]
#[specta::specta]
pub async fn add_project(db: DbState<'_>, path: String) -> AppResult<i64> {
    let id = with_db(&db, |conn| project::add_project(conn, &path))?;
    project::refresh(&db.0, id)?;
    Ok(id)
}

/// Forgets a project and the backups of its originals; the project's files stay as they are.
#[tauri::command]
#[specta::specta]
pub async fn remove_project(db: DbState<'_>, app: AppHandle, id: i64) -> AppResult<()> {
    let dir = data_dir(&app)?;
    with_db(&db, |conn| project::remove_project(conn, &dir, id))
}

/// Picks up added, changed and removed files of a project; returns its slots.
#[tauri::command]
#[specta::specta]
pub async fn refresh_project(db: DbState<'_>, id: i64) -> AppResult<Vec<Slot>> {
    project::refresh(&db.0, id)
}

/// Puts a library sound into a slot (the original is backed up first).
#[tauri::command]
#[specta::specta]
pub async fn replace_slot(db: DbState<'_>, app: AppHandle, slot_id: i64, sound_id: i64) -> AppResult<Slot> {
    let dir = data_dir(&app)?;
    with_db(&db, |conn| project::replace(conn, &dir, slot_id, sound_id))
}

/// Puts a slot's original file back.
#[tauri::command]
#[specta::specta]
pub async fn restore_slot(db: DbState<'_>, app: AppHandle, slot_id: i64) -> AppResult<Slot> {
    let dir = data_dir(&app)?;
    with_db(&db, |conn| project::restore(conn, &dir, slot_id))
}

/// Marks a slot as done, or opens it again.
#[tauri::command]
#[specta::specta]
pub async fn set_slot_approved(db: DbState<'_>, slot_id: i64, approved: bool) -> AppResult<Slot> {
    with_db(&db, |conn| project::set_approved(conn, slot_id, approved))
}

/// Plays a slot's file as it is now. The player reports it as sound `-slot_id`.
#[tauri::command]
#[specta::specta]
pub async fn play_slot(
    db: DbState<'_>,
    player: State<'_, Player>,
    slot_id: i64,
    start: f64,
    looping: bool,
    volume: f64,
) -> AppResult<()> {
    let (path, duration) = with_db(&db, |conn| {
        Ok((project::slot_file(conn, slot_id)?, project::get_slot(conn, slot_id)?.duration))
    })?;
    player.play(PlayRequest { sound_id: -slot_id, path, start, looping, volume: volume as f32, duration })
}

/// Shows a slot's file in the file manager.
#[tauri::command]
#[specta::specta]
pub async fn reveal_slot(db: DbState<'_>, slot_id: i64) -> AppResult<()> {
    let path = with_db(&db, |conn| project::slot_file(conn, slot_id))?;
    system::reveal(&path.to_string_lossy())
}
