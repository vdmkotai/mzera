# Mzera — plan (v2, Rust, verified)

Companion to `explainer.html`. Evidence for every number and every correction is in `verification/`.

Status 2026-09-04: phases 0–6 are implemented. The toolchain, design, input
model, release build, real-data performance gates, and WebM hardware decoding
have been verified; see `verification/implementation.md`.

## 1. Requirements

Must have (v1)
- Open a folder recursively; first tiles visible immediately even for ~35 000 files.
- Gallery: flat grid and grouped by folder (sticky headers + folder jump list), thumbnail zoom.
- Filters by format: include / exclude JPEG, PNG, WebP, GIF, HEIC/AVIF, JXL, video. Presets on 1–9.
- Fullscreen viewer: arrows navigate, scale modes, pan/zoom, instant next/prev.
- Captions: brief / metadata panel / clean. Help overlay generated from the action table.
- WebM video: tile with duration, inline playback with a minimal control pill.
- **Full keyboard navigation and full mouse navigation, both complete on their own** (see §8, `verification/input.md`).
- Follows the Omarchy theme live (colors, dark/light, font).
- Launch paths kept: `Super+Alt+P`, launcher, Nautilus, CLI. English-only UI. No F-keys. No Super combos.

Nice to have (v2+): slideshow, rotate, trash with undo, open-with, multi-select ops, sort by date/size,
name search, per-folder state, hover-scrub video tiles, flip.

Not planned: editing, tags, ratings, library database, cloud.

## 2. Decision: Rust

| | Python + PyGObject | Rust + gtk4-rs (chosen) |
|---|---|---|
| startup to first frame | 300–500 ms | **160–180 ms measured** on a 35 k-item skeleton, target < 120 ms |
| model ops on 35 k items (filter, sort) | 50 ms – 1 s, needs incremental tricks | 2–5 ms |
| memory for the model | 100–150 MB | ~10 MB |
| custom widgets that draw textures directly | possible but every snapshot() call crosses into Python | native |
| GC pauses while scrolling | yes | none |
| decode / thumbnails / GPU / video | same native libraries | same |
| toolchain | installed | rustup installed (user space, `~/.cargo/bin`) |
| build | n/a | clean 49 s, incremental 3 s, 160 crates |

Verified stack: `gtk4 0.11 (v4_22)`, `libadwaita 0.9 (v1_9)`, `glycin 3.1 (gdk4)`, `libvips` crate (in-process,
works with libvips 8.18), `gstreamer 0.25` only if needed beyond `gtk::MediaFile`, `kamadak-exif`, `notify`, `toml`, `async-channel`.

## 3. What verification changed in the plan

1. **GtkGridView has no sections/header factory in GTK 4.22** (only ListView). Grouped-by-folder with sticky
   headers is impossible with GridView → **custom `GalleryView` widget** (gtk::Widget + Scrollable) that lays out
   fixed-size tiles itself and draws textures and sticky headers in `snapshot()`. Zero child widgets. This is also
   the fastest option: bind cost 0, exact visibility priority, one render pass of 200–400 texture nodes.
2. **glycin does not downscale** raster images, and without a `Pool` it spawns a sandbox per decode. Thumbnails
   therefore use **libvips in-process** (`thumbnail` = shrink-on-load: 12 ms per 1080p WebP vs 32 ms pure-Rust,
   vs 56 ms via CLI). glycin (with a shared Pool) is the viewer's decoder and the fallback for HEIC/AVIF/JXL/SVG/TIFF.
3. **gdk-pixbuf has no native loaders on this system** (delegates to sandboxed glycin, 6.4 ms per tile, does not
   parallelise). Tile reads go through `gdk::Texture::from_filename` (built-in PNG/JPEG, 0.46 ms) or an in-process
   PNG decode → `MemoryTexture` (0.27 ms). Never gdk-pixbuf, never glycin for tiles.
4. **GTK list models are not on the hot path.** SortListModel restarts on every insert during a streaming scan;
   FilterListModel's incremental mode adds ~1 s of churn. The scanner emits entries already in sorted DFS order;
   the model is a `Vec<Entry>` + section-start array + filtered index (`Vec<u32>`, rebuilt in ~2 ms).
5. **No stat during the scan**: `read_dir` + `d_type` only; size/mtime are read lazily by the thumbnail worker
   (fd.o mtime check) and by the metadata caption.
6. **Textures are throttled per frame**: at most ~32 new tile textures assigned per frame (a full new screen is
   ~40 MB of upload), nearest-to-viewport first; no decode requests while scroll velocity is high.
7. Video packages: `gst-plugins-good`, `gst-plugin-va`, `gst-plugin-dav1d` (AV1 is *not* in gst-plugins-bad on
   Arch), optional `gst-libav`. GTK's built-in GStreamer backend supports DMABuf zero-copy; verify with
   `GST_DEBUG=gtkgstsink:4`.
8. Test set corrected: `~/Downloads/University_of_Problems-1.9.0-Extended-pc` — 34 988 images (32 779 WebP, all
   1920×1080, none animated; 1 261 PNG; 948 JPEG) + 432 WebM.

## 4. Architecture

```
mzera [--gallery|--viewer] [path…] | --warm <dir>          (single instance; one root shown at a time)
  │
  ├─ app.rs        adw::Application, CLI, single-instance, opens window BEFORE the scan starts
  ├─ theme.rs      colors.toml → CSS → one CssProvider; notify/FileMonitor live reload; Adw color scheme
  ├─ scan.rs       thread: sorted DFS (natural sort per dir, dirs first), read_dir + d_type only,
  │                batches of 512 over async_channel → main context at Priority::DEFAULT
  ├─ model.rs      Vec<Entry{path, folder_id, kind, ext}> · sections: Vec<u32> starts ·
  │                filtered index Vec<u32> · selection bitset + current + anchor
  ├─ thumbs.rs     ThumbService: fd.o cache (~/.cache/thumbnails/large, PNG level 1, Thumb::URI/MTime) ·
  │                pool of N threads (8–12 in-app, 16 for --warm) · priority queue keyed by distance from the
  │                viewport centre, LIFO within equal distance, generation counter for cancellation ·
  │                decoders: libvips thumbnail (images) or ffmpegthumbnailer (video), both off the UI thread
  │                for video · results: async_channel → tick-callback drain, ≤ 32 textures per frame
  ├─ gallery.rs    GalleryView: gtk::Widget + Scrollable · fixed square tiles · flat or sectioned layout with
  │                prefix sums · sticky headers + rubber-band + hover/selection rings drawn in snapshot() ·
  │                jump list (ListView, sections) · keyboard + mouse handling per input.md
  ├─ viewer.rs     ImageView: gtk::Widget · snapshot() = append_scaled_texture under zoom/pan transform ·
  │                trilinear < 1, linear ≥ 1 · GestureZoom/Drag/Scroll mutate (scale, offset) only ·
  │                glycin Loader with shared Pool, accepted formats R8g8b8(a8) · preload ring ±2 with Cancellables ·
  │                bytes-capped LRU (≈1 GB) · turbojpeg DCT scaling for huge JPEG (v2) · tiling > 16 384 px (v2)
  ├─ video.rs      gtk::Video + gtk::MediaFile (built-in GStreamer, dmabuf) · autoplay muted loop · control pill
  ├─ overlays.rs   floating toolbar (auto-hide), captions (brief / metadata panel / clean), help, toasts
  ├─ actions.rs    ONE table: name, label, group, modes, accels, mouse hint → SimpleActions + accels +
  │                menus + tooltips + help overlay
  └─ filters.rs    format chips with counts, presets 1–9, folder-only filter, name search (v2)
```

Threads: main (GTK + glycin futures) · scanner (1) · thumbnail pool (N) · glycin Pool (≤ 4 loader processes) ·
GStreamer's own. Nothing at idle priority on the hot path.

## 5. Performance budgets (gates)

Measured values come from `verification/`. A phase is not done until its gate holds on the test set.

| Metric | Budget | Measured / basis |
|---|---|---|
| app start → first frame (empty window) | < 120 ms | skeleton with 35 k items: 160–180 ms incl. model fill; window shown before scan |
| full scan, 35 k entries, warm cache | < 300 ms | sorted DFS without stat: 50–150 ms |
| open folder → first tiles drawn | < 200 ms | first 512-entry batch arrives 10–20 ms after the window |
| scroll, warm cache | 60 fps, 0 decode on UI thread | tile read 0.27–0.46 ms on a worker; 48-tile screen ≈ 13–22 ms of worker time |
| scroll, cold cache | visible first, no jank | priority queue by distance, cancel on scroll-away, ≤ 32 textures/frame |
| thumbnail decode, 1080p WebP → 256 px | ≤ 15 ms per file per thread | libvips in-process 12.3 ms; 451 files/s on 16 threads |
| cold build of the full set (`--warm`) | < 2 min | projected 78 s warm / 84 s cold + 3 s for 432 videos |
| viewer next / prev | < 16 ms perceived (one frame) | preload ring ±2, texture already resident; upload of a 1080p RGBA frame ≈ 8 MB |
| filter toggle on 35 k | < 10 ms | filtered index rebuild ≈ 2 ms; no incremental FilterListModel |
| memory | model ≈ 10 MB · tile textures ≤ 512 MB · viewer LRU ≤ 1 GB | 175–262 KB per tile texture; 1 500–2 500 resident, pinned visible ± 2 screens |

## 6. Thumbnail cache

Freedesktop layout (shared with Nautilus): `~/.cache/thumbnails/large/<md5 of file URI>.png` (256 px),
`x-large` (512 px) for tile sizes > 256. PNG compression level 1, `Thumb::URI`, `Thumb::MTime`, `Thumb::Size`.
Validation = one `statx` in the worker. Failed decodes get a `fail/mzera/` marker per spec so they are not retried.
If profiling ever shows PNG decode limiting cold-box scroll, a private JPEG Q85 side cache (0.10 ms, 11 KB) is
the fallback; WebP is never used as cache format on GTK (6 ms reads).

## 7. Theme sync with Omarchy

- Source: `~/.local/state/omarchy/current/theme/colors.toml` (always present). Map: background → window and
  popovers; dark_background → gallery; lighter_background → tiles, chips, section lines; accent → selection
  ring, active chips, progress; foreground / muted → text; red / green / yellow → errors, video badge, warnings;
  `mode` → `adw::StyleManager` color scheme.
- Live reload: `notify` (inotify) on the file and on `theme.name`; regenerate the ~50-line CSS and swap the
  provider. No restart.
- Optional override: `~/.config/omarchy/themed/mzera.css.tpl`, rendered by Omarchy into
  `current/theme/mzera.css`; if present it wins over the generated CSS.
- Font: monospace overlays use `omarchy font current` (JetBrainsMono Nerd Font); UI font from GTK settings.
- Viewer background stays black in clean mode (configurable).

## 8. Input (summary; complete tables in `verification/input.md`)

Principles: swayimg habits first, GNOME HIG combos as aliases, every action reachable by keyboard and by mouse,
no F-keys, no Super, no Ctrl+Space (layout toggle), no media keys (compositor-owned).

| | Keyboard | Mouse |
|---|---|---|
| next / prev | `← →` (`↑ ↓`, Space, PageUp/Down ±10, Home/End) | wheel when fitted, side buttons 8/9, edge ‹ › on hover, two-finger swipe |
| gallery ↔ viewer | `Enter`, `g`, `Esc` | double-click tile / middle click in viewer / Grid button |
| pan · zoom | `Shift+arrows` · `+ −`, `Ctrl+0`, `Ctrl+1/2`, `w` scale modes | left drag · Ctrl+wheel around pointer, double-click 1:1, pinch |
| folders | `[ ]`, `Ctrl+←/→`, `j` jump list, `m` flat/grouped | header click, jump list, Shift+wheel, side buttons in gallery |
| selection | arrows, `Shift+arrows`, `Space`/`Insert`, `Ctrl+A` | click, Ctrl/Shift+click, middle click, rubber-band drag, corner checkbox |
| filters | `Tab`/`Shift+Tab`, `1–9`, `0`, `i` | Filters chip (wheel cycles, middle resets), popover checkboxes |
| thumbnails size | `+ −`, `Ctrl+0` | Ctrl+wheel, status-bar px label |
| captions · help | `t`, `Alt+Enter` · `h`, `?` | toolbar buttons, ? icon, metadata rows clickable |
| video | `Space`, `Shift+←/→` ±5 s, `Ctrl+Shift` ±30 s, `, .` step, `Shift+↑/↓` volume, `v` mute, `l` loop | click = play/pause, pill: bar scrub, speaker, time |
| files | `y` copy path, `o` open folder, `e` open with, `Ctrl+O`, `Ctrl+R` | context menu (right click), status-bar root/counts |
| app | `f`, `q`/`Ctrl+Q`, `Menu`, `Ctrl+Tab` focus regions | ⋮ menu, right click empty space |

Implementation: one static action table → `gio::SimpleAction` + accels + menus + tooltips + help overlay.
Capture-phase `EventControllerKey` for keys that must beat widget defaults; `GestureClick(button 0)` reading
`current_button()` for 1/2/3/8/9 (accept 10/11 too); `EventControllerScroll` with `unit()` to separate wheel
from touchpad; `GestureZoom` for pinch; `PopoverMenu::from_model` for context menus.

## 9. Video

`gtk::Video` + `gtk::MediaFile` (GTK's built-in GStreamer backend; DMABuf/GL zero-copy on Wayland;
`set_graphics_offload`). Install: `gst-plugins-good gst-plugin-va gst-plugin-dav1d` (+ `gst-libav` optional).
Thumbnails: `ffmpegthumbnailer -s 256` (65–87 ms per file). Tiles show a play badge + duration (duration read
lazily via GStreamer discoverer or ffprobe in the worker).

## 10. Roadmap

| Phase | Scope | Gate |
|---|---|---|
| 0 · skeleton (½ day) | cargo project from `verification/skeleton`, `mzera` CLI, desktop entry, `Super+Alt+P` → mzera, folder picker, theme.rs live, empty window shown before scan | opens from all launch paths, themed, re-skins on `omarchy theme set`, first frame < 120 ms |
| 1 · scan + gallery core (1 day) | scan.rs streaming, model.rs, `GalleryView` flat mode with placeholders, keyboard/mouse basics (arrows, click, double-click, wheel, Ctrl+wheel size) | 35 k folder: first tiles < 200 ms, full scan < 300 ms, 60 fps scroll with placeholders |
| 2 · thumbnails (1 day) | thumbs.rs: fd.o cache, libvips workers, priority queue, per-frame throttle, `--warm`, video thumbs via ffmpegthumbnailer | warm scroll 60 fps; cold: visible tiles fill in < 150 ms; `--warm` on the test set < 2 min |
| 3 · viewer (1 day) | `ImageView`, glycin Pool, preload ring, scale modes, zoom/pan/pinch, side buttons, toolbar auto-hide, captions 3 modes, help overlay from action table | next/prev < 16 ms perceived on 1080p WebP; no decode on the UI thread |
| 4 · grouped gallery + filters (½–1 day) | sections with sticky headers, jump list, `m`/`[ ]`, format chips with counts, presets 1–9, folder-only filter | filter toggle < 10 ms; section jump instant |
| 5 · video (½ day) | install gst plugins, `gtk::Video` in the viewer, control pill, keys | WebM plays with VA decode; arrows still navigate; zero-copy confirmed or documented |
| 6 · integration (½ day) | xdg-mime default, Nautilus "Open with" + script, slideshow, copy path / open folder, settings file, swayimg kept as fallback | double-click in Nautilus opens mzera on the whole folder |
| later | trash with undo, rotate/flip, multi-select ops, sort by date/size, name search, turbojpeg scaled decode, tiling, hover-scrub, per-folder state | |

Total to a usable v1 (phases 0–6): about 5–6 working days.

## 11. Decisions still open

1. App shell: V1 immersive (floating pill, no header bar) vs V2 classic header bar + sidebar. Recommendation: V1.
2. Thumbnail cache: shared freedesktop PNG (recommendation, 0.46 ms reads, Nautilus contributes) vs private JPEG.
3. Metadata: side panel (recommendation) vs corner overlay; both can exist as caption modes.
4. Keep swayimg + `photos` as fallback until phase 6 is done (recommendation: yes).

## 12. Commands

```sh
# toolchain (done): rustup in user space
export PATH="$HOME/.cargo/bin:$PATH"

# video support (phase 5)
omarchy pkg add gst-plugins-good gst-plugin-va gst-plugin-dav1d

# build / run
cargo build --release && ./target/release/mzera ~/Pictures
./target/release/mzera --warm ~/Pictures
```
