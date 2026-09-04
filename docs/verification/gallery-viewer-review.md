# Gallery / viewer performance review (GTK 4.22.4 internals vs the plan)

Verified locally: default renderer is **GskVulkanRenderer** (RADV Phoenix, `maxImageDimension2D = 16384`); `Gtk.MediaFile` instantiates as `GtkGstMediaFileBuiltin` (GStreamer backend compiled into libgtk-4 on Arch); `~/.cache/thumbnails/{large,x-large}` exists.

## Two findings that break the original plan

1. **GtkGridView has no sections / header factory.** Only GtkListView has it (`gtklistview.h:61`); `gtkgridview.c` at 4.22.4 contains zero occurrences of "header". An unmerged upstream branch `gridview-sections` exists (GTK issue #2854). Even ListView headers are ordinary inline tiles, not sticky.
2. **glycin does not downscale raster images.** `FrameRequest::scale` is stored but the image-rs loader (JPEG/PNG/WebP/GIF/TIFF) never reads it; glycin's own thumbnailer decodes full size and resizes on the CPU. Shrink-on-load only exists via libwebp / libjpeg-turbo / libvips.

## 1. GtkGridView vs custom widget

GtkGridView facts (4.22.4 source): measures only materialised tiles; live widget budget is fixed at `max_columns × 32 + 1` regardless of viewport (default max-columns 7 → 225 widgets); if the viewport shows more than 30 rows, rows go blank unless `max-columns` is raised, and raising it to 24 for small tiles means ~770 tiles permanently bound, most off-screen. `scroll_to` works for unmaterialised items but not before first allocation.

**Recommendation: custom `GalleryView` (`gtk::Widget` + `gtk::Scrollable`), own layout, own `snapshot()`.** Fixed-size square tiles make layout trivial (row = idx / cols, per-section prefix sums, O(log sections) lookup); zero child widgets, bind cost 0; one snapshot with 200–400 `append_texture` nodes per frame (trivial for Vulkan); sticky section headers drawn in snapshot; visibility priority is exact because the widget owns the geometry. Same approach as Loupe's image widget. Wrap in `GtkScrolledWindow` for kinetic scrolling and scrollbars. Cost: ~700–900 lines. Loses accessibility and built-in rubber-band (both acceptable; rubber-band can be drawn by hand later).

## 2. Textures

- `GdkTexture` is immutable and **thread-safe**; `from_filename`/`from_bytes` are documented for use in worker threads. GTK has built-in PNG/JPEG/TIFF loaders for this path.
- Cheapest path: decode on a worker → `gdk::MemoryTexture::new(w, h, R8g8b8a8, bytes, stride)` (keeps a ref, no copy), or `Texture::from_filename` on the cache PNG. dmabuf is pointless at 256 px.
- GPU upload happens at first draw and is cached on the texture; ≤ 256×256 textures go into 1024² atlases (ideal). GPU-side GC after 15 s unused; you only manage CPU-side refs.
- Avoid upload stalls: a fresh screen is ~200 tiles × ~200 KB ≈ 40 MB in one frame. Assign at most ~32 new textures per frame from a tick-callback drain, nearest-to-viewport first; stop requesting decodes while scroll velocity is high, request visible ± 1 screen once the position settles for 1–2 frames.
- Memory: ~175–262 KB per tile texture. Keep 1 500–2 500 resident (256–512 MB), pin visible ± 2 screens, evict by distance from viewport, then by age.

## 3. Model

- 35 k GObjects in a ListStore ≈ 10 MB in Rust, fine; but keep items thin (`Rc<Entry>` or index into a slab).
- `SortListModel` with a custom sorter: ~530 k FFI comparisons for 35 k (0.1–0.2 s) and **insertions during an incremental sort restart the sort**; a streaming scan feeding a SortListModel is pathological.
- `FilterListModel` incremental mode: 512 items per idle step → ~1 s of churn for 35 k. A kind-enum filter is 3–5 ms synchronously. **Do not use incremental mode.**
- Recommended: scanner emits entries in **sorted DFS order** (natural sort per directory) so the base list is already folder-grouped and name-sorted; sections are contiguous runs. With a custom view the model can be a plain `Vec<Entry>` + `Vec<u32>` section starts + a filtered index `Vec<u32>` rebuilt in ~2 ms. No SortListModel in v1; later sorts are a `par_sort_unstable_by_key` on a worker (~5 ms) and one refresh.

## 4. Scan

- `std::fs::read_dir`: `DirEntry::file_type()` is free (`d_type`); `metadata()` is an `lstat` each — **skip it during the scan**, stat lazily in the thumbnail worker (needed for fd.o mtime validation) and for captions.
- Sorted single-threaded DFS of 35 k entries: 50–150 ms warm. `ignore::WalkBuilder::build_parallel()` only helps on cold cache and loses ordering; `jwalk` is unmaintained.
- Stream in batches of 512–1024 over `async_channel`, consume with `glib::spawn_future_local`, insert at `Priority::DEFAULT` (not idle — idle starves behind rendering). ~40–70 batches total.

## 5. Viewer

- Custom `ImageView` widget (Loupe pattern): `snapshot()` does `append_scaled_texture(texture, filter, rect)` under a zoom/pan transform; `Trilinear` when zoom < 1 (GPU mipmaps), `Linear`/`Nearest` at ≥ 1; snap origin to device pixels except during animation. Zoom/pan = `GestureZoom` + `GestureDrag` + `EventControllerScroll` mutating (scale, offset) and `queue_draw()`; never re-decode. `gtk::Picture` cannot do this.
- Decode: glycin crate 3.x with a shared `Arc<Pool>` (loader process reuse; default retention 30 s) and `accepted_memory_formats(R8g8b8a8 | R8g8b8)` to avoid f16 textures. The texture is an mmap of a sealed memfd — no copy on the UI thread. Keep the sandbox on for the viewer.
- Preload ring ±2 with per-slot `gio::Cancellable`; bytes-capped LRU ≈ 1–1.5 GB (a 4K RGBA frame is 33 MB; 24 MP JPEG is 96 MB). First draw still uploads; possible mitigation (unverified): draw the next texture inside a 1×1 clip node one frame early.
- Huge images: glycin gives full size only; RADV texture limit 16 384 px → Loupe-style tiling for larger; for JPEGs far above screen size use `turbojpeg` DCT scaling (1/2…1/8) for the fit view, 1:1 lazily.
- Animated WebP/GIF: loop `Image::next_frame()` driven by a tick callback and `Frame::delay()`, keep the `Image` alive while visible. (Test set has no animated files.)

## 6. Video

- GTK 4.22's built-in GStreamer backend advertises DMABuf, GLMemory and system-memory caps and builds `GdkDmabufTexture`/`GdkGLTexture`/`GdkMemoryTexture` accordingly; zero-copy paths exist; ported to GstPlay; gapless looping with GStreamer 1.28. `Gtk.Video`/`Gtk.MediaFile` is the right choice; `gtk4paintablesink` only if driving the pipeline yourself.
- Missing packages: `gst-plugins-good` (matroskademux, vp8/vp9 via libvpx), `gst-plugin-va` (`vavp9dec`/`vaav1dec`; VA driver `radeonsi_drv_video.so` is present via mesa), `gst-plugin-dav1d` (software AV1; **not** part of `gst-plugins-bad` on Arch), optionally `gst-libav`. Zero-copy end-to-end is plausible but unverified: check with `GST_DEBUG=gtkgstsink:4`.

## 7. Startup

Dynamic linking + GTK init + Wayland/Vulkan context ≈ 40–80 ms. Rules: tiny `.gresource`, one CssProvider with ~50 lines of generated theme CSS, no icons or `gtk::Picture` in the first frame, present the window **before** starting the scan thread, read `colors.toml` synchronously, no gdk-pixbuf. Expect 60–100 ms to first frame; if Vulkan init dominates, compare `GSK_RENDERER=ngl`.

## 8. What in the original plan will not hold

- GridView sections + sticky headers: impossible (above).
- glycin for thumbnails: no scaled decode, and a bwrap spawn per decode without `Pool` — minutes for 35 k. Use libvips in-process (or libwebp-sys + turbojpeg) for tiles; glycin with a Pool only for HEIC/AVIF/JXL/SVG/TIFF and for the viewer.
- FilterListModel incremental: slower, not faster.
- SortListModel over a streaming scan: restarts the sort per batch. Sort in the scanner.
- "GridView recycles so only visible+buffer widgets exist": the buffer is 32 × max-columns regardless of viewport.
- fd.o cache PNGs need stat + PNG decode per tile (0.3–0.5 ms on a worker): fine, but never stat during the scan.

## Recommended architecture

- Model: scanner (sorted DFS) → `Vec<Entry>` slab → section-start array → filtered index → `GalleryView`. Selection = own indices; no `SelectionModel`.
- Widgets: `GalleryView` (custom, Scrollable, sticky headers in snapshot, jump list = scroll to section offset); `ImageView` (custom snapshot with zoom/pan); `gtk::Video`/`MediaFile` for WebM; overlays as `gtk::Overlay` children in the viewer only.
- Threads: main (GTK + glycin futures); scanner thread; thumbnail pool of N threads consuming a priority queue (key = distance from viewport centre, LIFO within equal distance, generation counter for cancellation); glycin `Pool` ≤ 4 parallel; GStreamer threads. Results via `async_channel` → `spawn_future_local`; textures assigned ≤ N per frame from a tick callback.
- Priorities: model splices at `Priority::DEFAULT`; thumbnail results drained on the frame clock; nothing at `DEFAULT_IDLE` on the hot path; `--warm` runs in its own process at `nice 10`.

## Risks

| Risk | Mitigation |
|---|---|
| Custom view loses keyboard/a11y semantics | implement arrows/Home/End/PageUp/Down/Enter by hand; accept no a11y |
| Upload hitch on first draw of a large viewer texture | 1×1-clip pre-draw (unverified) or accept one frame |
| glycin crate/loader mismatch | pin crate 3.x for loaders "2+"; assert `supported_mime_types()` at startup |
| VA zero-copy path not taken | verify with `GST_DEBUG=gtkgstsink:4`; force `vapostproc ! video/x-raw(memory:DMABuf)` via gtk4paintablesink if needed |
| Viewer LRU memory blow-up | byte cap ≈ 1 GB, cancel ring decodes on fast key repeat |
| fd.o PNG decode limits scroll on a cold box | throttle N textures/frame; private raw cache as a later option |
| Vulkan renderer regressions | `GSK_RENDERER=ngl` fallback in the launcher |

Sources: GTK 4.22.4 `gtk/gtkgridview.c`, `gtklistbase.c`, `gtklistitemmanager.c`, `gtksortlistmodel.c`, `gtkfilterlistmodel.c`, `gtk/media/gtkgstsink.c`, `gsk/gpu/gskgpucache.c`, `gdk/gdktexture.c`; local headers in `/usr/include/gtk-4.0/gtk/`; GdkTexture docs; GTK issues #2854, #3334; glycin `glycin-core/src/{api/loader.rs,pool.rs}`, `glycin-loaders/glycin-image-rs/src/lib.rs`, `glycin-thumbnailer/src/lib.rs`; Loupe `src/widgets/image/{rendering,loading}.rs`; gst-plugin-gtk4 README.
