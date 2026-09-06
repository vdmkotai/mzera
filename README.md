# Mzera

A small, fast photo and WebM browser for Omarchy (Hyprland/Wayland), written in
Rust with GTK4 and libadwaita. It is fully usable with either the keyboard or
the mouse and follows the active Omarchy theme live.

The usable v1 described by phases 0–6 of [`docs/PLAN.md`](docs/PLAN.md) is
implemented. Later ideas explicitly listed in the plan—editing, trash/undo,
sorting by metadata, name search, hover-scrubbing, and per-folder state—remain
out of scope for v1.

## Features

- streaming recursive scan with natural ordering and immediate first results
- flat or folder-grouped custom gallery with sticky headers and a jump list
- freedesktop-compatible 256/512 px thumbnail cache, priority workers, and
  bounded result back-pressure
- JPEG, PNG, WebP, GIF, HEIC/AVIF, JXL, SVG/TIFF and WebM/video filtering
- fullscreen image viewer with preload ring, byte-capped LRU, scale modes,
  pan/zoom/pinch, rotation, captions, EXIF metadata, slideshow, and key help
- WebM playback through GTK/GStreamer, including GTK's seek/volume controls
- persistent layout, thumbnail, scale, caption, and slideshow preferences
- desktop entry, MIME integration, Nautilus script, and CLI cache warming

## Build and verify

```sh
make check
make build
./target/release/mzera ~/Pictures
./target/release/mzera --viewer ~/Pictures
./target/release/mzera --warm ~/Pictures --size 256
```

Build natively on either x86_64 or aarch64 Linux with the same commands above.
Cargo selects the host architecture; no x86-specific compiler flags are required.
`cargo` must be on PATH (both the Arch Rust package and rustup are supported).
On current Arch/Omarchy, install the build and image-decoding dependencies with:

```sh
omarchy pkg add rust pkgconf gtk4 libadwaita libvips glycin glycin-gtk4
```

GTK 4.22 or newer and libadwaita 1.9 or newer are required.
The required Rust components are `rustfmt` and `clippy`. Runtime video support
on Omarchy is installed with:

```sh
make deps-video
```

This installs `gst-plugins-good` and `gst-plugin-dav1d` via `omarchy pkg add`.
VA-API is not required on Apple Silicon: `gst-plugin-va` is optional for GPUs
with working VA-API drivers. `gst-libav` is an optional compatibility fallback.

## Install and desktop integration

```sh
make install
make default       # optional: register mzera as the default viewer
```

`make install` places the binary and desktop entry under `~/.local`, and adds
the Nautilus script “Open in Mzera”. The desktop icon is shared with mzera-mac. To replace the existing Omarchy Photos
binding in `~/.config/hypr/bindings.lua`, use:

```lua
hl.unbind("SUPER + ALT + P")
o.bind("SUPER + ALT + P", "Mzera", { launch = "mzera" })
```

Then validate the Hyprland configuration with `hyprctl reload` followed by
`hyprctl configerrors`.

Implementation measurements and runtime verification results are in
[`docs/verification/implementation.md`](docs/verification/implementation.md).
