//! SQLite storage of folders, sounds, tags and settings.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, params, params_from_iter};

use crate::analysis::Analysis;
use crate::error::{AppResult, invalid};
use crate::models::{Folder, Settings, Sound, Waveform};
use crate::tagging::normalize_tag;

/// Schema migrations, applied in order; `user_version` counts the applied ones.
const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE folders (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    last_scan INTEGER,
    available INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE sounds (
    id INTEGER PRIMARY KEY,
    folder_id INTEGER NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    dir TEXT NOT NULL,
    name TEXT NOT NULL,
    ext TEXT NOT NULL,
    size INTEGER NOT NULL,
    modified INTEGER NOT NULL,
    duration REAL NOT NULL DEFAULT 0,
    sample_rate INTEGER NOT NULL DEFAULT 0,
    channels INTEGER NOT NULL DEFAULT 0,
    bits INTEGER NOT NULL DEFAULT 0,
    peak_db REAL NOT NULL DEFAULT -120,
    rms_db REAL NOT NULL DEFAULT -120,
    zcr REAL NOT NULL DEFAULT 0,
    attack REAL NOT NULL DEFAULT 0,
    -- Loop found by the analysis (marked / seamless) ...
    analysis_loop TEXT,
    -- ... and including loops by name; what the app shows.
    loop_kind TEXT,
    description TEXT NOT NULL DEFAULT '',
    waveform BLOB,
    error TEXT,
    analysis_version INTEGER NOT NULL DEFAULT 0,
    favorite INTEGER NOT NULL DEFAULT 0,
    play_count INTEGER NOT NULL DEFAULT 0,
    last_played INTEGER,
    added INTEGER NOT NULL
);
CREATE INDEX sounds_folder ON sounds(folder_id);
-- source: 'auto' (from the scan), 'user' (added by the user) or 'removed' (an automatic tag the
-- user removed; kept so that the next scan does not bring it back).
CREATE TABLE tags (
    sound_id INTEGER NOT NULL REFERENCES sounds(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    source TEXT NOT NULL,
    PRIMARY KEY (sound_id, tag)
) WITHOUT ROWID;
CREATE INDEX tags_tag ON tags(tag);
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
    r#"
-- Target projects: folders (of a game, a video, …) whose sound files ("slots") get replaced with library sounds.
CREATE TABLE projects (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    added INTEGER NOT NULL
);
CREATE TABLE slots (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- Relative to the project folder, `/`-separated.
    rel_path TEXT NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    modified INTEGER NOT NULL DEFAULT 0,
    duration REAL NOT NULL DEFAULT 0,
    sample_rate INTEGER NOT NULL DEFAULT 0,
    channels INTEGER NOT NULL DEFAULT 0,
    waveform BLOB,
    error TEXT,
    -- 'placeholder', 'trying' or 'approved'.
    status TEXT NOT NULL DEFAULT 'placeholder',
    -- The library file SoundHive last put into the slot; null while it holds its original.
    source_path TEXT,
    -- Whether the original file has been backed up (before the first replacement).
    backed_up INTEGER NOT NULL DEFAULT 0,
    UNIQUE (project_id, rel_path)
);
-- Every library file tried in a slot.
CREATE TABLE slot_tries (
    slot_id INTEGER NOT NULL REFERENCES slots(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    -- Unix milliseconds of the last time it was put in.
    tried INTEGER NOT NULL,
    PRIMARY KEY (slot_id, source_path)
) WITHOUT ROWID;
"#,
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    conn
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---- settings --------------------------------------------------------------------------------

pub fn get_settings(conn: &Connection) -> AppResult<Settings> {
    let json: Option<String> =
        conn.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

pub fn save_settings(conn: &Connection, settings: &Settings) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('settings', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [serde_json::to_string(settings)?],
    )?;
    Ok(())
}

// ---- folders ---------------------------------------------------------------------------------

pub(crate) fn folder_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

pub fn list_folders(conn: &Connection) -> AppResult<Vec<Folder>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.path, f.last_scan, f.available, (SELECT COUNT(*) FROM sounds s WHERE s.folder_id = f.id)
         FROM folders f ORDER BY f.path COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let path: String = r.get(1)?;
        Ok(Folder {
            id: r.get(0)?,
            name: folder_name(&path),
            path,
            last_scan: r.get(2)?,
            available: r.get(3)?,
            sound_count: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Adds a library folder. Folders inside or around existing ones are refused, since their files
/// would be in the library twice.
pub fn add_folder(conn: &Connection, path: &str) -> AppResult<i64> {
    let path = path.trim_end_matches(['/', '\\']);
    let path = if path.is_empty() { "/" } else { path };
    if !Path::new(path).is_dir() {
        return Err(invalid(format!("Folder not found: {path}")));
    }
    let new = Path::new(path);
    for folder in list_folders(conn)? {
        let old = Path::new(&folder.path);
        if new == old {
            return Err(invalid("This folder is already in the library."));
        }
        if new.starts_with(old) {
            return Err(invalid(format!("This folder is already part of {}.", folder.path)));
        }
        if old.starts_with(new) {
            return Err(invalid(format!(
                "{} is already in the library. Remove it first to add the folder around it.",
                folder.path
            )));
        }
    }
    conn.execute("INSERT INTO folders (path) VALUES (?1)", [path])?;
    Ok(conn.last_insert_rowid())
}

pub fn remove_folder(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", [id])?;
    Ok(())
}

pub fn folder_scanned(conn: &Connection, id: i64, available: bool) -> AppResult<()> {
    if available {
        conn.execute("UPDATE folders SET last_scan = ?2, available = 1 WHERE id = ?1", params![id, now()])?;
    } else {
        conn.execute("UPDATE folders SET available = 0 WHERE id = ?1", [id])?;
    }
    Ok(())
}

// ---- sounds ----------------------------------------------------------------------------------

const SOUND_COLUMNS: &str = "id, folder_id, path, dir, name, ext, size, modified, duration, sample_rate,
    channels, bits, peak_db, rms_db, zcr, attack, loop_kind, description, favorite, play_count,
    last_played, added, error";

fn sound_from_row(r: &rusqlite::Row) -> rusqlite::Result<Sound> {
    Ok(Sound {
        id: r.get(0)?,
        folder_id: r.get(1)?,
        path: r.get(2)?,
        dir: r.get(3)?,
        name: r.get(4)?,
        ext: r.get(5)?,
        size: r.get(6)?,
        modified: r.get(7)?,
        duration: r.get(8)?,
        sample_rate: r.get(9)?,
        channels: r.get(10)?,
        bits: r.get(11)?,
        peak_db: r.get(12)?,
        rms_db: r.get(13)?,
        zcr: r.get(14)?,
        attack: r.get(15)?,
        loop_kind: r.get(16)?,
        description: r.get(17)?,
        favorite: r.get(18)?,
        play_count: r.get(19)?,
        last_played: r.get(20)?,
        added: r.get(21)?,
        error: r.get(22)?,
        auto_tags: Vec::new(),
        user_tags: Vec::new(),
    })
}

/// All sounds with their tags, in no particular order.
pub fn list_sounds(conn: &Connection) -> AppResult<Vec<Sound>> {
    let mut stmt = conn.prepare(&format!("SELECT {SOUND_COLUMNS} FROM sounds"))?;
    let mut sounds: Vec<Sound> = stmt.query_map([], sound_from_row)?.collect::<Result<_, _>>()?;
    let index: HashMap<i64, usize> = sounds.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut stmt =
        conn.prepare("SELECT sound_id, tag, source FROM tags WHERE source != 'removed' ORDER BY tag")?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        if let Some(&i) = index.get(&id) {
            let tag: String = r.get(1)?;
            let source: String = r.get(2)?;
            if source == "user" {
                sounds[i].user_tags.push(tag);
            } else {
                sounds[i].auto_tags.push(tag);
            }
        }
    }
    Ok(sounds)
}

pub fn get_sound(conn: &Connection, id: i64) -> AppResult<Sound> {
    let mut sound =
        conn.query_row(&format!("SELECT {SOUND_COLUMNS} FROM sounds WHERE id = ?1"), [id], sound_from_row)?;
    let mut stmt = conn
        .prepare("SELECT tag, source FROM tags WHERE sound_id = ?1 AND source != 'removed' ORDER BY tag")?;
    for row in stmt.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (tag, source) = row?;
        if source == "user" { sound.user_tags.push(tag) } else { sound.auto_tags.push(tag) }
    }
    Ok(sound)
}

fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

pub fn waveforms(conn: &Connection, ids: &[i64]) -> AppResult<Vec<Waveform>> {
    let mut out = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(500) {
        let sql = format!(
            "SELECT id, waveform FROM sounds WHERE id IN ({}) AND waveform IS NOT NULL",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows =
            stmt.query_map(params_from_iter(chunk), |r| Ok(Waveform { id: r.get(0)?, peaks: r.get(1)? }))?;
        for row in rows {
            out.push(row?);
        }
    }
    Ok(out)
}

pub fn sound_paths(conn: &Connection, ids: &[i64]) -> AppResult<Vec<String>> {
    let mut paths = Vec::with_capacity(ids.len());
    let mut stmt = conn.prepare("SELECT path FROM sounds WHERE id = ?1")?;
    for &id in ids {
        paths.push(stmt.query_row([id], |r| r.get(0))?);
    }
    Ok(paths)
}

pub fn set_favorite(conn: &Connection, ids: &[i64], favorite: bool) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare("UPDATE sounds SET favorite = ?2 WHERE id = ?1")?;
        for &id in ids {
            stmt.execute(params![id, favorite])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn record_play(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE sounds SET play_count = play_count + 1, last_played = ?2 WHERE id = ?1",
        params![id, now()],
    )?;
    Ok(())
}

// ---- tags ------------------------------------------------------------------------------------

pub fn add_tag(conn: &Connection, ids: &[i64], tag: &str) -> AppResult<String> {
    let tag = normalize_tag(tag);
    if tag.is_empty() {
        return Err(invalid("Tags cannot be empty."));
    }
    let tx = conn.unchecked_transaction()?;
    {
        // An automatic tag stays automatic; a removed one comes back as the user's.
        let mut stmt = tx.prepare(
            "INSERT INTO tags (sound_id, tag, source) VALUES (?1, ?2, 'user')
             ON CONFLICT(sound_id, tag) DO UPDATE SET source = CASE WHEN source = 'auto' THEN 'auto' ELSE 'user' END",
        )?;
        for &id in ids {
            stmt.execute(params![id, tag])?;
        }
    }
    tx.commit()?;
    Ok(tag)
}

pub fn remove_tag(conn: &Connection, ids: &[i64], tag: &str) -> AppResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut user = tx.prepare("DELETE FROM tags WHERE sound_id = ?1 AND tag = ?2 AND source = 'user'")?;
        let mut auto = tx.prepare(
            "UPDATE tags SET source = 'removed' WHERE sound_id = ?1 AND tag = ?2 AND source = 'auto'",
        )?;
        for &id in ids {
            user.execute(params![id, tag])?;
            auto.execute(params![id, tag])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Renames a tag on every sound; it becomes a user tag there.
pub fn rename_tag(conn: &Connection, from: &str, to: &str) -> AppResult<()> {
    let ids: Vec<i64> = conn
        .prepare("SELECT sound_id FROM tags WHERE tag = ?1 AND source != 'removed'")?
        .query_map([from], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let to = add_tag(conn, &ids, to)?;
    if to != from {
        remove_tag(conn, &ids, from)?;
    }
    Ok(())
}

/// Replaces a sound's automatic tags, leaving the user's tags and removals alone.
pub fn set_auto_tags(tx: &Transaction, sound_id: i64, tags: &[String]) -> rusqlite::Result<()> {
    let existing: Vec<(String, String)> = tx
        .prepare_cached("SELECT tag, source FROM tags WHERE sound_id = ?1")?
        .query_map([sound_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (tag, source) in &existing {
        if source == "auto" && !tags.contains(tag) {
            tx.prepare_cached("DELETE FROM tags WHERE sound_id = ?1 AND tag = ?2")?
                .execute(params![sound_id, tag])?;
        }
    }
    let mut insert =
        tx.prepare_cached("INSERT OR IGNORE INTO tags (sound_id, tag, source) VALUES (?1, ?2, 'auto')")?;
    for tag in tags {
        insert.execute(params![sound_id, tag])?;
    }
    Ok(())
}

// ---- scanning --------------------------------------------------------------------------------

/// What the database knows about a file, to decide whether it needs analysing again.
#[derive(Debug, Clone)]
pub struct Known {
    pub id: i64,
    pub size: i64,
    pub modified: i64,
    pub version: i64,
}

pub fn known_files(conn: &Connection, folder_id: i64) -> AppResult<HashMap<String, Known>> {
    let mut stmt =
        conn.prepare("SELECT path, id, size, modified, analysis_version FROM sounds WHERE folder_id = ?1")?;
    let rows = stmt.query_map([folder_id], |r| {
        Ok((r.get(0)?, Known { id: r.get(1)?, size: r.get(2)?, modified: r.get(3)?, version: r.get(4)? }))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// A file found by the scan.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub folder_id: i64,
    pub path: String,
    pub dir: String,
    pub name: String,
    pub ext: String,
    pub size: i64,
    pub modified: i64,
}

/// Inserts or updates a file with its analysis; returns its id.
pub fn upsert_sound(
    tx: &Transaction,
    file: &FileEntry,
    analysis: &Result<Analysis, String>,
    loop_kind: Option<&str>,
) -> rusqlite::Result<i64> {
    let empty = Analysis::default();
    let (a, error) = match analysis {
        Ok(a) => (a, None),
        Err(e) => (&empty, Some(e.as_str())),
    };
    let waveform = (!a.waveform.is_empty()).then_some(&a.waveform);
    tx.prepare_cached(
        "INSERT INTO sounds (folder_id, path, dir, name, ext, size, modified, duration, sample_rate,
            channels, bits, peak_db, rms_db, zcr, attack, analysis_loop, loop_kind, description, waveform,
            error, analysis_version, added)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
         ON CONFLICT(path) DO UPDATE SET folder_id = excluded.folder_id, dir = excluded.dir,
            name = excluded.name, ext = excluded.ext, size = excluded.size, modified = excluded.modified,
            duration = excluded.duration, sample_rate = excluded.sample_rate, channels = excluded.channels,
            bits = excluded.bits, peak_db = excluded.peak_db, rms_db = excluded.rms_db, zcr = excluded.zcr,
            attack = excluded.attack, analysis_loop = excluded.analysis_loop, loop_kind = excluded.loop_kind,
            description = excluded.description, waveform = excluded.waveform, error = excluded.error,
            analysis_version = excluded.analysis_version",
    )?
    .execute(params![
        file.folder_id,
        file.path,
        file.dir,
        file.name,
        file.ext,
        file.size,
        file.modified,
        a.duration,
        a.sample_rate,
        a.channels,
        a.bits,
        a.peak_db,
        a.rms_db,
        a.zcr,
        a.attack,
        a.loop_kind,
        loop_kind,
        a.description,
        waveform,
        error,
        crate::analysis::ANALYSIS_VERSION,
        now(),
    ])?;
    tx.query_row("SELECT id FROM sounds WHERE path = ?1", [&file.path], |r| r.get(0))
}

/// Gives a moved file the favourite flag, play count and tags of the file it replaces.
pub fn carry_over(tx: &Transaction, from: i64, to: i64) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE sounds SET (favorite, play_count, last_played, added) =
            (SELECT favorite, play_count, last_played, added FROM sounds WHERE id = ?1) WHERE id = ?2",
        params![from, to],
    )?;
    tx.execute(
        "INSERT INTO tags (sound_id, tag, source) SELECT ?2, tag, source FROM tags WHERE sound_id = ?1 AND source != 'auto'
         ON CONFLICT(sound_id, tag) DO UPDATE SET source = excluded.source",
        params![from, to],
    )?;
    Ok(())
}

pub fn delete_sounds(tx: &Transaction, ids: &[i64]) -> rusqlite::Result<()> {
    let mut stmt = tx.prepare_cached("DELETE FROM sounds WHERE id = ?1")?;
    for id in ids {
        stmt.execute([id])?;
    }
    Ok(())
}

/// What re-tagging needs of every sound.
pub struct TagSource {
    pub id: i64,
    pub name: String,
    pub dir: String,
    pub duration: f64,
    pub attack: f64,
    pub analysis_loop: Option<String>,
}

pub fn tag_sources(conn: &Connection, ids: Option<&[i64]>) -> AppResult<Vec<TagSource>> {
    let map = |r: &rusqlite::Row| {
        Ok(TagSource {
            id: r.get(0)?,
            name: r.get(1)?,
            dir: r.get(2)?,
            duration: r.get(3)?,
            attack: r.get(4)?,
            analysis_loop: r.get(5)?,
        })
    };
    let sql = "SELECT id, name, dir, duration, attack, analysis_loop FROM sounds";
    match ids {
        None => Ok(conn.prepare(sql)?.query_map([], map)?.collect::<Result<_, _>>()?),
        Some(ids) => {
            let mut out = Vec::new();
            let mut stmt = conn.prepare(&format!("{sql} WHERE id = ?1"))?;
            for &id in ids {
                out.push(stmt.query_row([id], map)?);
            }
            Ok(out)
        }
    }
}

pub fn set_loop_kind(tx: &Transaction, id: i64, loop_kind: Option<&str>) -> rusqlite::Result<()> {
    tx.prepare_cached("UPDATE sounds SET loop_kind = ?2 WHERE id = ?1")?.execute(params![id, loop_kind])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(conn: &Connection, folder: i64, path: &str) -> i64 {
        let tx = conn.unchecked_transaction().unwrap();
        let file = FileEntry {
            folder_id: folder,
            path: path.into(),
            dir: String::new(),
            name: "x".into(),
            ext: "wav".into(),
            size: 1,
            modified: 1,
        };
        let id = upsert_sound(&tx, &file, &Ok(Analysis::default()), None).unwrap();
        tx.commit().unwrap();
        id
    }

    fn tags(conn: &Connection, id: i64) -> (Vec<String>, Vec<String>) {
        let s = get_sound(conn, id).unwrap();
        (s.auto_tags, s.user_tags)
    }

    #[test]
    fn auto_and_user_tags() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_in_memory();
        let folder = add_folder(&conn, &dir.path().to_string_lossy()).unwrap();
        let id = insert(&conn, folder, "/a.wav");

        let tx = conn.unchecked_transaction().unwrap();
        set_auto_tags(&tx, id, &["door".into(), "impact".into()]).unwrap();
        tx.commit().unwrap();
        add_tag(&conn, &[id], "  My   Favs ").unwrap();
        add_tag(&conn, &[id], "door").unwrap();
        assert_eq!(tags(&conn, id), (vec!["door".into(), "impact".into()], vec!["my favs".into()]));

        // A removed automatic tag stays removed when the scan finds it again.
        remove_tag(&conn, &[id], "impact").unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        set_auto_tags(&tx, id, &["impact".into(), "wood".into()]).unwrap();
        tx.commit().unwrap();
        assert_eq!(tags(&conn, id), (vec!["wood".into()], vec!["my favs".into()]));

        rename_tag(&conn, "wood", "timber").unwrap();
        assert_eq!(tags(&conn, id), (vec![], vec!["my favs".into(), "timber".into()]));
    }

    #[test]
    fn nested_folders_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner");
        std::fs::create_dir(&inner).unwrap();
        let conn = open_in_memory();
        add_folder(&conn, &inner.to_string_lossy()).unwrap();
        assert!(add_folder(&conn, &inner.to_string_lossy()).is_err());
        assert!(add_folder(&conn, &dir.path().to_string_lossy()).is_err());
        assert!(add_folder(&conn, "/definitely/not/here").is_err());
    }

    #[test]
    fn carry_over_keeps_user_data() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_in_memory();
        let folder = add_folder(&conn, &dir.path().to_string_lossy()).unwrap();
        let old = insert(&conn, folder, "/old.wav");
        let new = insert(&conn, folder, "/new.wav");
        set_favorite(&conn, &[old], true).unwrap();
        add_tag(&conn, &[old], "keeper").unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        carry_over(&tx, old, new).unwrap();
        delete_sounds(&tx, &[old]).unwrap();
        tx.commit().unwrap();
        let s = get_sound(&conn, new).unwrap();
        assert!(s.favorite);
        assert_eq!(s.user_tags, ["keeper"]);
    }
}
