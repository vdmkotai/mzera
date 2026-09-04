//! One table of actions: keys, labels, mouse hints. Feeds shortcuts, menus and the help overlay.

pub const GALLERY: u8 = 1;
pub const VIEWER: u8 = 2;
pub const VIDEO: u8 = 4;
pub const ALL: u8 = GALLERY | VIEWER | VIDEO;
pub const IMG: u8 = VIEWER | VIDEO;

pub struct ActionSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub modes: u8,
    pub accels: &'static [&'static str],
    pub mouse: &'static str,
}

macro_rules! a {
    ($name:expr, $label:expr, $group:expr, $modes:expr, [$($k:expr),*], $mouse:expr) => {
        ActionSpec { name: $name, label: $label, group: $group, modes: $modes, accels: &[$($k),*], mouse: $mouse }
    };
}

pub const ACTIONS: &[ActionSpec] = &[
    // Navigate
    a!(
        "next",
        "Next",
        "Navigate",
        IMG,
        ["Right", "Down", "KP_Right"],
        "wheel · side button forward · ›"
    ),
    a!("space-next", "Next", "Navigate", VIEWER, ["space"], ""),
    a!(
        "prev",
        "Previous",
        "Navigate",
        IMG,
        ["Left", "Up", "<Shift>space", "KP_Left"],
        "wheel · side button back · ‹"
    ),
    a!("skip-fwd", "Skip +10", "Navigate", IMG, ["Page_Down"], ""),
    a!("skip-back", "Skip −10", "Navigate", IMG, ["Page_Up"], ""),
    a!("first", "First", "Navigate", IMG, ["Home"], ""),
    a!("last", "Last", "Navigate", IMG, ["End"], ""),
    a!(
        "open",
        "Open in viewer",
        "Navigate",
        GALLERY,
        ["g"],
        "double-click"
    ),
    a!(
        "back",
        "Back to gallery",
        "Navigate",
        IMG,
        ["Return", "KP_Enter", "g"],
        "middle click · Grid button"
    ),
    a!(
        "next-folder",
        "Next folder",
        "Navigate",
        ALL,
        ["bracketright", "<Ctrl>Right", "<Ctrl>Page_Down"],
        "Shift+wheel · side buttons (gallery)"
    ),
    a!(
        "prev-folder",
        "Previous folder",
        "Navigate",
        ALL,
        ["bracketleft", "<Ctrl>Left", "<Ctrl>Page_Up"],
        "Shift+wheel · side buttons (gallery)"
    ),
    a!(
        "parent",
        "Parent folder",
        "Navigate",
        GALLERY,
        ["BackSpace", "<Alt>Up"],
        "status bar root"
    ),
    a!(
        "pan-left",
        "Pan left",
        "Navigate",
        VIEWER,
        ["<Shift>Left"],
        "drag"
    ),
    a!(
        "pan-right",
        "Pan right",
        "Navigate",
        VIEWER,
        ["<Shift>Right"],
        "drag"
    ),
    a!(
        "pan-up",
        "Pan up",
        "Navigate",
        VIEWER,
        ["<Shift>Up"],
        "drag"
    ),
    a!(
        "pan-down",
        "Pan down",
        "Navigate",
        VIEWER,
        ["<Shift>Down"],
        "drag"
    ),
    // View
    a!(
        "scale-next",
        "Scale mode",
        "View",
        IMG,
        ["w"],
        "toolbar button"
    ),
    a!(
        "scale-prev",
        "Scale mode back",
        "View",
        IMG,
        ["<Shift>w"],
        ""
    ),
    a!(
        "zoom-in",
        "Zoom in",
        "View",
        VIEWER,
        ["plus", "equal", "KP_Add", "<Ctrl>plus", "<Ctrl>equal"],
        "Ctrl+wheel · pinch"
    ),
    a!(
        "zoom-out",
        "Zoom out",
        "View",
        VIEWER,
        ["minus", "KP_Subtract", "<Ctrl>minus"],
        "Ctrl+wheel · pinch"
    ),
    a!(
        "zoom-reset",
        "Reset zoom",
        "View",
        VIEWER,
        ["<Ctrl>0", "BackSpace"],
        "double-click"
    ),
    a!(
        "zoom-100",
        "Zoom 100 %",
        "View",
        VIEWER,
        ["<Ctrl>1"],
        "double-click"
    ),
    a!("zoom-200", "Zoom 200 %", "View", VIEWER, ["<Ctrl>2"], ""),
    a!(
        "rotate-cw",
        "Rotate clockwise",
        "View",
        VIEWER,
        ["r"],
        "menu"
    ),
    a!(
        "rotate-ccw",
        "Rotate counter-clockwise",
        "View",
        VIEWER,
        ["<Shift>r"],
        "menu"
    ),
    a!("antialias", "Antialiasing", "View", VIEWER, ["a"], ""),
    a!(
        "captions",
        "Captions: brief → metadata → clean",
        "View",
        ALL,
        ["t"],
        "toolbar button"
    ),
    a!(
        "metadata",
        "Metadata panel",
        "View",
        ALL,
        ["<Alt>Return"],
        "toolbar button"
    ),
    a!(
        "layout",
        "Flat ↔ by folder",
        "View",
        ALL,
        ["m"],
        "pill chips"
    ),
    a!(
        "thumb-bigger",
        "Bigger thumbnails",
        "View",
        GALLERY,
        ["plus", "equal", "KP_Add", "<Ctrl>plus", "<Ctrl>equal"],
        "Ctrl+wheel · pinch"
    ),
    a!(
        "thumb-smaller",
        "Smaller thumbnails",
        "View",
        GALLERY,
        ["minus", "KP_Subtract", "<Ctrl>minus"],
        "Ctrl+wheel · pinch"
    ),
    a!(
        "thumb-reset",
        "Reset thumbnail size",
        "View",
        GALLERY,
        ["<Ctrl>0"],
        "status bar px"
    ),
    a!(
        "jump-list",
        "Folder jump list",
        "View",
        GALLERY,
        ["j", "<Ctrl>j"],
        "status bar section"
    ),
    a!("fullscreen", "Fullscreen", "View", ALL, ["f"], "⋮ menu"),
    a!("slideshow", "Slideshow", "View", ALL, ["s"], "⋮ menu"),
    a!(
        "focus-next",
        "Focus next controls",
        "View",
        ALL,
        ["<Ctrl>Tab", "<Ctrl><Shift>Tab"],
        ""
    ),
    // Filters
    a!(
        "filter-next",
        "Next filter preset",
        "Filters",
        ALL,
        ["Tab"],
        "wheel on Filters chip"
    ),
    a!(
        "filter-prev",
        "Previous filter preset",
        "Filters",
        ALL,
        ["<Shift>Tab", "ISO_Left_Tab", "<Shift>ISO_Left_Tab"],
        "wheel on Filters chip"
    ),
    a!("filter-1", "All formats", "Filters", ALL, ["1", "KP_1"], ""),
    a!("filter-2", "Only JPEG", "Filters", ALL, ["2", "KP_2"], ""),
    a!("filter-3", "Only PNG", "Filters", ALL, ["3", "KP_3"], ""),
    a!("filter-4", "Only WebP", "Filters", ALL, ["4", "KP_4"], ""),
    a!("filter-5", "No WebP", "Filters", ALL, ["5", "KP_5"], ""),
    a!("filter-6", "Only GIF", "Filters", ALL, ["6", "KP_6"], ""),
    a!(
        "filter-7",
        "Only HEIC / AVIF / JXL",
        "Filters",
        ALL,
        ["7", "KP_7"],
        ""
    ),
    a!("filter-8", "Only video", "Filters", ALL, ["8", "KP_8"], ""),
    a!("filter-9", "No video", "Filters", ALL, ["9", "KP_9"], ""),
    a!(
        "filter-reset",
        "Reset filter",
        "Filters",
        ALL,
        ["0", "KP_0"],
        "middle click Filters chip"
    ),
    a!(
        "filters",
        "Filter popover",
        "Filters",
        ALL,
        ["i"],
        "Filters chip"
    ),
    // Gallery selection
    a!(
        "select-all",
        "Select all",
        "Gallery",
        GALLERY,
        ["<Ctrl>a"],
        "rubber-band drag"
    ),
    a!(
        "select-none",
        "Select none",
        "Gallery",
        GALLERY,
        ["<Ctrl><Shift>a"],
        "click empty space"
    ),
    // Files
    a!(
        "copy-path",
        "Copy path",
        "Files",
        ALL,
        ["y", "<Ctrl>c"],
        "context menu"
    ),
    a!(
        "open-folder",
        "Open folder in file manager",
        "Files",
        ALL,
        ["o"],
        "context menu · status bar root"
    ),
    a!(
        "open-with",
        "Open with default app",
        "Files",
        ALL,
        ["e"],
        "context menu"
    ),
    a!(
        "open-dir",
        "Open another folder…",
        "Files",
        ALL,
        ["<Ctrl>o"],
        "⋮ menu"
    ),
    a!(
        "rescan",
        "Rescan",
        "Files",
        ALL,
        ["<Ctrl>r"],
        "status bar counts"
    ),
    // Video
    a!(
        "play-pause",
        "Play / pause",
        "Video",
        VIDEO,
        ["space"],
        "click on video · ▶"
    ),
    a!(
        "seek-fwd",
        "Seek +5 s",
        "Video",
        VIDEO,
        ["<Shift>Right"],
        "drag the bar"
    ),
    a!(
        "seek-back",
        "Seek −5 s",
        "Video",
        VIDEO,
        ["<Shift>Left"],
        "drag the bar"
    ),
    a!(
        "seek-fwd-big",
        "Seek +30 s",
        "Video",
        VIDEO,
        ["<Ctrl><Shift>Right"],
        ""
    ),
    a!(
        "seek-back-big",
        "Seek −30 s",
        "Video",
        VIDEO,
        ["<Ctrl><Shift>Left"],
        ""
    ),
    a!(
        "seek-start",
        "Seek to start",
        "Video",
        VIDEO,
        ["<Shift>Home"],
        ""
    ),
    a!(
        "seek-end",
        "Seek to end",
        "Video",
        VIDEO,
        ["<Shift>End"],
        ""
    ),
    a!("frame-fwd", "Step forward", "Video", VIDEO, ["period"], ""),
    a!("frame-back", "Step back", "Video", VIDEO, ["comma"], ""),
    a!(
        "volume-up",
        "Volume up",
        "Video",
        VIDEO,
        ["<Shift>Up"],
        "wheel on speaker"
    ),
    a!(
        "volume-down",
        "Volume down",
        "Video",
        VIDEO,
        ["<Shift>Down"],
        "wheel on speaker"
    ),
    a!("mute", "Mute", "Video", VIDEO, ["v"], "speaker"),
    a!("loop", "Loop", "Video", VIDEO, ["l"], ""),
    // App
    a!(
        "help",
        "Help",
        "App",
        ALL,
        ["h", "question", "<Shift>question", "<Ctrl>question"],
        "? button"
    ),
    a!(
        "context-menu",
        "Context menu",
        "App",
        ALL,
        ["Menu", "<Ctrl>m"],
        "right click"
    ),
    a!("escape", "Back / close", "App", ALL, ["Escape"], ""),
    a!(
        "quit",
        "Quit",
        "App",
        ALL,
        ["q", "<Ctrl>q", "<Ctrl>w"],
        "⋮ menu"
    ),
];

/// Human label for an accelerator string ("<Ctrl>plus" → "Ctrl++").
pub fn accel_label(accel: &str) -> String {
    if let Some((key, mods)) = gtk::accelerator_parse(accel) {
        let s = gtk::accelerator_get_label(key, mods).to_string();
        return s;
    }
    accel.to_string()
}
