//! Library scans. Listing a folder is cheap; only new and changed files are decoded, on all CPU
//! cores. A scan runs on its own thread with its own database connection, so the UI stays
//! responsive.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, UNIX_EPOCH};

use rayon::prelude::*;
use rusqlite::Connection;

use crate::analysis::{self, ANALYSIS_VERSION, Analysis};
use crate::db::{self, FileEntry};
use crate::error::AppResult;
use crate::models::{ScanProgress, Settings};
use crate::tagging::{self, TagInput};

/// Receives progress and the news that the library changed.
pub trait ScanListener: Send + Sync + 'static {
    fn progress(&self, progress: ScanProgress);
    fn library_changed(&self);
}

/// Which folders to scan.
#[derive(Debug, Clone, PartialEq)]
pub enum Scope {
    All,
    Folders(HashSet<i64>),
}

impl Scope {
    fn merge(self, other: Scope) -> Scope {
        match (self, other) {
            (Scope::Folders(mut a), Scope::Folders(b)) => {
                a.extend(b);
                Scope::Folders(a)
            }
            _ => Scope::All,
        }
    }

    fn includes(&self, id: i64) -> bool {
        match self {
            Scope::All => true,
            Scope::Folders(ids) => ids.contains(&id),
        }
    }
}

/// Runs one scan at a time; requests during a scan are merged and run right after it.
pub struct Scanner {
    db_path: PathBuf,
    listener: Arc<dyn ScanListener>,
    state: Mutex<ScannerState>,
    cancel: AtomicBool,
}

#[derive(Default)]
struct ScannerState {
    running: bool,
    pending: Option<Scope>,
}

impl Scanner {
    pub fn new(db_path: PathBuf, listener: Arc<dyn ScanListener>) -> Arc<Self> {
        Arc::new(Scanner { db_path, listener, state: Mutex::default(), cancel: AtomicBool::new(false) })
    }

    pub fn is_running(&self) -> bool {
        self.state.lock().unwrap().running
    }

    pub fn start(self: &Arc<Self>, scope: Scope) {
        {
            let mut state = self.state.lock().unwrap();
            if state.running {
                state.pending = Some(match state.pending.take() {
                    Some(p) => p.merge(scope),
                    None => scope,
                });
                return;
            }
            state.running = true;
        }
        let scanner = self.clone();
        std::thread::Builder::new()
            .name("scan".into())
            .spawn(move || {
                let mut scope = scope;
                loop {
                    scanner.cancel.store(false, Ordering::SeqCst);
                    if let Err(e) = scanner.run(&scope) {
                        scanner.listener.progress(ScanProgress {
                            phase: "done".into(),
                            done: 0,
                            total: 0,
                            current: String::new(),
                            summary: format!("Scan failed: {e}"),
                        });
                    }
                    scanner.listener.library_changed();
                    let mut state = scanner.state.lock().unwrap();
                    match state.pending.take() {
                        Some(next) => scope = next,
                        None => {
                            state.running = false;
                            break;
                        }
                    }
                }
            })
            .expect("could not start the scan thread");
    }

    pub fn cancel(&self) {
        self.state.lock().unwrap().pending = None;
        self.cancel.store(true, Ordering::SeqCst);
    }

    fn run(&self, scope: &Scope) -> AppResult<()> {
        let conn = db::open(&self.db_path)?;
        let summary = scan(&conn, scope, &self.cancel, self.listener.as_ref())?;
        self.listener.progress(ScanProgress {
            phase: if self.cancel.load(Ordering::SeqCst) { "cancelled" } else { "done" }.into(),
            done: summary.analysed as u32,
            total: summary.analysed as u32,
            current: String::new(),
            summary: summary.to_string(),
        });
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Summary {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub moved: usize,
    pub failed: usize,
    pub analysed: usize,
    pub missing_folders: usize,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        for (n, what) in [
            (self.added, "new"),
            (self.changed, "changed"),
            (self.moved, "moved"),
            (self.removed, "removed"),
            (self.failed, "unreadable"),
            (self.missing_folders, "folders missing"),
        ] {
            if n > 0 {
                parts.push(format!("{n} {what}"));
            }
        }
        if parts.is_empty() { write!(f, "Library is up to date") } else { write!(f, "{}", parts.join(", ")) }
    }
}

fn unix_secs(time: std::io::Result<std::time::SystemTime>) -> i64 {
    time.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Lists the sound files below `root`.
fn list_files(folder_id: i64, root: &Path, cancel: &AtomicBool) -> Vec<FileEntry> {
    walkdir::WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .take_while(|_| !cancel.load(Ordering::Relaxed))
        .filter(|e| e.file_type().is_file() && analysis::is_sound_file(e.path()))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let path = e.path();
            let dir = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            Some(FileEntry {
                folder_id,
                path: path.to_string_lossy().into_owned(),
                dir,
                name: path.file_stem()?.to_string_lossy().into_owned(),
                ext: path.extension()?.to_string_lossy().to_ascii_lowercase(),
                size: meta.len() as i64,
                modified: unix_secs(meta.modified()),
            })
        })
        .collect()
}

/// Loop kind shown in the app: from the analysis, else from the name.
fn loop_kind(analysis_loop: Option<&str>, looped_by_name: bool) -> Option<&str> {
    analysis_loop.or(looped_by_name.then_some("name"))
}

pub fn scan(
    conn: &Connection,
    scope: &Scope,
    cancel: &AtomicBool,
    listener: &dyn ScanListener,
) -> AppResult<Summary> {
    let settings = db::get_settings(conn)?;
    let mut summary = Summary::default();
    let progress = |phase: &str, done: u32, total: u32, current: &str| {
        listener.progress(ScanProgress {
            phase: phase.into(),
            done,
            total,
            current: current.into(),
            summary: String::new(),
        })
    };

    // 1. List the folders and compare with what is known.
    let mut to_analyse: Vec<(FileEntry, bool)> = Vec::new(); // (file, is new)
    let mut removed: Vec<(i64, String)> = Vec::new(); // (id, path)
    for folder in db::list_folders(conn)?.into_iter().filter(|f| scope.includes(f.id)) {
        progress("listing", 0, 0, &folder.path);
        let root = Path::new(&folder.path);
        if !root.is_dir() {
            // Probably an unplugged drive: keep its sounds for when it comes back.
            db::folder_scanned(conn, folder.id, false)?;
            summary.missing_folders += 1;
            continue;
        }
        let files = list_files(folder.id, root, cancel);
        if cancel.load(Ordering::SeqCst) {
            return Ok(summary);
        }
        let mut known = db::known_files(conn, folder.id)?;
        for file in files {
            match known.remove(&file.path) {
                Some(k)
                    if k.size == file.size
                        && k.modified == file.modified
                        && k.version == ANALYSIS_VERSION => {}
                Some(_) => to_analyse.push((file, false)),
                None => to_analyse.push((file, true)),
            }
        }
        removed.extend(known.into_iter().map(|(path, k)| (k.id, path)));
        db::folder_scanned(conn, folder.id, true)?;
    }

    // 2. Analyse in parallel, saving in batches as results come in.
    let total = to_analyse.len() as u32;
    let done = AtomicU32::new(0);
    let (tx, rx) = mpsc::sync_channel::<(usize, Result<Analysis, String>)>(256);
    let mut saved_ids: HashMap<usize, i64> = HashMap::new();
    let mut save_error = None;
    std::thread::scope(|s| {
        let files = &to_analyse;
        let done = &done;
        s.spawn(move || {
            files.par_iter().enumerate().for_each_with(tx, |tx, (i, (file, _))| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let result = analysis::analyze(Path::new(&file.path));
                done.fetch_add(1, Ordering::Relaxed);
                let _ = tx.send((i, result));
            });
        });

        let mut last_report = Instant::now() - Duration::from_secs(1);
        let mut last_refresh = Instant::now();
        let mut batch = Vec::new();
        let mut flush = |batch: &mut Vec<(usize, Result<Analysis, String>)>| -> rusqlite::Result<()> {
            let tx = conn.unchecked_transaction()?;
            for (i, result) in batch.drain(..) {
                let (file, _) = &to_analyse[i];
                let (tags, by_name, analysis_loop) = {
                    let a = result.as_ref().ok();
                    let input = TagInput {
                        name: &file.name,
                        dir: &file.dir,
                        duration: a.map_or(0.0, |a| a.duration),
                        attack: a.map_or(0.0, |a| a.attack),
                        loop_kind: a.and_then(|a| a.loop_kind.as_deref()),
                    };
                    let (tags, by_name) =
                        tagging::auto_tags(&input, &settings.tag_rules, settings.folder_tags);
                    (tags, by_name, a.and_then(|a| a.loop_kind.clone()))
                };
                let id = db::upsert_sound(&tx, file, &result, loop_kind(analysis_loop.as_deref(), by_name))?;
                db::set_auto_tags(&tx, id, &tags)?;
                saved_ids.insert(i, id);
            }
            tx.commit()
        };
        for (i, result) in rx {
            if result.is_err() {
                summary.failed += 1;
            }
            batch.push((i, result));
            if batch.len() >= 200 || last_report.elapsed() > Duration::from_millis(250) {
                if let Err(e) = flush(&mut batch) {
                    save_error = Some(e);
                    cancel.store(true, Ordering::SeqCst);
                }
                let i = batch_progress_name(&to_analyse, i);
                progress("analyzing", done.load(Ordering::Relaxed), total, &i);
                last_report = Instant::now();
                // Long first scans show their results as they come in.
                if last_refresh.elapsed() > Duration::from_secs(3) {
                    listener.library_changed();
                    last_refresh = Instant::now();
                }
            }
        }
        if let Err(e) = flush(&mut batch) {
            save_error = Some(e);
        }
    });
    if let Some(e) = save_error {
        return Err(e.into());
    }
    summary.analysed = saved_ids.len();
    for (i, (_, is_new)) in to_analyse.iter().enumerate() {
        if saved_ids.contains_key(&i) {
            if *is_new { summary.added += 1 } else { summary.changed += 1 }
        }
    }
    if cancel.load(Ordering::SeqCst) {
        return Ok(summary);
    }

    // 3. Files that are gone; a new file with the same name and size is taken to be the same file,
    //    moved, and inherits its tags and favourite flag.
    let mut new_by_key: HashMap<(String, i64), Option<i64>> = HashMap::new();
    for (i, (file, is_new)) in to_analyse.iter().enumerate() {
        if let (true, Some(&id)) = (*is_new, saved_ids.get(&i)) {
            let key = (file_name(&file.path), file.size);
            // Two candidates: ambiguous, carry nothing over.
            new_by_key.entry(key).and_modify(|v| *v = None).or_insert(Some(id));
        }
    }
    let tx = conn.unchecked_transaction()?;
    if !removed.is_empty() {
        let mut sizes = tx.prepare("SELECT size FROM sounds WHERE id = ?1")?;
        for (id, path) in &removed {
            let size: i64 = sizes.query_row([id], |r| r.get(0))?;
            if let Some(Some(new_id)) = new_by_key.get(&(file_name(path), size)) {
                db::carry_over(&tx, *id, *new_id)?;
                summary.moved += 1;
                summary.added -= 1;
            }
        }
    }
    let ids: Vec<i64> = removed.iter().map(|(id, _)| *id).collect();
    db::delete_sounds(&tx, &ids)?;
    tx.commit()?;
    summary.removed = removed.len() - summary.moved;
    Ok(summary)
}

fn batch_progress_name(files: &[(FileEntry, bool)], i: usize) -> String {
    files.get(i).map(|(f, _)| format!("{}.{}", f.name, f.ext)).unwrap_or_default()
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Applies the tag rules again to every sound, without decoding anything.
pub fn retag(conn: &Connection, settings: &Settings, ids: Option<&[i64]>) -> AppResult<()> {
    let sources = db::tag_sources(conn, ids)?;
    let tx = conn.unchecked_transaction()?;
    for s in sources {
        let input = TagInput {
            name: &s.name,
            dir: &s.dir,
            duration: s.duration,
            attack: s.attack,
            loop_kind: s.analysis_loop.as_deref(),
        };
        let (tags, by_name) = tagging::auto_tags(&input, &settings.tag_rules, settings.folder_tags);
        db::set_auto_tags(&tx, s.id, &tags)?;
        db::set_loop_kind(&tx, s.id, loop_kind(s.analysis_loop.as_deref(), by_name))?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{sine, write_wav};

    struct Quiet;
    impl ScanListener for Quiet {
        fn progress(&self, _: ScanProgress) {}
        fn library_changed(&self) {}
    }

    fn run(conn: &Connection) -> Summary {
        scan(conn, &Scope::All, &AtomicBool::new(false), &Quiet).unwrap()
    }

    #[test]
    fn scans_incrementally_and_follows_moves() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Doors/Wood")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        let hit = sine(22050, 0.5, 200.0, |t| (-t * 10.0).exp());
        write_wav(&root.join("Doors/Wood/creak_01.wav"), 22050, 1, &hit, &[]);
        write_wav(&root.join("ambience_loop.wav"), 22050, 1, &hit, &[]);
        write_wav(&root.join(".hidden/secret.wav"), 22050, 1, &hit, &[]);
        std::fs::write(root.join("notes.txt"), "not a sound").unwrap();
        std::fs::write(root.join("broken.mp3"), "not a sound").unwrap();

        let conn = db::open(&root.join("test.db")).unwrap();
        db::add_folder(&conn, &root.to_string_lossy()).unwrap();
        let first = run(&conn);
        assert_eq!((first.added, first.failed), (3, 1));

        let sounds = db::list_sounds(&conn).unwrap();
        let creak = sounds.iter().find(|s| s.name == "creak_01").unwrap();
        assert_eq!(creak.dir, "Doors/Wood");
        assert_eq!(creak.auto_tags, ["door", "one-shot", "wood"]);
        let amb = sounds.iter().find(|s| s.name == "ambience_loop").unwrap();
        assert_eq!(amb.loop_kind.as_deref(), Some("name"));
        assert!(amb.auto_tags.contains(&"loop".to_string()));
        assert!(sounds.iter().any(|s| s.name == "broken" && s.error.is_some()));

        // Nothing changed: nothing is analysed.
        assert_eq!(run(&conn), Summary { failed: 0, ..Default::default() });

        // A moved file keeps the user's tags and favourite flag.
        db::add_tag(&conn, &[creak.id], "spooky").unwrap();
        db::set_favorite(&conn, &[creak.id], true).unwrap();
        std::fs::rename(root.join("Doors/Wood/creak_01.wav"), root.join("creak_01.wav")).unwrap();
        let moved = run(&conn);
        assert_eq!((moved.moved, moved.added, moved.removed), (1, 0, 0));
        let sounds = db::list_sounds(&conn).unwrap();
        let creak = sounds.iter().find(|s| s.name == "creak_01").unwrap();
        assert_eq!(creak.dir, "");
        assert!(creak.favorite);
        assert_eq!(creak.user_tags, ["spooky"]);

        std::fs::remove_file(root.join("creak_01.wav")).unwrap();
        assert_eq!(run(&conn).removed, 1);
    }

    #[test]
    fn missing_folders_keep_their_sounds() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib");
        std::fs::create_dir(&lib).unwrap();
        write_wav(&lib.join("a.wav"), 8000, 1, &sine(8000, 0.2, 100.0, |_| 0.5), &[]);
        let conn = db::open(&dir.path().join("test.db")).unwrap();
        db::add_folder(&conn, &lib.to_string_lossy()).unwrap();
        run(&conn);
        std::fs::rename(&lib, dir.path().join("elsewhere")).unwrap();
        assert_eq!(run(&conn).missing_folders, 1);
        assert_eq!(db::list_sounds(&conn).unwrap().len(), 1);
        assert!(!db::list_folders(&conn).unwrap()[0].available);
    }
}

/// Scans a real library: `SOUNDHIVE_BENCH=/path cargo test --release bench -- --ignored --nocapture`
#[cfg(test)]
mod bench {
    use super::*;

    struct Print;
    impl ScanListener for Print {
        fn progress(&self, _: ScanProgress) {}
        fn library_changed(&self) {}
    }

    #[test]
    #[ignore]
    fn bench_scan() {
        let Ok(path) = std::env::var("SOUNDHIVE_BENCH") else { return };
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open(&dir.path().join("bench.db")).unwrap();
        db::add_folder(&conn, &path).unwrap();
        for run in ["first", "again"] {
            let t = Instant::now();
            let summary = scan(&conn, &Scope::All, &AtomicBool::new(false), &Print).unwrap();
            println!("{run} scan: {:.2?} — {summary}", t.elapsed());
        }
        let t = Instant::now();
        let sounds = db::list_sounds(&conn).unwrap();
        let json = serde_json::to_string(&sounds).unwrap();
        println!("list_sounds: {} sounds, {:.2?}, {} KB JSON", sounds.len(), t.elapsed(), json.len() / 1024);
        let mut tags = std::collections::BTreeMap::new();
        for s in &sounds {
            for t in &s.auto_tags {
                *tags.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let mut tags: Vec<_> = tags.into_iter().collect();
        tags.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!("top tags: {:?}", &tags[..tags.len().min(25)]);
        let loops: Vec<_> = sounds.iter().filter_map(|s| s.loop_kind.clone()).collect();
        println!(
            "loops: {} marked, {} name, {} seamless",
            loops.iter().filter(|k| *k == "marked").count(),
            loops.iter().filter(|k| *k == "name").count(),
            loops.iter().filter(|k| *k == "seamless").count()
        );
        for s in sounds.iter().filter(|s| s.error.is_some()).take(10) {
            println!("error: {} — {}", s.path, s.error.as_ref().unwrap());
        }
        println!("errors total: {}", sounds.iter().filter(|s| s.error.is_some()).count());
    }
}
