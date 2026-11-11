//! Getting sounds out of the app: file clipboard, drag and drop, external editor, copying into
//! a project folder, and the file manager.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext};

use crate::error::{AppError, AppResult, invalid};

fn io_error(e: impl std::fmt::Display) -> AppError {
    AppError::Io { message: e.to_string() }
}

/// `file://` URI with everything but unreserved characters percent-encoded.
pub fn file_uri(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut out = String::from("file://");
    if !path.starts_with('/') {
        out.push('/'); // Windows drive letter
    }
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Keeps the clipboard context alive: on X11 the app has to serve the clipboard itself.
pub struct FileClipboard(Mutex<Option<ClipboardContext>>);

impl FileClipboard {
    pub fn new() -> Self {
        FileClipboard(Mutex::new(None))
    }

    /// Puts files on the clipboard so that file managers, DAWs and editors can paste them.
    pub fn copy_files(&self, paths: &[String]) -> AppResult<()> {
        let mut guard = self.0.lock().unwrap();
        if guard.is_none() {
            *guard = Some(ClipboardContext::new().map_err(|e| io_error(format!("No clipboard: {e}")))?);
        }
        let ctx = guard.as_ref().unwrap();
        let contents = if cfg!(target_os = "linux") {
            let uris: Vec<String> = paths.iter().map(|p| file_uri(p)).collect();
            // GNOME's file manager wants its own format; everything else takes the URI list.
            let gnome = format!("copy\n{}", uris.join("\n"));
            vec![
                ClipboardContent::Files(uris),
                ClipboardContent::Other("x-special/gnome-copied-files".into(), gnome.into_bytes()),
                ClipboardContent::Text(paths.join("\n")),
            ]
        } else {
            vec![ClipboardContent::Files(paths.to_vec())]
        };
        ctx.set(contents).map_err(|e| io_error(format!("Could not copy to the clipboard: {e}")))
    }
}

/// Splits the editor setting into program and arguments. A path to an existing file is taken
/// whole (it may contain spaces); otherwise the setting is split at whitespace, so that e.g.
/// `flatpak run org.audacityteam.Audacity` works.
fn editor_command(setting: &str) -> Option<(PathBuf, Vec<String>)> {
    let setting = setting.trim().trim_matches('"');
    if setting.is_empty() {
        return find_audacity().map(|p| (p, Vec::new()));
    }
    if Path::new(setting).is_file() {
        return Some((PathBuf::from(setting), Vec::new()));
    }
    let mut parts = setting.split_whitespace().map(String::from);
    let program = parts.next()?;
    Some((PathBuf::from(program), parts.collect()))
}

fn find_audacity() -> Option<PathBuf> {
    if let Ok(path) = which::which("audacity") {
        return Some(path);
    }
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(dir) = std::env::var_os(var) {
                let exe = Path::new(&dir).join("Audacity").join("Audacity.exe");
                if exe.is_file() {
                    return Some(exe);
                }
            }
        }
    }
    if cfg!(target_os = "macos") {
        let app = Path::new("/Applications/Audacity.app/Contents/MacOS/Audacity");
        if app.is_file() {
            return Some(app.to_path_buf());
        }
    }
    None
}

/// Name of the editor that "Open in editor" starts, for the button's tooltip.
pub fn editor_name(setting: &str) -> Option<String> {
    editor_command(setting).map(|(p, _)| {
        p.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned())
    })
}

pub fn open_in_editor(setting: &str, paths: &[String]) -> AppResult<()> {
    let (program, args) = editor_command(setting)
        .ok_or_else(|| invalid("Audacity was not found. Set the editor to use in Settings."))?;
    std::process::Command::new(&program)
        .args(args)
        .args(paths)
        .spawn()
        .map_err(|e| io_error(format!("Could not start {}: {e}", program.display())))?;
    Ok(())
}

/// Copies files into `dest`, adding " (2)" etc. instead of overwriting; returns the new paths.
pub fn copy_to_folder(paths: &[String], dest: &str) -> AppResult<Vec<String>> {
    let dest = Path::new(dest);
    if !dest.is_dir() {
        return Err(invalid(format!("Folder not found: {}", dest.display())));
    }
    let mut out = Vec::new();
    for path in paths {
        let src = Path::new(path);
        let name = src.file_name().ok_or_else(|| invalid("Not a file"))?;
        let mut target = dest.join(name);
        let stem = src.file_stem().unwrap_or_default().to_string_lossy();
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let mut n = 2;
        while target.exists() {
            target = dest.join(format!("{stem} ({n}){ext}"));
            n += 1;
        }
        std::fs::copy(src, &target).map_err(|e| io_error(format!("Could not copy {path}: {e}")))?;
        out.push(target.to_string_lossy().into_owned());
    }
    Ok(out)
}

pub fn reveal(path: &str) -> AppResult<()> {
    if !Path::new(path).exists() {
        return Err(invalid(format!("File not found: {path}")));
    }
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| io_error(format!("Could not show {path}: {e}")))
}

pub fn open_folder(path: &str) -> AppResult<()> {
    if !Path::new(path).is_dir() {
        return Err(invalid(format!("Folder not found: {path}")));
    }
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|e| io_error(format!("Could not open {path}: {e}")))
}

/// Opens a sound in the system's default app for its type.
pub fn open_default(path: &str) -> AppResult<()> {
    if !crate::analysis::is_sound_file(Path::new(path)) || !Path::new(path).is_file() {
        return Err(invalid(format!("Not a sound file: {path}")));
    }
    tauri_plugin_opener::open_path(path, None::<&str>)
        .map_err(|e| io_error(format!("Could not open {path}: {e}")))
}

/// Starts a native drag of files out of the window, into a DAW, an editor, a game engine or the
/// file manager. Must run on the main thread.
pub fn start_drag(window: &tauri::WebviewWindow, paths: Vec<PathBuf>) -> AppResult<()> {
    const ICON: &[u8] = include_bytes!("../icons/64x64.png");
    #[cfg(target_os = "linux")]
    let target = window.gtk_window().map_err(io_error)?;
    #[cfg(not(target_os = "linux"))]
    let target = window.clone();
    drag::start_drag(
        &target,
        drag::DragItem::Files(paths),
        drag::Image::Raw(ICON.to_vec()),
        |_, _| {},
        drag::Options::default(),
    )
    .map_err(|e| io_error(format!("Could not start dragging: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uris_are_encoded() {
        assert_eq!(file_uri("/home/me/My Sounds/boom#1.wav"), "file:///home/me/My%20Sounds/boom%231.wav");
        assert_eq!(file_uri(r"C:\SFX\ä.wav"), "file:///C:/SFX/%C3%A4.wav");
    }

    #[test]
    fn copies_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.wav");
        std::fs::write(&src, "x").unwrap();
        let dest = dir.path().join("out");
        std::fs::create_dir(&dest).unwrap();
        let src = src.to_string_lossy().into_owned();
        let dest_s = dest.to_string_lossy().into_owned();
        copy_to_folder(std::slice::from_ref(&src), &dest_s).unwrap();
        let second = copy_to_folder(&[src], &dest_s).unwrap();
        assert!(second[0].ends_with("a (2).wav"));
    }

    #[test]
    fn editor_setting_with_arguments() {
        let (program, args) = editor_command("flatpak run org.audacityteam.Audacity").unwrap();
        assert_eq!(program, PathBuf::from("flatpak"));
        assert_eq!(args, ["run", "org.audacityteam.Audacity"]);
    }
}
