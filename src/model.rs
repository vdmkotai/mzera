//! The list model: plain vectors, section starts, filtered index, selection.

use crate::scan::{Entry, Folder, GROUPS, Group};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    /// bitmask of included groups
    pub groups: u16,
    /// only entries directly in the root folder
    pub root_only: bool,
    /// only entries in this folder id
    pub folder_only: Option<u32>,
}

pub const ALL_GROUPS: u16 = 0xff;

impl Default for Filter {
    fn default() -> Self {
        Filter {
            groups: ALL_GROUPS,
            root_only: false,
            folder_only: None,
        }
    }
}

impl Filter {
    pub fn accepts(&self, e: &Entry) -> bool {
        if self.groups & e.group.bit() == 0 {
            return false;
        }
        if self.root_only && e.folder != 0 {
            return false;
        }
        if let Some(f) = self.folder_only
            && e.folder != f
        {
            return false;
        }
        true
    }

    pub fn describe(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.groups == ALL_GROUPS {
            parts.push("All formats".into());
        } else {
            let inc: Vec<&str> = GROUPS
                .iter()
                .filter(|g| self.groups & g.bit() != 0)
                .map(|g| g.label())
                .collect();
            let exc: Vec<&str> = GROUPS
                .iter()
                .filter(|g| self.groups & g.bit() == 0)
                .map(|g| g.label())
                .collect();
            if inc.len() <= exc.len() {
                parts.push(format!("Only {}", inc.join(", ")));
            } else {
                parts.push(format!("No {}", exc.join(", ")));
            }
        }
        if self.root_only {
            parts.push("root folder only".into());
        }
        if self.folder_only.is_some() {
            parts.push("one folder".into());
        }
        parts.join(" · ")
    }
}

/// Filter presets on keys 1–9.
pub fn preset(n: u8) -> Option<(&'static str, u16)> {
    let g = |x: Group| x.bit();
    Some(match n {
        1 => ("All formats", ALL_GROUPS),
        2 => ("Only JPEG", g(Group::Jpeg)),
        3 => ("Only PNG", g(Group::Png)),
        4 => ("Only WebP", g(Group::Webp)),
        5 => ("No WebP", ALL_GROUPS & !g(Group::Webp)),
        6 => ("Only GIF", g(Group::Gif)),
        7 => ("Only HEIC / AVIF / JXL", g(Group::Heif) | g(Group::Jxl)),
        8 => ("Only video", g(Group::Video)),
        9 => ("No video", ALL_GROUPS & !g(Group::Video)),
        _ => return None,
    })
}

pub const N_PRESETS: u8 = 9;

#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub folder: u32,
    /// start position in `visible`
    pub start: u32,
    pub len: u32,
}

pub struct Model {
    pub root: PathBuf,
    pub root_name: String,
    pub entries: Vec<Entry>,
    pub folders: Vec<Folder>,
    pub visible: Vec<u32>,
    pub sections: Vec<Section>,
    pub filter: Filter,
    pub counts: [u32; 8],
    pub scanning: bool,
    /// selection over master indices
    pub selected: HashSet<u32>,
    pub generation: u64,
}

impl Model {
    pub fn empty() -> Model {
        Model {
            root: PathBuf::new(),
            root_name: String::new(),
            entries: Vec::new(),
            folders: Vec::new(),
            visible: Vec::new(),
            sections: Vec::new(),
            filter: Filter::default(),
            counts: [0; 8],
            scanning: false,
            selected: HashSet::new(),
            generation: 0,
        }
    }

    pub fn reset(&mut self, root: PathBuf) {
        self.root_name = root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| root.to_string_lossy().to_string());
        self.root = root;
        self.entries.clear();
        self.folders.clear();
        self.visible.clear();
        self.sections.clear();
        self.counts = [0; 8];
        self.selected.clear();
        self.scanning = true;
        self.generation += 1;
        self.filter.folder_only = None;
    }

    pub fn add_folder(&mut self, f: Folder) {
        self.folders.push(f);
    }

    /// Append a batch (already in master order) and extend the visible index incrementally.
    pub fn append(&mut self, batch: Vec<Entry>) {
        let base = self.entries.len() as u32;
        self.entries.reserve(batch.len());
        for (i, e) in batch.into_iter().enumerate() {
            self.counts[e.group as usize] += 1;
            let m = base + i as u32;
            let accept = self.filter.accepts(&e);
            let folder = e.folder;
            self.entries.push(e);
            if accept {
                self.push_visible(m, folder);
            }
        }
    }

    fn push_visible(&mut self, m: u32, folder: u32) {
        let pos = self.visible.len() as u32;
        self.visible.push(m);
        match self.sections.last_mut() {
            Some(s) if s.folder == folder => s.len += 1,
            _ => self.sections.push(Section {
                folder,
                start: pos,
                len: 1,
            }),
        }
    }

    pub fn rebuild_visible(&mut self) {
        self.visible.clear();
        self.sections.clear();
        for m in 0..self.entries.len() as u32 {
            let e = &self.entries[m as usize];
            if self.filter.accepts(e) {
                let folder = e.folder;
                self.push_visible(m, folder);
            }
        }
        let filter = self.filter;
        self.selected.retain(|&master| {
            self.entries
                .get(master as usize)
                .is_some_and(|entry| filter.accepts(entry))
        });
    }

    pub fn set_filter(&mut self, f: Filter) {
        if f != self.filter {
            self.filter = f;
            self.rebuild_visible();
        }
    }

    pub fn entry_at(&self, pos: u32) -> &Entry {
        &self.entries[self.visible[pos as usize] as usize]
    }

    pub fn master(&self, pos: u32) -> u32 {
        self.visible[pos as usize]
    }

    pub fn n_visible(&self) -> u32 {
        self.visible.len() as u32
    }

    /// Visible position of a master index (visible is strictly increasing).
    pub fn pos_of_master(&self, m: u32) -> Option<u32> {
        self.visible.binary_search(&m).ok().map(|p| p as u32)
    }

    /// Nearest visible position for a master index that may be hidden.
    pub fn nearest_pos(&self, m: u32) -> Option<u32> {
        if self.visible.is_empty() {
            return None;
        }
        match self.visible.binary_search(&m) {
            Ok(p) => Some(p as u32),
            Err(p) => Some((p.min(self.visible.len() - 1)) as u32),
        }
    }

    /// Section index containing visible position `pos`.
    pub fn section_of(&self, pos: u32) -> usize {
        match self.sections.binary_search_by(|s| s.start.cmp(&pos)) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    pub fn folder_of_pos(&self, pos: u32) -> &Folder {
        &self.folders[self.entry_at(pos).folder as usize]
    }

    pub fn folder_display(&self, id: u32) -> &str {
        let f = &self.folders[id as usize];
        if f.rel.is_empty() {
            &self.root_name
        } else {
            &f.rel
        }
    }

    pub fn n_folders_visible(&self) -> usize {
        self.sections.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Entry, Group};

    fn entry(name: &str, folder: u32, group: Group) -> Entry {
        Entry {
            path: PathBuf::from(name),
            name: name.into(),
            folder,
            group,
        }
    }

    #[test]
    fn filtering_rebuilds_sections_and_selection() {
        let mut model = Model::empty();
        model.folders = vec![
            Folder {
                path: PathBuf::from("root"),
                rel: String::new(),
            },
            Folder {
                path: PathBuf::from("root/sub"),
                rel: "sub".into(),
            },
        ];
        model.append(vec![
            entry("a.jpg", 0, Group::Jpeg),
            entry("b.png", 0, Group::Png),
            entry("c.jpg", 1, Group::Jpeg),
        ]);
        model.selected.extend([0, 1, 2]);

        model.set_filter(Filter {
            groups: Group::Jpeg.bit(),
            root_only: false,
            folder_only: None,
        });

        assert_eq!(model.visible, [0, 2]);
        assert_eq!(model.sections.len(), 2);
        assert_eq!((model.sections[0].start, model.sections[0].len), (0, 1));
        assert_eq!((model.sections[1].start, model.sections[1].len), (1, 1));
        assert_eq!(model.selected, HashSet::from([0, 2]));
    }

    #[test]
    fn nearest_visible_position_handles_hidden_items_and_edges() {
        let mut model = Model::empty();
        model.visible = vec![2, 5, 9];
        assert_eq!(model.nearest_pos(0), Some(0));
        assert_eq!(model.nearest_pos(4), Some(1));
        assert_eq!(model.nearest_pos(9), Some(2));
        assert_eq!(model.nearest_pos(99), Some(2));
    }
}
