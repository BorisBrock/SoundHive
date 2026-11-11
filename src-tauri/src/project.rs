//! Target projects: the folder of makeshift sound files ("slots") of a game, a video or any other
//! project, which get replaced one by one with sounds from the library, tried out in the
//! project, and approved.
//!
//! The library is only ever read. Only the project's files are written, always by writing a
//! temporary file next to the target and renaming it over the target, so that a game engine or editor
//! watching the folder never sees a half-written file. Before a slot is replaced the first time,
//! its original is backed up in the app's data folder (not in the project's folder, where an engine
//! would import it), so that it can always be restored.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use rayon::prelude::*;
use rusqlite::{Connection, OptionalExtension, params};

use crate::analysis::{self, Analysis};
use crate::db::{self, now};
use crate::error::{AppError, AppResult, invalid};
use crate::models::{Project, Slot, SlotStatus, SlotTry};

/// Longer sounds are not converted (they would not fit in memory comfortably).
const MAX_CONVERT_SECONDS: u64 = 10 * 60;

fn io_error(e: impl std::fmt::Display) -> AppError {
    AppError::Io { message: e.to_string() }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Where a project's originals are backed up.
pub fn backup_dir(data_dir: &Path, project_id: i64) -> PathBuf {
    data_dir.join("projects").join(project_id.to_string()).join("originals")
}

// ---- projects --------------------------------------------------------------------------------

pub fn list_projects(conn: &Connection) -> AppResult<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.path,
            (SELECT COUNT(*) FROM slots s WHERE s.project_id = p.id),
            (SELECT COUNT(*) FROM slots s WHERE s.project_id = p.id AND s.status = 'approved')
         FROM projects p ORDER BY p.path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let path: String = r.get(1)?;
        Ok(Project {
            id: r.get(0)?,
            name: db::folder_name(&path),
            available: Path::new(&path).is_dir(),
            path,
            slot_count: r.get(2)?,
            approved_count: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn add_project(conn: &Connection, path: &str) -> AppResult<i64> {
    let path = path.trim_end_matches(['/', '\\']);
    let path = if path.is_empty() { "/" } else { path };
    if !Path::new(path).is_dir() {
        return Err(invalid(format!("Folder not found: {path}")));
    }
    let exists: Option<i64> =
        conn.query_row("SELECT id FROM projects WHERE path = ?1", [path], |r| r.get(0)).optional()?;
    if exists.is_some() {
        return Err(invalid("This folder is already a project."));
    }
    conn.execute("INSERT INTO projects (path, added) VALUES (?1, ?2)", params![path, now()])?;
    Ok(conn.last_insert_rowid())
}

/// Forgets a project and its backups; the project's files stay as they are.
pub fn remove_project(conn: &Connection, data_dir: &Path, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    let dir = backup_dir(data_dir, id);
    if let Some(project_dir) = dir.parent() {
        match std::fs::remove_dir_all(project_dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io_error(e)),
            _ => {}
        }
    }
    Ok(())
}

fn project_path(conn: &Connection, id: i64) -> AppResult<String> {
    Ok(conn.query_row("SELECT path FROM projects WHERE id = ?1", [id], |r| r.get(0))?)
}

// ---- slots -----------------------------------------------------------------------------------

const SLOT_COLUMNS: &str = "id, project_id, rel_path, duration, sample_rate, channels, waveform, error, status, source_path, backed_up";

fn slot_from_row(r: &rusqlite::Row) -> rusqlite::Result<Slot> {
    let rel_path: String = r.get(2)?;
    let (dir, file) = rel_path.rsplit_once('/').unwrap_or(("", &rel_path));
    let (name, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    let (dir, name, ext) = (dir.to_string(), name.to_string(), ext.to_ascii_lowercase());
    let status: String = r.get(8)?;
    Ok(Slot {
        id: r.get(0)?,
        project_id: r.get(1)?,
        rel_path,
        name,
        dir,
        ext,
        duration: r.get(3)?,
        sample_rate: r.get(4)?,
        channels: r.get(5)?,
        peaks: r.get::<_, Option<Vec<u8>>>(6)?.unwrap_or_default(),
        error: r.get(7)?,
        status: SlotStatus::parse(&status),
        source_path: r.get(9)?,
        backed_up: r.get(10)?,
        tries: Vec::new(),
    })
}

/// A project's slots in path order, with what was tried in them.
pub fn list_slots(conn: &Connection, project_id: i64) -> AppResult<Vec<Slot>> {
    let mut slots: Vec<Slot> = conn
        .prepare(&format!(
            "SELECT {SLOT_COLUMNS} FROM slots WHERE project_id = ?1 ORDER BY rel_path COLLATE NOCASE"
        ))?
        .query_map([project_id], slot_from_row)?
        .collect::<Result<_, _>>()?;
    let index: HashMap<i64, usize> = slots.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut stmt = conn.prepare(
        "SELECT t.slot_id, t.source_path, t.tried FROM slot_tries t JOIN slots s ON s.id = t.slot_id
         WHERE s.project_id = ?1 ORDER BY t.tried DESC",
    )?;
    let mut rows = stmt.query([project_id])?;
    while let Some(r) = rows.next()? {
        if let Some(&i) = index.get(&r.get::<_, i64>(0)?) {
            slots[i].tries.push(SlotTry { source_path: r.get(1)?, tried: r.get(2)? });
        }
    }
    Ok(slots)
}

pub fn get_slot(conn: &Connection, id: i64) -> AppResult<Slot> {
    let mut slot =
        conn.query_row(&format!("SELECT {SLOT_COLUMNS} FROM slots WHERE id = ?1"), [id], slot_from_row)?;
    slot.tries = conn
        .prepare("SELECT source_path, tried FROM slot_tries WHERE slot_id = ?1 ORDER BY tried DESC")?
        .query_map([id], |r| Ok(SlotTry { source_path: r.get(0)?, tried: r.get(1)? }))?
        .collect::<Result<_, _>>()?;
    Ok(slot)
}

/// Absolute path of a slot's file.
pub fn slot_file(conn: &Connection, slot_id: i64) -> AppResult<PathBuf> {
    let (project_id, rel_path): (i64, String) =
        conn.query_row("SELECT project_id, rel_path FROM slots WHERE id = ?1", [slot_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    Ok(Path::new(&project_path(conn, project_id)?).join(rel_path))
}

/// Stores what the analysis of a slot's file found.
fn store_analysis(
    conn: &Connection,
    slot_id: i64,
    file: &Path,
    analysis: &Result<Analysis, String>,
) -> AppResult<()> {
    let meta = std::fs::metadata(file).ok();
    let size = meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
    let modified = meta.and_then(|m| m.modified().ok()).map(unix_secs).unwrap_or(0);
    let empty = Analysis::default();
    let (a, error) = match analysis {
        Ok(a) => (a, None),
        Err(e) => (&empty, Some(e.as_str())),
    };
    conn.execute(
        "UPDATE slots SET size = ?2, modified = ?3, duration = ?4, sample_rate = ?5, channels = ?6,
            waveform = ?7, error = ?8 WHERE id = ?1",
        params![
            slot_id,
            size,
            modified,
            a.duration,
            a.sample_rate,
            a.channels,
            (!a.waveform.is_empty()).then_some(&a.waveform),
            error
        ],
    )?;
    Ok(())
}

fn unix_secs(t: std::time::SystemTime) -> i64 {
    t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Brings a project's slots in line with its folder: new files become slots, changed ones are
/// analysed again, and slots whose file is gone are dropped. Returns the slots.
pub fn refresh(db: &Mutex<Connection>, project_id: i64) -> AppResult<Vec<Slot>> {
    let (root, known) = {
        let conn = db.lock().unwrap();
        let root = project_path(&conn, project_id)?;
        let known: HashMap<String, (i64, i64, i64)> = conn
            .prepare("SELECT rel_path, id, size, modified FROM slots WHERE project_id = ?1")?
            .query_map([project_id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?
            .collect::<Result<_, _>>()?;
        (root, known)
    };
    let root = PathBuf::from(root);
    if !root.is_dir() {
        // Probably an unplugged drive: keep the slots for when it comes back.
        return Err(invalid(format!("Project folder not found: {}", root.display())));
    }

    let files = crate::scan::list_files(project_id, &root, &AtomicBool::new(false));
    let rel = |path: &str| {
        Path::new(path)
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    };
    let present: std::collections::HashSet<String> = files.iter().map(|f| rel(&f.path)).collect();
    let changed: Vec<_> = files
        .iter()
        .filter(|f| {
            known
                .get(&rel(&f.path))
                .is_none_or(|&(_, size, modified)| size != f.size || modified != f.modified)
        })
        .collect();
    // Analysed without holding the database, so that the rest of the app stays responsive.
    let analysed: Vec<_> = changed.par_iter().map(|f| (*f, analysis::analyze(Path::new(&f.path)))).collect();

    let conn = db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    for (file, analysis) in &analysed {
        tx.execute(
            "INSERT INTO slots (project_id, rel_path) VALUES (?1, ?2) ON CONFLICT(project_id, rel_path) DO NOTHING",
            params![project_id, rel(&file.path)],
        )?;
        let id: i64 = tx.query_row(
            "SELECT id FROM slots WHERE project_id = ?1 AND rel_path = ?2",
            params![project_id, rel(&file.path)],
            |r| r.get(0),
        )?;
        store_analysis(&tx, id, Path::new(&file.path), analysis)?;
    }
    for (rel_path, (id, _, _)) in &known {
        if !present.contains(rel_path) {
            tx.execute("DELETE FROM slots WHERE id = ?1", [id])?;
        }
    }
    tx.commit()?;
    list_slots(&conn, project_id)
}

// ---- replacing -------------------------------------------------------------------------------

/// Extension with spelling variants folded together.
fn format_of(path: &Path) -> String {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "wave" => "wav".into(),
        "aif" => "aiff".into(),
        "oga" => "ogg".into(),
        _ => ext,
    }
}

/// A hidden file next to `target`, for writing before renaming it over `target`. Hidden files
/// are skipped by scans, and most engines ignore them.
fn temp_path(target: &Path) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    target.with_file_name(format!(".{name}.soundhive-tmp"))
}

/// Puts the temporary file in place of `target` in one step.
fn commit_temp(temp: &Path, target: &Path) -> AppResult<()> {
    std::fs::rename(temp, target).map_err(|e| {
        let _ = std::fs::remove_file(temp);
        io_error(format!("Could not replace {}: {e}. Is the file open in another program?", target.display()))
    })
}

/// Writes 16-bit PCM WAV.
fn write_wav(path: &Path, pcm: &analysis::Pcm) -> std::io::Result<()> {
    let channels = pcm.channels.max(1);
    let data_len = pcm.samples.len() as u64 * 2;
    if data_len + 36 > u32::MAX as u64 {
        return Err(std::io::Error::other("too long for a WAV file"));
    }
    let mut f = BufWriter::new(File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len as u32).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&pcm.sample_rate.to_le_bytes())?;
    f.write_all(&(pcm.sample_rate * channels as u32 * 2).to_le_bytes())?;
    f.write_all(&(channels * 2).to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&(data_len as u32).to_le_bytes())?;
    for &s in &pcm.samples {
        let s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
        f.write_all(&((s * 32767.0).round() as i16).to_le_bytes())?;
    }
    f.flush()
}

/// Writes `source` over `target` in the target's format: copied if the formats match, converted
/// if the target is a WAV file. Other conversions are refused.
pub fn write_into(source: &Path, target: &Path) -> AppResult<()> {
    let (from, to) = (format_of(source), format_of(target));
    let temp = temp_path(target);
    let written = if from == to {
        std::fs::copy(source, &temp)
            .map(|_| ())
            .map_err(|e| io_error(format!("Could not copy the sound: {e}")))
    } else if to == "wav" {
        analysis::decode(source, MAX_CONVERT_SECONDS)
            .map_err(|e| invalid(format!("Could not convert the sound: {e}")))
            .and_then(|pcm| {
                write_wav(&temp, &pcm).map_err(|e| io_error(format!("Could not write the sound: {e}")))
            })
    } else {
        let (from, to) = (from.to_uppercase(), to.to_uppercase());
        return Err(invalid(format!(
            "The project uses {to} here, but this sound is {from}. Pick a {to} sound: converting to {to} is not supported yet."
        )));
    };
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    commit_temp(&temp, target)
}

/// Backs up a slot's original file, once, before it is first replaced.
fn ensure_backup(conn: &Connection, data_dir: &Path, slot: &Slot, file: &Path) -> AppResult<()> {
    if slot.backed_up {
        return Ok(());
    }
    let backup = backup_dir(data_dir, slot.project_id).join(&slot.rel_path);
    if let Some(dir) = backup.parent() {
        std::fs::create_dir_all(dir).map_err(io_error)?;
    }
    std::fs::copy(file, &backup).map_err(|e| io_error(format!("Could not back up the original: {e}")))?;
    conn.execute("UPDATE slots SET backed_up = 1 WHERE id = ?1", [slot.id])?;
    Ok(())
}

/// Puts a library sound into a slot, to try it out in the project.
pub fn replace(conn: &Connection, data_dir: &Path, slot_id: i64, sound_id: i64) -> AppResult<Slot> {
    let slot = get_slot(conn, slot_id)?;
    let target = slot_file(conn, slot_id)?;
    let source = PathBuf::from(db::sound_paths(conn, &[sound_id])?.remove(0));
    if !source.is_file() {
        return Err(invalid(format!("File not found: {}", source.display())));
    }
    if !target.is_file() {
        return Err(invalid(format!("{} is gone from the project. Refresh the project.", slot.rel_path)));
    }
    ensure_backup(conn, data_dir, &slot, &target)?;
    write_into(&source, &target)?;
    store_analysis(conn, slot_id, &target, &analysis::analyze(&target))?;
    let source = source.to_string_lossy();
    conn.execute(
        "UPDATE slots SET status = 'trying', source_path = ?2 WHERE id = ?1",
        params![slot_id, source],
    )?;
    conn.execute(
        "INSERT INTO slot_tries (slot_id, source_path, tried) VALUES (?1, ?2, ?3)
         ON CONFLICT(slot_id, source_path) DO UPDATE SET tried = excluded.tried",
        params![slot_id, source, now_ms()],
    )?;
    get_slot(conn, slot_id)
}

/// Puts a slot's original file back.
pub fn restore(conn: &Connection, data_dir: &Path, slot_id: i64) -> AppResult<Slot> {
    let slot = get_slot(conn, slot_id)?;
    if !slot.backed_up {
        return Err(invalid("This slot still holds its original sound."));
    }
    let backup = backup_dir(data_dir, slot.project_id).join(&slot.rel_path);
    if !backup.is_file() {
        return Err(invalid(format!("The backup of the original is gone: {}", backup.display())));
    }
    let target = slot_file(conn, slot_id)?;
    let temp = temp_path(&target);
    std::fs::copy(&backup, &temp).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        io_error(format!("Could not restore the original: {e}"))
    })?;
    commit_temp(&temp, &target)?;
    store_analysis(conn, slot_id, &target, &analysis::analyze(&target))?;
    conn.execute("UPDATE slots SET status = 'placeholder', source_path = NULL WHERE id = ?1", [slot_id])?;
    get_slot(conn, slot_id)
}

/// Marks a slot as done, or opens it again.
pub fn set_approved(conn: &Connection, slot_id: i64, approved: bool) -> AppResult<Slot> {
    let slot = get_slot(conn, slot_id)?;
    let status = match (approved, &slot.source_path) {
        (true, _) => SlotStatus::Approved,
        (false, Some(_)) => SlotStatus::Trying,
        (false, None) => SlotStatus::Placeholder,
    };
    conn.execute("UPDATE slots SET status = ?2 WHERE id = ?1", params![slot_id, status.as_str()])?;
    get_slot(conn, slot_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{sine, write_wav as write_test_wav};

    struct Quiet;
    impl crate::scan::ScanListener for Quiet {
        fn progress(&self, _: crate::models::ScanProgress) {}
        fn library_changed(&self) {}
    }

    struct Setup {
        _dir: tempfile::TempDir,
        db: Mutex<Connection>,
        game: PathBuf,
        data: PathBuf,
        library: PathBuf,
        project: i64,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let library = dir.path().join("library");
        let data = dir.path().join("data");
        std::fs::create_dir_all(game.join("sfx")).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        let placeholder = sine(22050, 0.2, 440.0, |_| 0.3);
        write_test_wav(&game.join("sfx/door_open.wav"), 22050, 1, &placeholder, &[]);
        write_test_wav(&game.join("sfx/jump.wav"), 22050, 1, &placeholder, &[]);
        write_test_wav(&library.join("real door.wav"), 44100, 2, &sine(44100, 0.5, 220.0, |_| 0.5), &[]);
        let conn = db::open_in_memory();
        db::add_folder(&conn, &library.to_string_lossy()).unwrap();
        crate::scan::scan(&conn, &crate::scan::Scope::All, &AtomicBool::new(false), &Quiet).unwrap();
        let project = add_project(&conn, &game.to_string_lossy()).unwrap();
        Setup { _dir: dir, db: Mutex::new(conn), game, data, library, project }
    }

    fn sound_id(conn: &Connection, path: &Path) -> i64 {
        conn.query_row("SELECT id FROM sounds WHERE path = ?1", [path.to_string_lossy()], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn replace_try_approve_and_restore() {
        let s = setup();
        let slots = refresh(&s.db, s.project).unwrap();
        assert_eq!(
            slots.iter().map(|s| s.rel_path.as_str()).collect::<Vec<_>>(),
            ["sfx/door_open.wav", "sfx/jump.wav"]
        );
        let door = &slots[0];
        assert_eq!((door.name.as_str(), door.dir.as_str(), door.ext.as_str()), ("door_open", "sfx", "wav"));
        assert_eq!(door.status, SlotStatus::Placeholder);
        let original = std::fs::read(s.game.join("sfx/door_open.wav")).unwrap();

        let conn = s.db.lock().unwrap();
        let real = sound_id(&conn, &s.library.join("real door.wav"));
        let slot = replace(&conn, &s.data, door.id, real).unwrap();
        assert_eq!(slot.status, SlotStatus::Trying);
        assert!(slot.backed_up);
        assert_eq!(slot.channels, 2);
        assert!((slot.duration - 0.25).abs() < 0.01, "{}", slot.duration); // stereo: interleaved
        assert_eq!(slot.tries.len(), 1);
        assert_eq!(
            std::fs::read(s.game.join("sfx/door_open.wav")).unwrap(),
            std::fs::read(s.library.join("real door.wav")).unwrap()
        );
        // No temporary files are left behind.
        assert_eq!(std::fs::read_dir(s.game.join("sfx")).unwrap().count(), 2);

        // Replacing again keeps the first backup: the original.
        replace(&conn, &s.data, door.id, real).unwrap();
        let slot = set_approved(&conn, door.id, true).unwrap();
        assert_eq!(slot.status, SlotStatus::Approved);
        assert_eq!(list_projects(&conn).unwrap()[0].approved_count, 1);
        assert_eq!(set_approved(&conn, door.id, false).unwrap().status, SlotStatus::Trying);

        let slot = restore(&conn, &s.data, door.id).unwrap();
        assert_eq!(slot.status, SlotStatus::Placeholder);
        assert_eq!(slot.source_path, None);
        assert_eq!(slot.tries.len(), 1, "history is kept");
        assert_eq!(std::fs::read(s.game.join("sfx/door_open.wav")).unwrap(), original);
        drop(conn);

        // The library is never written to.
        assert_eq!(std::fs::read_dir(&s.library).unwrap().count(), 1);

        // A removed file drops its slot.
        std::fs::remove_file(s.game.join("sfx/jump.wav")).unwrap();
        assert_eq!(refresh(&s.db, s.project).unwrap().len(), 1);

        let conn = s.db.lock().unwrap();
        remove_project(&conn, &s.data, s.project).unwrap();
        assert!(!s.data.join("projects").join(s.project.to_string()).exists());
    }

    #[test]
    fn converts_to_wav_but_not_to_other_formats() {
        let dir = tempfile::tempdir().unwrap();
        // Formats are told apart by content, so a WAV named .aiff stands in for any other format.
        let src = dir.path().join("hit.aiff");
        write_test_wav(&src, 48000, 1, &sine(48000, 0.25, 330.0, |_| 0.5), &[]);

        let ogg = dir.path().join("game.ogg");
        std::fs::write(&ogg, b"old").unwrap();
        let err = write_into(&src, &ogg).unwrap_err();
        assert!(err.to_string().contains("OGG"), "{err}");
        assert_eq!(std::fs::read(&ogg).unwrap(), b"old");

        let wav = dir.path().join("game.wav");
        std::fs::write(&wav, b"old").unwrap();
        write_into(&src, &wav).unwrap();
        let a = analysis::analyze(&wav).unwrap();
        assert!((a.duration - 0.25).abs() < 0.01, "{}", a.duration);
        assert_eq!((a.sample_rate, a.channels, a.bits), (48000, 1, 16));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3, "no temporary files are left");
    }

    #[test]
    fn projects_are_unique() {
        let s = setup();
        let conn = s.db.lock().unwrap();
        assert!(add_project(&conn, &s.game.to_string_lossy()).is_err());
        assert!(add_project(&conn, "/definitely/not/here").is_err());
    }
}
