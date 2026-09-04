# Verification (2026-09-04)

Four independent checks were run against the plan before writing code. Each file is the
reviewer's report, lightly edited. Numbers are measured on this machine
(AMD Ryzen Z1 Extreme, 16 threads, 15 GB, NVMe, Hyprland/Wayland, GTK 4.22.4, libadwaita 1.9.3,
glycin loaders 2.1.5, GStreamer 1.28.6, libvips 8.18.5).

| File | What it checked | Outcome |
|---|---|---|
| `toolchain.md` | gtk4-rs / libadwaita / glycin crates compile against the system, build times, cold start | OK. gtk4 0.11.4 `v4_22`, libadwaita 0.9.2 `v1_9`, glycin 3.1 `gdk4`. Clean build 49 s, incremental 3 s, first frame 160–180 ms with 35 k items |
| `thumbnails.md` | thumbnail decode / encode / cache-read costs on the real test set | libvips in-process 12 ms per 1080p WebP; full set ≈ 80 s on 16 threads; PNG cache reads 0.46 ms via Gdk.Texture; never use gdk-pixbuf or glycin for tiles here |
| `gallery-viewer-review.md` | GTK internals vs the plan: GridView, textures, models, scan, viewer, video, startup | Two plan-breaking findings: GtkGridView has no sections; glycin does not downscale. Custom gallery widget recommended |
| `input.md` | complete keyboard + mouse map, Hyprland conflicts, GTK4 implementation notes | Full map delivered; Super combos, Alt+Tab, media keys and Ctrl+Space are unavailable to the app |
| `skeleton/` | the Cargo.toml and main.rs that compiled and ran in the toolchain check | reference for Phase 0 |
| `implementation.md` | completed v1 checks and real-data gates | 13 tests pass; scan 84.8 ms; full thumbnail pass 78.3 s with 0 failures |
| `code-review.md` | independent post-implementation safety, concurrency, performance, and integration review | four high-severity defects and the actionable integration gaps corrected before release |

Test set (corrected path): `~/Downloads/University_of_Problems-1.9.0-Extended-pc`
37 098 files, 29.3 GB, 248 dirs. Images 34 988 (10.1 GB): WebP 32 779 (all 1920×1080, none animated),
PNG 1 261, JPEG 948. Video: 432 WebM (VP8/VP9 1080p, 4.5 GB).
