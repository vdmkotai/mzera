# Independent code review (2026-09-04)

The complete Rust implementation and desktop integration were reviewed by a
separate agent against `docs/PLAN.md`. The review included all source modules,
strict Clippy, debug and release tests, the release build, native linkage, GTK
lifecycles, concurrency, path handling, cache bounds, and GStreamer behavior.

## Corrected findings

- A reentrant `RefCell` borrow made every filter control panic. Callbacks now
  run only after state borrows are released, with a regression test.
- A gallery selection anchor stored a filtered position and could panic after
  narrowing the filter. It now stores a stable master index.
- A cancelled glycin future could remove a newer in-flight request for the same
  item. Requests now have identities and stale completions cannot mutate the
  replacement entry; the ordering case has a regression test.
- Viewer and gallery texture limits did not reflect actual allocations. Glycin
  is restricted to 8-bit RGB/RGBA, frame stride bytes are accounted exactly,
  oversized images are rejected before full-frame decode, and gallery textures
  have a hard 512 MiB byte cap.
- Popovers and menus now retain native keyboard handling; `Ctrl+Tab` cycles
  focus regions, metadata rows are keyboard-activatable, and toolbar controls
  can receive focus.
- Folder headers now expose the folder-only workflow (single click) and open
  the folder in the file manager (double click); the filter is removable.
- The scanner preserves Linux non-UTF-8 paths and reports incomplete directory
  reads instead of silently presenting a complete result.
- `ffmpegthumbnailer` and `ffprobe` have bounded execution time; transient
  helper failures are not stored as permanent thumbnail failures.
- Video backend errors are surfaced as toasts. Thumbnail uploads are drained
  from the frame clock at no more than 32 results per frame.
- Shutdown cancels decoders, media, timers, and the worker pool; owner callbacks
  use weak references where they would otherwise create the largest cycles.
- The installed desktop-file ID now matches `dev.vdm.mzera`, file URI handling
  is delegated to GIO, and the desktop entry intentionally accepts one root.

## Verified properties

- Application Rust code has crate-level `unsafe_code = "forbid"`.
- Scanner and thumbnail results are fenced by generation; result-channel
  back-pressure is bounded to twice the worker count.
- External paths are passed with `Command::arg`, not through a shell.
- Settings and thumbnail cache writes use temporary files plus atomic rename.
- Pan and zoom reject non-finite values and enforce scale bounds.
- The viewer uses one sandboxed glycin pool limited to four operations.
- Strict Clippy, debug tests, release tests, release build, desktop validation,
  real-data scan/thumbnail gates, and VP9 hardware playback pass.

## Residual design risks

- Native libvips and codec libraries are outside Rust's memory-safety boundary;
  full-size viewer decoding uses sandboxed glycin, while thumbnail decoding is
  intentionally in-process for measured performance.
- Multiple requested paths are accepted, but the UI has one active root and
  explicitly reports that only the first root is displayed.
- Animated GIF/WebP currently display their first decoded frame.
- The custom zero-child gallery has limited accessibility semantics compared
  with a standard GTK list/grid widget.
- Hardware VP9 decoding is confirmed; end-to-end DMABuf zero-copy has not been
  proven independently of GTK/GStreamer internals.
