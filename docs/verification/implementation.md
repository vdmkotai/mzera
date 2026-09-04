# Implementation verification (2026-09-04)

The v1 implementation was reviewed against `docs/PLAN.md`, corrected, and
verified on the machine used for the original plan.

## Automated checks

- `cargo fmt --all -- --check`: pass
- `cargo clippy --all-targets -- -D warnings`: pass
- `cargo test --all-targets`: 13 passed
- `cargo test --all-targets --release`: 13 passed
- `cargo build --release`: pass
- crate-level `unsafe_code = "forbid"`

Tests cover format classification, total natural ordering, sorted recursive
scanning (including non-UTF-8 names), filtering/section rebuilding, reentrant
filter callbacks, selection retention, nearest-item navigation, stale decoder
completion, settings defaults, freedesktop PNG metadata validation, grayscale
pixel conversion, and size formatting.

An independent post-implementation review found four high-severity defects and
several integration gaps. The panic paths, decoder cancellation race, memory
accounting, input focus, folder filtering, scanner diagnostics, helper process
timeouts, media errors, frame throttling, shutdown, and desktop integration
were corrected before release. See `code-review.md`.

## Real data gates

Dataset: `~/Downloads/University_of_Problems-1.9.0-Extended-pc`.

| Gate | Result |
|---|---:|
| Media discovered | 35,420 in 248 folders |
| Full recursive scan | 84.8 ms (budget: 300 ms) |
| Warm startup to mapped window | 92.4–105.9 ms (budget: 120 ms) |
| Full 256 px cache build/read pass | 78.3 s, 0 failures (budget: 2 min) |
| Re-read of the complete warm cache | 3.7 s, 0 failures |

The GUI was launched against both `/usr/share/pixmaps` and the full test set.
The only GTK diagnostic was an unavailable per-session accessibility bus; it
does not affect the application.

## Video runtime verification

The planned GStreamer runtime packages were installed through Omarchy:

```sh
omarchy pkg add gst-plugins-good gst-plugin-va gst-plugin-dav1d
```

`matroskademux`, the software `vp8dec`/`vp9dec` fallbacks, `dav1ddec`, and the
available VA decoders all register successfully. A 1920x1080 VP9 WebM from the
real dataset was discovered and decoded to end-of-stream without errors;
GStreamer selected `vavp9dec` on `/dev/dri/renderD128`. Opening the same file in
the installed `mzera` binary mapped the window in 103.7 ms, completed its folder
scan in 37.8 ms, and produced no GStreamer errors during the playback smoke
test.
