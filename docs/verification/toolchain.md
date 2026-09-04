# Toolchain verification — gtk4-rs on this system

Project used: `/tmp/mzera-verify` (copied to `skeleton/`). rustc/cargo 1.98.1 via rustup (user space, `~/.cargo/bin`).

## Crate versions (crates.io, 2026-09-04)

| crate | version | notes |
|---|---|---|
| gtk4 | 0.11.4 | MSRV 1.92; features `v4_2`…`v4_24`; use **`v4_22`** (system GTK 4.22.4), do not enable `v4_24` |
| libadwaita | 0.9.2 | features `v1_1`…`v1_10`; use **`v1_9`** |
| glycin | 3.1.0 stable (4.0.0-beta.1 is a prerelease, untested) | features `gdk4` (gives `Frame::texture() -> gdk::Texture`), `tokio`, `gobject`. Loader API "2+" ↔ crate 3.x |
| glycin-gtk4 | does not exist | the `gdk4` feature of `glycin` is the equivalent |
| gstreamer / -video / -play | 0.25.3 / 0.25.3 / 0.25.0 | version-checked only, not compiled |
| libvips | 2.3.0 latest; **1.7 compiled and worked** against libvips 8.18 in the thumbnail benchmark | |
| image / fast_image_resize | 0.25.10 / 5 | |
| webp / libwebp-sys | 0.3.1 / 0.14.4 | |
| turbojpeg | latest | for DCT-scaled JPEG decode of huge files |
| rexiv2 / kamadak-exif / nom-exif | 0.10.0 / 0.6.1 / 3.7.0 | |
| walkdir / ignore | 2.5.0 / 0.4.33 | jwalk 0.9.0 is unmaintained |
| rayon / notify / toml | 1.12.0 / 8.2.0 / 1.1.5 | |

## Compile test

```toml
[dependencies]
gtk    = { package = "gtk4",       version = "0.11", features = ["v4_22"] }
adw    = { package = "libadwaita", version = "0.9",  features = ["v1_9"] }
glycin = { version = "3.1", features = ["gdk4"] }
```

- Compiles with zero warnings. Resolved: gtk4/gdk4/gsk4 0.11.4, libadwaita 0.9.2, glycin 3.1.0, glib/gio 0.22.9, zbus 5.19.0. 160 crates.
- Clean `cargo build --release` (sources pre-fetched, 16 cores): **48.9 s**. Incremental after touching main.rs: **2.9 s**.
- Binary 6.8 MB (4.7 MB stripped), dynamically linked PIE.
- Test program: Adw application, ListStore of 35 000 StringObjects → SortListModel with a section sorter → GridView with label tiles, plus a ListView with a header factory for sections. Header binding verified at runtime.

## Cold start (Wayland/Hyprland, release, 5 runs, warm page cache)

| point | ms from `main()` |
|---|---|
| `activate` | 49–51 |
| 35 k-item model fill | 7 |
| window `map` | 116–128 |
| first `after-paint` (first frame) | **158–183** |

Not measured: dropped-caches cold start (needs root).

## API presence in gtk4 0.11.4 (compile-checked)

Present: `gtk::SectionModel`, `FilterListModel::set_incremental`, `gdk::Texture::from_filename`, `gdk::MemoryTexture::new`, `gdk::DmabufTextureBuilder`, `Picture::set_paintable`, `MediaFile::for_filename`, `Video`, `EventControllerScroll`, `GestureDrag`, `GestureZoom`, `ShortcutController`, `glycin::Loader`.

**Absent: `gtk::GridView::set_header_factory`.** This is GTK itself, not the bindings: GTK 4.22.4 headers and `libgtk-4.so` only export `gtk_list_view_/gtk_column_view_/gtk_drop_down_set_header_factory`. The plan's "GtkGridView sections since 4.12" premise was wrong.

## glycin runtime facts

- The Rust crate does not link `libglycin-2`; it spawns loader binaries from `/usr/lib/glycin-loaders/2+/` over a D-Bus peer connection inside a bwrap sandbox.
- Verified: PNG 1024² → `gdk::Texture` in 86 ms, WebP 1024² in 100 ms (includes sandbox spawn; use `glycin::Pool` to reuse loader processes).
- API: `Loader::new(gio::File).load().await -> Result<Image, ErrorCtx>`, `Image::next_frame().await`, `Image::details().info_format_name()`, `Image::mime_type()`, `Frame::texture()`.

## Gotchas

- `cargo add glycin` picks 3.1.0 (prereleases skipped). Do not use crate 2.x or 4.0-beta with loaders "2+".
- `/usr/bin/time` and `bc` are not installed. `vips` warns about a missing `libopenslide.so.1` module (harmless).
