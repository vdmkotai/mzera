//! Recursive scanner: sorted DFS, no stat, streams batches to the UI thread.

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
}

/// Format groups used by the filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Group {
    Jpeg = 0,
    Png = 1,
    Webp = 2,
    Gif = 3,
    Heif = 4,
    Jxl = 5,
    OtherImage = 6,
    Video = 7,
}

pub const GROUPS: [Group; 8] = [
    Group::Jpeg,
    Group::Png,
    Group::Webp,
    Group::Gif,
    Group::Heif,
    Group::Jxl,
    Group::OtherImage,
    Group::Video,
];

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Jpeg => "JPEG",
            Group::Png => "PNG",
            Group::Webp => "WebP",
            Group::Gif => "GIF",
            Group::Heif => "HEIC / AVIF",
            Group::Jxl => "JXL",
            Group::OtherImage => "Other images",
            Group::Video => "Video",
        }
    }
    pub fn bit(self) -> u16 {
        1 << (self as u16)
    }
    pub fn kind(self) -> Kind {
        if self == Group::Video {
            Kind::Video
        } else {
            Kind::Image
        }
    }
}

#[cfg(test)]
pub fn classify(name: &str) -> Option<Group> {
    classify_bytes(name.as_bytes())
}

fn classify_os(name: &OsStr) -> Option<Group> {
    classify_bytes(name.as_bytes())
}

pub fn classify_path(path: &Path) -> Option<Group> {
    classify_os(path.file_name()?)
}

fn classify_bytes(name: &[u8]) -> Option<Group> {
    let dot = name.iter().rposition(|byte| *byte == b'.')?;
    let ext = &name[dot + 1..];
    let mut buf = [0u8; 8];
    if ext.len() > 8 {
        return None;
    }
    for (i, b) in ext.iter().enumerate() {
        buf[i] = b.to_ascii_lowercase();
    }
    let e = std::str::from_utf8(&buf[..ext.len()]).ok()?;
    Some(match e {
        "jpg" | "jpeg" | "jpe" | "jfif" => Group::Jpeg,
        "png" => Group::Png,
        "webp" => Group::Webp,
        "gif" => Group::Gif,
        "heic" | "heif" | "avif" => Group::Heif,
        "jxl" => Group::Jxl,
        "tif" | "tiff" | "bmp" | "svg" | "qoi" | "tga" | "exr" | "ico" | "pnm" | "ppm" | "pgm" => {
            Group::OtherImage
        }
        "webm" | "mp4" | "mkv" | "mov" | "m4v" => Group::Video,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    pub name: Box<str>,
    pub folder: u32,
    pub group: Group,
}

impl Entry {
    pub fn kind(&self) -> Kind {
        self.group.kind()
    }
}

#[derive(Clone, Debug)]
pub struct Folder {
    pub path: PathBuf,
    /// Display name relative to the root ("Day 2 / Museum"); empty for the root itself.
    pub rel: String,
}

pub enum Msg {
    Folder(Folder),
    Entries(Vec<Entry>),
    Warnings(u32),
    Done,
}

/// Natural, case-insensitive comparison: "1, 2, 10, a, b".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let originals = (a.as_bytes(), b.as_bytes());
    let (mut ai, mut bi) = (a.as_bytes(), b.as_bytes());
    loop {
        match (ai.first(), bi.first()) {
            (None, None) => return originals.0.cmp(originals.1),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na = ai.iter().take_while(|c| c.is_ascii_digit()).count();
                    let nb = bi.iter().take_while(|c| c.is_ascii_digit()).count();
                    let (da, db) = (&ai[..na], &bi[..nb]);
                    let ta = da
                        .iter()
                        .position(|&c| c != b'0')
                        .map(|p| &da[p..])
                        .unwrap_or(&[]);
                    let tb = db
                        .iter()
                        .position(|&c| c != b'0')
                        .map(|p| &db[p..])
                        .unwrap_or(&[]);
                    let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                    if o != Ordering::Equal {
                        return o;
                    }
                    ai = &ai[na..];
                    bi = &bi[nb..];
                } else {
                    let (la, lb) = (ca.to_ascii_lowercase(), cb.to_ascii_lowercase());
                    if la != lb {
                        return la.cmp(&lb);
                    }
                    ai = &ai[1..];
                    bi = &bi[1..];
                }
            }
        }
    }
}

const BATCH: usize = 512;

struct Walker {
    tx: async_channel::Sender<Msg>,
    batch: Vec<Entry>,
    next_folder: u32,
    root: PathBuf,
    warnings: u32,
}

impl Walker {
    fn flush(&mut self) -> bool {
        if self.batch.is_empty() {
            return true;
        }
        let b = std::mem::take(&mut self.batch);
        self.tx.send_blocking(Msg::Entries(b)).is_ok()
    }

    fn walk(&mut self, dir: &Path, depth: u32) -> bool {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(_) => {
                self.warnings = self.warnings.saturating_add(1);
                return true;
            }
        };
        let rel = if depth == 0 {
            String::new()
        } else {
            dir.strip_prefix(&self.root)
                .map(|p| p.to_string_lossy().replace('/', " / "))
                .unwrap_or_default()
        };
        let folder_id = self.next_folder;
        self.next_folder += 1;
        if self
            .tx
            .send_blocking(Msg::Folder(Folder {
                path: dir.to_path_buf(),
                rel,
            }))
            .is_err()
        {
            return false;
        }
        let mut files: Vec<(OsString, Group)> = Vec::new();
        let mut dirs: Vec<OsString> = Vec::new();
        for entry in rd {
            let e = match entry {
                Ok(e) => e,
                Err(_) => {
                    self.warnings = self.warnings.saturating_add(1);
                    continue;
                }
            };
            let name = e.file_name();
            if name.as_bytes().first() == Some(&b'.') {
                continue;
            }
            let ft = match e.file_type() {
                Ok(ft) => ft,
                Err(_) => {
                    self.warnings = self.warnings.saturating_add(1);
                    continue;
                }
            };
            if ft.is_dir() {
                dirs.push(name);
            } else if ft.is_file() {
                if let Some(g) = classify_os(&name) {
                    files.push((name, g));
                }
            } else if ft.is_symlink() {
                // follow symlinked files only (never directories: loop protection)
                if let Ok(md) = std::fs::metadata(e.path())
                    && md.is_file()
                    && let Some(g) = classify_os(&name)
                {
                    files.push((name, g));
                }
            }
        }
        let cmp_os = |a: &OsStr, b: &OsStr| {
            natural_cmp(&a.to_string_lossy(), &b.to_string_lossy())
                .then_with(|| a.as_bytes().cmp(b.as_bytes()))
        };
        files.sort_by(|a, b| cmp_os(&a.0, &b.0));
        dirs.sort_by(|a, b| cmp_os(a, b));
        for (name, group) in files {
            let display_name = name.to_string_lossy().into_owned();
            self.batch.push(Entry {
                path: dir.join(&name),
                name: display_name.into_boxed_str(),
                folder: folder_id,
                group,
            });
            if self.batch.len() >= BATCH && !self.flush() {
                return false;
            }
        }
        // Flush per folder so a folder's files land before its subfolders' headers.
        if !self.flush() {
            return false;
        }
        for d in dirs {
            if !self.walk(&dir.join(d), depth + 1) {
                return false;
            }
        }
        true
    }
}

pub fn spawn(root: PathBuf) -> async_channel::Receiver<Msg> {
    let (tx, rx) = async_channel::bounded(64);
    std::thread::Builder::new()
        .name("mzera-scan".into())
        .spawn(move || {
            let mut w = Walker {
                tx: tx.clone(),
                batch: Vec::new(),
                next_folder: 0,
                root: root.clone(),
                warnings: 0,
            };
            if w.walk(&root, 0) {
                w.flush();
                if w.warnings > 0 {
                    let _ = tx.send_blocking(Msg::Warnings(w.warnings));
                }
                let _ = tx.send_blocking(Msg::Done);
            }
        })
        .expect("spawn scanner");
    rx
}

/// Blocking scan (used by --warm).
pub fn scan_blocking(root: &Path) -> Vec<Entry> {
    let rx = spawn(root.to_path_buf());
    let mut out = Vec::new();
    while let Ok(m) = rx.recv_blocking() {
        match m {
            Msg::Entries(v) => out.extend(v),
            Msg::Done => break,
            Msg::Folder(_) | Msg::Warnings(_) => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn classification_is_case_insensitive_and_rejects_unknown_files() {
        assert_eq!(classify("PHOTO.JPEG"), Some(Group::Jpeg));
        assert_eq!(classify("clip.WeBm"), Some(Group::Video));
        assert_eq!(classify("archive.tar.gz"), None);
        assert_eq!(classify("no-extension"), None);
    }

    #[test]
    fn natural_order_is_numeric_and_total() {
        let mut names = vec!["10.webp", "2.webp", "01.webp", "1.webp", "A.webp", "a.webp"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            ["01.webp", "1.webp", "2.webp", "10.webp", "A.webp", "a.webp"]
        );
        for pair in names.windows(2) {
            assert_ne!(natural_cmp(pair[0], pair[1]), Ordering::Equal);
        }
    }

    #[test]
    fn blocking_scan_is_sorted_dfs_and_skips_hidden_files() {
        let root = tempfile::tempdir().expect("temporary directory");
        std::fs::create_dir(root.path().join("sub2")).expect("sub2");
        std::fs::create_dir(root.path().join("sub10")).expect("sub10");
        for relative in [
            "10.jpg",
            "2.png",
            ".hidden.webp",
            "sub2/3.webp",
            "sub10/1.gif",
            "notes.txt",
        ] {
            std::fs::write(root.path().join(relative), b"test").expect("test file");
        }

        let entries = scan_blocking(root.path());
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_ref()).collect();
        assert_eq!(names, ["2.png", "10.jpg", "3.webp", "1.gif"]);
        assert_eq!(
            entries.iter().map(|entry| entry.folder).collect::<Vec<_>>(),
            [0, 0, 1, 2]
        );
    }

    #[test]
    fn blocking_scan_keeps_non_utf8_names_with_ascii_extensions() {
        let root = tempfile::tempdir().expect("temporary directory");
        let name = OsString::from_vec(b"image-\xff.jpg".to_vec());
        std::fs::write(root.path().join(&name), b"test").expect("non-UTF-8 test file");

        let entries = scan_blocking(root.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path.file_name(), Some(name.as_os_str()));
        assert_eq!(entries[0].group, Group::Jpeg);
    }
}
