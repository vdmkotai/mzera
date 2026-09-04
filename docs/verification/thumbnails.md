# Thumbnail pipeline benchmark (real test set)

All runs `nice -n 10`, scripts in `/tmp/mzera-bench`. Nothing written to `~/.cache`.

## Inventory — `~/Downloads/University_of_Problems-1.9.0-Extended-pc`

37 098 files, 29.3 GB, 248 dirs.

| ext | count | total | median | p90 | p99 |
|---|---|---|---|---|---|
| webp | 32 779 | 9.55 GB | 278 KB | 430 KB | 645 KB (max 4.4 MB) |
| png | 1 261 | 203 MB | 112 KB | 271 KB | 1.8 MB |
| jpg | 948 | 355 MB | 129 KB | 1.0 MB | 1.3 MB |
| webm | 432 | 4.54 GB | ~12 MB | | |
| other (rpa, ogg, py…) | ~1 700 | 15 GB | not images | | |

Images: 34 988 (10.1 GB). Every sampled WebP is 1920×1080, VP8X lossy with a 672-byte ICC profile. **Animated WebP: 0** (checked all 32 779 via VP8X flags and `vipsheader n-pages`).

## Single-file cost, 1920×1080 WebP → 256 px (median of 200)

Process startup alone: `vips` 28 ms, `magick` 5 ms. It dominates every CLI number.

| method | median ms | p90 | output |
|---|---|---|---|
| `vips thumbnail f out.png 256` (CLI) | 56.1 | 67.3 | 68 KB |
| `vips thumbnail f out.jpg[Q=85] 256` | 48.7 | 53.5 | 9.8 KB |
| `vips webpload f` full decode, no shrink | 72.1 | 76.5 | |
| `vips webpload f --shrink 7` | 48.5 | 53.2 | |
| `magick f -thumbnail 256x256 out.jpg` | 36.6 | 43.0 | 9.1 KB |
| **Rust, in-process libvips (`libvips` crate 1.7), no encode** | **12.3** | | |
| Rust libvips → PNG (compression 1) | 14.6 | | 69.5 KB |
| Rust libvips → JPEG Q85 | 13.5 | | 9.6 KB |
| Rust libvips → WebP Q85 | 16.0 | | 7.1 KB |
| Rust `image` 0.25 + `fast_image_resize` 5, no encode | 35.1 (decode 32.2 + resize 2.8) | | |
| Rust `image` with its own Lanczos3 resize | 46.0 (resize 15.1) | | |

JPEG 1080p sources: libvips 13.5 ms, `image` 13.5 ms. Small PNG/JPEG sprites: < 1–3 ms.

Decode dominates (> 80 %): encode adds 1.2 ms (JPEG), 2.3 ms (PNG), 3.7 ms (WebP). Shrink-on-load is the difference between 12 ms (libvips, libwebp scaled decode) and 32 ms (pure-Rust decoder without scaled decode).

## Throughput, 2 000 random WebP, 16 workers

| run | real | user | sys | rate |
|---|---|---|---|---|
| `xargs -P16` × `vips` CLI, warm | 12.46 s | 127 s | 49.8 s | 160 f/s |
| 16 × Rust in-process libvips → PNG, warm | **4.43 s** | 56.8 s | 4.2 s | **451 f/s** |
| same → JPEG | 4.04 s | 53.1 s | | 495 f/s |
| 8 workers → PNG | 5.51 s | 53.6 s | | 363 f/s |
| 4 workers → PNG | 9.51 s | 46.0 s | | 210 f/s |
| 16 × Rust, **cold** (fadvise DONTNEED, 0 MB resident verified) | 4.78 s | 57.0 s | 4.2 s | 418 f/s |
| pure IO floor: `cat` of the same 558 MB, 16 procs | 0.18 s | | | 3.1 GB/s |

**CPU-bound.** Cold adds only 8 %; IO floor is 4 % of wall time. One process per file wastes 50 s of sys time on exec/mmap (2.8× slower). SMT adds ~24 % over 8 workers.

## Cache read side (500 × 256×144 thumbnails, tmpfs, warm)

| reader | PNG | JPEG | WebP |
|---|---|---|---|
| file size, median | 70 KB | 11 KB | 8.5 KB |
| **GTK 4.22 `Gdk.Texture.new_from_filename`** | **0.46 ms** | **0.10 ms** | 6.10 ms |
| gdk-pixbuf `Pixbuf.new_from_file` | 6.43 ms | 6.49 ms | 7.28 ms |
| in-process `image` crate | 0.27 ms | 0.14 ms | 0.69 ms |
| in-process libvips | 0.99 ms | 0.69 ms | 1.25 ms |
| `Gdk.MemoryTexture.new` from RGB bytes | 0.003 ms | | |

Key finding: on this system gdk-pixbuf 2.44 has **no native loader modules** (`/usr/lib/gdk-pixbuf-2.0/2.10.0/loaders/` is empty); it delegates to a bwrap-sandboxed glycin loader per image, hence 6.4 ms per tile and no scaling across processes. GTK 4.22 has built-in PNG/JPEG decoders, so `Gdk.Texture` is fast for those; WebP goes through glycin (6.1 ms).

## Video (5 WebM, 1080p VP8/VP9, 12–17 s)

`ffmpegthumbnailer -i x.webm -o out.png -s 256`: **65–87 ms** per file, clean. `ffmpeg -ss 1 … -frames:v 1`: 100–137 ms and VP8 decode errors when seeking to a non-keyframe. 432 videos ≈ 3 s on 16 workers.

## Recommended pipeline

- Decoder: **libvips in-process** (`libvips` crate) with `thumbnail` for WebP/JPEG/PNG/HEIF/JXL. Never shell out to `vips`. Pure-Rust `image` only as fallback, always paired with `fast_image_resize`. `ffmpegthumbnailer` for video.
- Workers: 16 (all threads) for `--warm`; 8–12 inside the app to leave headroom for the UI thread.
- Cache: PNG in the freedesktop layout, compression level 1, `Thumb::URI`/`Thumb::MTime` metadata. Read with `Gdk.Texture.new_from_filename` (0.46 ms) or in-process `png` → `MemoryTexture` (0.27 ms). If a private cache is ever wanted: JPEG Q85 (0.10 ms, 11 KB). Never WebP as cache format.
- Never route the tile read path through gdk-pixbuf or glycin on this distro.

## Projections

- Cold build of the full set: **~78 s warm cache, ~84 s cold** (34 988 images at 2.22 ms amortised over 16 workers) + 3 s video. CLI-per-file approach would be ~218 s.
- Warm scroll per visible tile: 0.27–0.46 ms of worker time. A 48-tile screen ≈ 13–22 ms off the UI thread.

Not measured: global page-cache drop (no root), GPU upload cost (no display in the harness), cold NVMe read of the cache itself.
