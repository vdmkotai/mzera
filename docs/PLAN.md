# mzera — plan

Companion to `explainer.html` (same content, text form, with the mockups left out).

## 1. Requirements

Must have (v1)
- Open a folder recursively; first tiles visible in well under a second even for ~35 000 files.
- Gallery modes: flat grid, grouped by folder (sticky headers + folder jump list), thumbnail zoom.
- Filters by format: include / exclude JPEG, PNG, WebP, GIF, HEIC/AVIF, JXL, video.
- Fullscreen viewer: arrows navigate, scale modes (fit / fill / width / height / 1:1), pan, zoom.
- Caption modes: brief / metadata (path, size, EXIF) / none. Help overlay with all keys.
- WebM video inline (gallery thumbnail + playback in the viewer).
- Theme follows Omarchy automatically (colors, dark/light mode, font).
- Launch paths kept: `Super+Alt+P`, launcher entry, Nautilus "Open with" / script, CLI.
- English-only UI. No F-keys.

Nice to have (v2+)
- Slideshow, rotate, move-to-trash, "open in editor", copy path.
- Sort by name / date / size; name search; date and size filters.
- Persistent per-folder state (last position, mode, filter).
- Hover-scrub video thumbnails.

Not planned
- Editing, tagging, ratings, library database, cloud.

Hard constraints
- Wayland / Hyprland / Omarchy. No X11 assumptions.
- No QtWidgets look. Native GTK4 / libadwaita look is the target.
- Deliberately small codebase; performance is the top priority.

## 2. Options for the engine / stack

| | A. swayimg + Lua (today) | B. fork swayimg (C) | C. GTK4 + libadwaita, Python ★ | D. GTK4 + libadwaita, Rust | E. Qt Quick / QML | F. Web (Tauri/Electron) | G. GTK front + swayimg engine |
|---|---|---|---|---|---|---|---|
| Grouped-by-folder gallery | no (flat imagelist) | write from scratch | native (GtkGridView sections, GTK ≥ 4.12) | native | write (ListView + sections) | write | only in the front, not in the viewer |
| Clickable, themeable UI | no | hand-drawn in C | yes | yes | yes, custom-skinned | yes | half |
| WebM video | no | ffmpeg integration, big | Gtk.MediaFile (GStreamer) | same | QtMultimedia (extra pkgs) | `<video>` | no |
| Thumbnail throughput | single decoder, stalls at ~90 | good | libvips / glycin workers, native speed | best | Qt image threads | native backend needed | libvips workers |
| Omarchy theme sync | colors only | colors only | CSS provider, live | same | QML palette, live | CSS, live | front only |
| Toolchain installed | yes | gcc, meson missing | yes (python-gobject, gtk4, adw, glycin, gst) | rustup needed | qt6-declarative yes, multimedia no | node yes, webkit no | yes |
| Dev cost to v1 | done, at ceiling | high | ~2–3 days | ~5–7 days | ~4–5 days | ~4 days | ~1 day, dead end |
| Maintenance | none | C codebase | small Python | Rust | QML + C++ glue | JS + native | two apps |

Recommendation: **C**, structured so every hot path is native (glycin loader processes,
libvips threads, GTK GPU renderer, GStreamer). Python only orchestrates. If profiling ever
shows Python on a hot path, that module gets ported to Rust; the boundaries are chosen so
that is a module swap, not a rewrite.

Rejected: A (ceiling reached), B (all UI by hand, no video), E (user does not want Qt;
QML would look fine but pulls a second toolkit into a GTK desktop), F (heavy, not native),
G (two apps, video and grouped gallery impossible in the swayimg half).

## 3. Architecture

```
CLI / desktop / Hyprland bind / Nautilus
        │
        ▼
  mzera.app  (Adw.Application, single instance, opens paths)
        │
        ├── scanner      os.scandir recursive in a thread, batches into Gio.ListStore
        ├── model        MediaItem(GObject): path, kind, ext, size, mtime, folder
        ├── pipeline     Gtk.FilterListModel (format filters) → Gtk.SortListModel → sections by folder
        ├── gallery      Gtk.GridView (recycling) + header factory (folder sections) + jump list
        ├── thumbs       ThumbService: freedesktop cache (~/.cache/thumbnails/{large,x-large})
        │                  workers: libvips (jpeg/png/webp/heif/jxl shrink-on-load),
        │                  glycin fallback, ffmpegthumbnailer for video; priority = visible first
        ├── viewer       Gtk.Picture + glycin async decode; ±2 preload ring; Gdk.Texture LRU
        ├── video        Gtk.Video / Gtk.MediaFile (GStreamer, VA-API hw decode)
        ├── overlays     captions (brief / metadata / none), help, status toast
        ├── keys         one table → shortcuts + help overlay generated from it
        └── theme        reads ~/.local/state/omarchy/current/theme/colors.toml,
                         generates CSS, Gio.FileMonitor for live reload, optional user .tpl
```

## 4. Performance plan (test set: ~35 000 files, 10 GB, mostly WebP)

Budgets
- Open folder → first grid frame: < 300 ms (scan streams, does not block).
- Full scan of 35 k entries: < 1 s.
- Scroll with warm cache: 60 fps, no per-frame decode on the UI thread.
- Cold thumbnail build: 35 k WebP at 256 px on 16 threads ≈ 2–3 min, in the background, visible tiles first.
- Viewer next/prev with preload: < 50 ms perceived.
- Memory: 35 k items in the model ≈ 30–40 MB; texture cache capped (default 512 MB).

How
1. Stream the scan into the model in batches (splice), UI never waits for the full tree.
2. Freedesktop thumbnail cache: shared with Nautilus, survives restarts, standard PNG layout with mtime check.
3. Worker pool sized to cores; LIFO priority queue keyed by visibility; cancel when a tile scrolls away.
4. libvips shrink-on-load for JPEG / WebP (decoder scales during decode); HEIF / JXL via libvips loaders; glycin for the rest.
5. Gdk.Texture created off the UI thread; GtkGridView recycles tile widgets so only ~visible+buffer widgets exist.
6. Gtk.FilterListModel in incremental mode so filter changes on 35 k items do not freeze the UI.
7. Viewer keeps a small ring of decoded neighbours; big images decoded by glycin in loader processes (no GIL).
8. `mzera --warm <dir>`: reuse the existing photos-warm idea, writes into the same fd.o cache.

## 5. Theme sync with Omarchy

- Source of truth: `~/.local/state/omarchy/current/theme/colors.toml` (always present after `omarchy theme set`).
- App maps: background → window, dark_background → gallery, lighter_background → tiles/cards, accent → selection ring & active chips, foreground/muted → text, mode → Adw.StyleManager (dark/light).
- Live reload: Gio.FileMonitor on that file (and `theme.name`); no restart needed.
- Optional: `~/.config/omarchy/themed/mzera.css.tpl` → rendered to `current/theme/mzera.css` by `omarchy-theme-set-templates`; if present it overrides generated CSS.
- Font: monospace overlays use `omarchy font current` (JetBrainsMono Nerd Font); UI font comes from GTK settings.

## 6. Video (WebM)

- Playback: `Gtk.Video` with `Gtk.MediaFile` (GStreamer backend, already used by GTK).
- Packages to add: `gst-plugins-good` (matroskademux, vp8/vp9), `gst-plugins-bad` (dav1d for AV1 in WebM), `gst-plugin-va` (hardware decode on the AMD Phoenix iGPU).
- Thumbnails: `ffmpegthumbnailer` (already installed as a thumbnailer) or `ffmpeg -ss 1 -frames 1`.
- Gallery tile shows a play badge + duration; viewer shows minimal controls, space = play/pause.

## 7. Keys (carried over from the swayimg config)

| Key | Action |
|---|---|
| ← → | previous / next |
| Shift + arrows | pan |
| g, Enter | gallery ↔ viewer |
| Tab, Shift+Tab, 1–9, 0 | cycle / pick / reset format filter |
| w, Shift+w | scale mode next / previous |
| + − (gallery) | thumbnail size |
| t | captions: brief → metadata → none |
| h, ? | help overlay |
| m | gallery layout: flat ↔ by folder |
| f | fullscreen, q / Esc close |
| Space (video) | play / pause |

## 8. Roadmap

Phase 0 — skeleton (½ day)
- package layout, `mzera` CLI, desktop entry, `Super+Alt+P` → `mzera`, folder picker, empty window, theme sync working. Gate: opens, is themed, switches theme live.

Phase 1 — gallery + thumbnails (1 day)
- scanner, model, flat GridView, ThumbService with fd.o cache, warm command, thumbnail zoom.
- Gate: 35 k folder opens < 300 ms to first frame, scroll stays smooth with warm cache.

Phase 2 — viewer (½–1 day)
- fullscreen viewer, arrows, scale modes, pan/zoom, preload ring, captions (3 modes), help overlay.
- Gate: next/prev < 50 ms perceived on the test set.

Phase 3 — filters + grouped gallery (½ day)
- format filter chips, incremental filtering, sections by folder with sticky headers, folder jump list, mode toggle.

Phase 4 — video (½ day)
- install gst plugins, video tiles, playback in the viewer, video in filters.

Phase 5 — integration & polish (½ day)
- xdg-mime default, Nautilus script/`Open with`, slideshow, trash, rotate, settings file, retire the swayimg config (keep as fallback).

Later
- Rust port of scanner/thumbs if profiling asks for it; hover-scrub; per-folder state.

## 9. Decisions still open

1. Python now vs. Rust from day one (recommendation: Python now).
2. App shell: immersive (no header bar, floating controls) vs. classic Adwaita header bar + sidebar (recommendation: immersive; see mockups).
3. Thumbnail cache: freedesktop shared cache (recommendation) vs. private cache.
4. Keep swayimg + `photos` as a fallback command, or remove once mzera is stable.
