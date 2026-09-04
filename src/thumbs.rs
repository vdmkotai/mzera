//! Thumbnail service: freedesktop cache, libvips in-process decode, worker pool
//! with a viewport-priority queue, results streamed back to the UI thread.

use crate::scan::Kind;
use gtk::{gdk, gio, glib, prelude::*};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

pub struct Job {
    pub idx: u32,
    pub path: PathBuf,
    pub kind: Kind,
    /// 256 or 512
    pub size: u32,
    /// lower = sooner
    pub prio: u32,
}

pub struct ThumbResult {
    pub idx: u32,
    pub generation: u64,
    pub size: u32,
    pub texture: Option<gdk::Texture>,
    pub texture_bytes: usize,
    pub duration: Option<f64>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct JobKey {
    generation: u64,
    idx: u32,
    size: u32,
}

struct QueuedJob {
    job: Job,
    generation: u64,
}

struct Queue {
    /// sorted by prio descending so pop() gives the lowest prio value first
    wanted: Vec<QueuedJob>,
    in_flight: HashSet<JobKey>,
    generation: u64,
    quit: bool,
}

struct Inner {
    q: Mutex<Queue>,
    cv: Condvar,
    tx: async_channel::Sender<ThumbResult>,
}

pub struct ThumbService {
    inner: Arc<Inner>,
    rx: async_channel::Receiver<ThumbResult>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

static VIPS: OnceLock<Result<libvips::VipsApp, String>> = OnceLock::new();

pub fn init_vips() -> Result<(), String> {
    VIPS.get_or_init(|| {
        let app = libvips::VipsApp::new("mzera", false).map_err(|error| error.to_string())?;
        app.concurrency_set(1);
        app.cache_set_max(0);
        app.cache_set_max_mem(0);
        Ok(app)
    })
    .as_ref()
    .map(|_| ())
    .map_err(Clone::clone)
}

impl ThumbService {
    pub fn new(n_workers: usize) -> ThumbService {
        let n_workers = n_workers.max(1);
        // Bound decoded pixel memory if the main loop is briefly busy.
        let (tx, rx) = async_channel::bounded(n_workers.saturating_mul(2));
        let inner = Arc::new(Inner {
            q: Mutex::new(Queue {
                wanted: Vec::new(),
                in_flight: HashSet::new(),
                generation: 0,
                quit: false,
            }),
            cv: Condvar::new(),
            tx,
        });
        let mut threads = Vec::new();
        for i in 0..n_workers {
            let inner = inner.clone();
            threads.push(
                std::thread::Builder::new()
                    .name(format!("mzera-thumb-{i}"))
                    .spawn(move || worker(inner))
                    .expect("spawn worker"),
            );
        }
        ThumbService { inner, rx, threads }
    }

    /// Replace the wanted list. Jobs already in flight are not duplicated.
    pub fn set_wanted(&self, jobs: Vec<Job>) {
        let mut q = lock_queue(&self.inner.q);
        let generation = q.generation;
        let mut jobs: Vec<QueuedJob> = jobs
            .into_iter()
            .filter(|job| {
                !q.in_flight.iter().any(|key| {
                    key.generation == generation && key.idx == job.idx && key.size >= job.size
                })
            })
            .map(|job| QueuedJob { job, generation })
            .collect();
        jobs.sort_by_key(|queued| std::cmp::Reverse(queued.job.prio));
        q.wanted = jobs;
        drop(q);
        self.inner.cv.notify_all();
    }

    pub fn clear(&self) {
        let mut q = lock_queue(&self.inner.q);
        q.wanted.clear();
    }

    pub fn set_generation(&self, g: u64) {
        let mut q = lock_queue(&self.inner.q);
        q.generation = g;
        q.wanted.clear();
    }

    pub fn try_recv(&self) -> Option<ThumbResult> {
        self.rx.try_recv().ok()
    }

    pub fn results_pending(&self) -> bool {
        !self.rx.is_empty()
    }

    pub fn receiver(&self) -> async_channel::Receiver<ThumbResult> {
        self.rx.clone()
    }

    pub fn pending(&self) -> usize {
        let q = lock_queue(&self.inner.q);
        q.wanted.len() + q.in_flight.len()
    }

    pub fn request_shutdown(&self) {
        {
            let mut q = lock_queue(&self.inner.q);
            q.quit = true;
            q.wanted.clear();
        }
        self.inner.tx.close();
        self.inner.cv.notify_all();
    }

    pub fn shutdown(mut self) {
        self.request_shutdown();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

impl Drop for ThumbService {
    fn drop(&mut self) {
        self.request_shutdown();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn lock_queue(mutex: &Mutex<Queue>) -> std::sync::MutexGuard<'_, Queue> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn worker(inner: Arc<Inner>) {
    loop {
        let queued = {
            let mut q = lock_queue(&inner.q);
            loop {
                if q.quit {
                    return;
                }
                if let Some(queued) = q.wanted.pop() {
                    q.in_flight.insert(JobKey {
                        generation: queued.generation,
                        idx: queued.job.idx,
                        size: queued.job.size,
                    });
                    break queued;
                }
                q = inner
                    .cv
                    .wait(q)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        let job = queued.job;
        let generation = queued.generation;
        let (texture, texture_bytes, duration, error) = match produce(&job.path, job.kind, job.size)
        {
            Ok(t) => (Some(t.texture), t.texture_bytes, t.duration, None),
            Err(e) => (None, 0, None, Some(e)),
        };
        {
            let mut q = lock_queue(&inner.q);
            q.in_flight.remove(&JobKey {
                generation,
                idx: job.idx,
                size: job.size,
            });
        }
        let _ = inner.tx.send_blocking(ThumbResult {
            idx: job.idx,
            generation,
            size: job.size,
            texture,
            texture_bytes,
            duration,
            error,
        });
    }
}

pub struct Produced {
    pub texture: gdk::Texture,
    pub texture_bytes: usize,
    pub duration: Option<f64>,
}

struct Pixels {
    w: u32,
    h: u32,
    alpha: bool,
    data: Vec<u8>,
}

impl Pixels {
    fn texture(self) -> gdk::Texture {
        let bpp = if self.alpha { 4 } else { 3 };
        let bytes = glib::Bytes::from_owned(self.data);
        let fmt = if self.alpha {
            gdk::MemoryFormat::R8g8b8a8
        } else {
            gdk::MemoryFormat::R8g8b8
        };
        gdk::MemoryTexture::new(
            self.w as i32,
            self.h as i32,
            fmt,
            &bytes,
            (self.w * bpp) as usize,
        )
        .upcast()
    }
}

fn cache_root() -> PathBuf {
    glib::user_cache_dir().join("thumbnails")
}

fn cache_dir(size: u32) -> PathBuf {
    cache_root().join(if size > 256 { "x-large" } else { "large" })
}

fn fail_dir() -> PathBuf {
    cache_root().join("fail").join("mzera")
}

pub fn uri_for(path: &Path) -> String {
    gio::File::for_path(path).uri().to_string()
}

fn md5_hex(uri: &str) -> String {
    format!("{:x}", md5::compute(uri.as_bytes()))
}

#[derive(Clone, Copy)]
struct FileStamp {
    mtime: i64,
    size: u64,
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        mtime: metadata.mtime(),
        size: metadata.len(),
    })
}

/// Read a cached PNG; returns None when missing or stale.
fn read_cached(cache: &Path, uri: &str, stamp: FileStamp, max_dimension: u32) -> Option<Pixels> {
    let f = std::fs::File::open(cache).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let info = reader.info();
    let mut cached_uri = None;
    let mut cached_mtime = None;
    let mut cached_size = None;
    for t in &info.uncompressed_latin1_text {
        match t.keyword.as_str() {
            "Thumb::URI" => cached_uri = Some(t.text.as_str()),
            "Thumb::MTime" => cached_mtime = t.text.trim().parse::<i64>().ok(),
            "Thumb::Size" => cached_size = t.text.trim().parse::<u64>().ok(),
            _ => {}
        }
    }
    if cached_uri != Some(uri)
        || cached_mtime != Some(stamp.mtime)
        || cached_size != Some(stamp.size)
        || info.width == 0
        || info.height == 0
        || info.width > max_dimension
        || info.height > max_dimension
    {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let out = reader.next_frame(&mut buf).ok()?;
    let (ct, _) = reader.output_color_type();
    let (w, h) = (out.width, out.height);
    buf.truncate(out.buffer_size());
    let (alpha, data) = match ct {
        png::ColorType::Rgb => (false, buf),
        png::ColorType::Rgba => (true, buf),
        png::ColorType::Grayscale => (false, buf.iter().flat_map(|&g| [g, g, g]).collect()),
        png::ColorType::GrayscaleAlpha => (
            true,
            buf.chunks(2)
                .flat_map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
        ),
        png::ColorType::Indexed => return None,
    };
    Some(Pixels { w, h, alpha, data })
}

fn write_cached(
    cache: &Path,
    px: &Pixels,
    uri: &str,
    mtime: i64,
    fsize: Option<u64>,
) -> std::io::Result<()> {
    let Some(dir) = cache.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "thumbnail path has no parent",
        ));
    };
    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut tmp = tempfile::Builder::new()
        .prefix(".mzera-")
        .tempfile_in(dir)?;
    {
        let mut w = std::io::BufWriter::new(tmp.as_file_mut());
        let mut enc = png::Encoder::new(&mut w, px.w, px.h);
        enc.set_color(if px.alpha {
            png::ColorType::Rgba
        } else {
            png::ColorType::Rgb
        });
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let _ = enc.add_text_chunk("Thumb::URI".into(), uri.to_string());
        let _ = enc.add_text_chunk("Thumb::MTime".into(), mtime.to_string());
        if let Some(s) = fsize {
            let _ = enc.add_text_chunk("Thumb::Size".into(), s.to_string());
        }
        let _ = enc.add_text_chunk("Software".into(), "mzera".into());
        let mut writer = enc.write_header().map_err(std::io::Error::other)?;
        writer
            .write_image_data(&px.data)
            .map_err(std::io::Error::other)?;
        writer.finish().map_err(std::io::Error::other)?;
        w.flush()?;
    }
    tmp.persist(cache).map(|_| ()).map_err(|e| e.error)
}

fn vips_thumbnail(path: &Path, size: u32) -> Result<Pixels, String> {
    use libvips::{VipsImage, ops};
    let p = path.to_str().ok_or("non-utf8 path")?;
    // header only: pick the target width so the result fits in size×size
    let header = VipsImage::new_from_file(p).map_err(|e| e.to_string())?;
    let (w, h) = (header.get_width().max(1), header.get_height().max(1));
    drop(header);
    let target_w = if w >= h {
        size as i32
    } else {
        ((size as i64 * w as i64) / h as i64).max(1) as i32
    };
    let target_w = target_w.min(w); // never upscale
    let img = ops::thumbnail(p, target_w).map_err(|e| e.to_string())?;
    // normalise to sRGB 8-bit, 3 or 4 bands
    let img = ops::colourspace(&img, ops::Interpretation::Srgb).map_err(|e| e.to_string())?;
    let img = if img
        .get_format()
        .map(|f| f as i32 != ops::BandFormat::Uchar as i32)
        .unwrap_or(true)
    {
        ops::cast(&img, ops::BandFormat::Uchar).map_err(|e| e.to_string())?
    } else {
        img
    };
    let bands = img.get_bands();
    let img = match bands {
        3 | 4 => img,
        1 => {
            let mut channels = [
                ops::extract_band(&img, 0).map_err(|e| e.to_string())?,
                ops::extract_band(&img, 0).map_err(|e| e.to_string())?,
                ops::extract_band(&img, 0).map_err(|e| e.to_string())?,
            ];
            ops::bandjoin(&mut channels).map_err(|e| e.to_string())?
        }
        2 => {
            let mut channels = [
                ops::extract_band(&img, 0).map_err(|e| e.to_string())?,
                ops::extract_band(&img, 0).map_err(|e| e.to_string())?,
                ops::extract_band(&img, 0).map_err(|e| e.to_string())?,
                ops::extract_band(&img, 1).map_err(|e| e.to_string())?,
            ];
            ops::bandjoin(&mut channels).map_err(|e| e.to_string())?
        }
        n if n > 4 => ops::extract_band_with_opts(&img, 0, &ops::ExtractBandOptions { n: 3 })
            .map_err(|e| e.to_string())?,
        _ => return Err(format!("unsupported band count {bands}")),
    };
    let bands = img.get_bands();
    let (w, h) = (img.get_width() as u32, img.get_height() as u32);
    let data = img.image_write_to_memory();
    let bpp = bands as usize;
    let expected = (w as usize)
        .checked_mul(h as usize)
        .and_then(|pixels| pixels.checked_mul(bpp))
        .ok_or("pixel buffer overflow")?;
    if data.len() < expected {
        return Err("short pixel buffer".into());
    }
    Ok(Pixels {
        w,
        h,
        alpha: bands == 4,
        data: data[..expected].to_vec(),
    })
}

fn ffmpeg_thumbnail(path: &Path, size: u32) -> Result<(Pixels, Option<f64>), String> {
    let tmp = tempfile::Builder::new()
        .prefix("mzera-video-")
        .suffix(".png")
        .tempfile()
        .map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new("ffmpegthumbnailer");
    command
        .args(["-i"])
        .arg(path)
        .arg("-o")
        .arg(tmp.path())
        .args(["-s", &size.to_string(), "-q", "8", "-a"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().map_err(|e| {
        format!("temporary thumbnail failure: could not start ffmpegthumbnailer: {e}")
    })?;
    let st = wait_with_timeout(&mut child, std::time::Duration::from_secs(15))?;
    if !st.success() {
        return Err("ffmpegthumbnailer failed".into());
    }
    let px = read_png_any(tmp.path(), size);
    let px = px.ok_or("bad png from ffmpegthumbnailer")?;
    Ok((px, probe_duration(path)))
}

fn probe_duration(path: &Path) -> Option<f64> {
    let mut command = std::process::Command::new("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().ok()?;
    let status = wait_with_timeout(&mut child, std::time::Duration::from_secs(5)).ok()?;
    if !status.success() {
        return None;
    }
    let mut stdout = Vec::new();
    child.stdout.take()?.read_to_end(&mut stdout).ok()?;
    let s = String::from_utf8_lossy(&stdout);
    s.trim().parse::<f64>().ok()
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> Result<std::process::ExitStatus, String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "temporary thumbnail failure: helper timed out after {} seconds",
                    timeout.as_secs()
                ));
            }
            Err(error) => return Err(format!("temporary thumbnail failure: {error}")),
        }
    }
}

fn read_png_any(p: &Path, max_dimension: u32) -> Option<Pixels> {
    let f = std::fs::File::open(p).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    if reader.info().width == 0
        || reader.info().height == 0
        || reader.info().width > max_dimension
        || reader.info().height > max_dimension
    {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let out = reader.next_frame(&mut buf).ok()?;
    let (ct, _) = reader.output_color_type();
    buf.truncate(out.buffer_size());
    let (alpha, data) = match ct {
        png::ColorType::Rgb => (false, buf),
        png::ColorType::Rgba => (true, buf),
        png::ColorType::Grayscale => (false, buf.iter().flat_map(|&g| [g, g, g]).collect()),
        png::ColorType::GrayscaleAlpha => (
            true,
            buf.chunks(2)
                .flat_map(|c| [c[0], c[0], c[0], c[1]])
                .collect(),
        ),
        png::ColorType::Indexed => return None,
    };
    Some(Pixels {
        w: out.width,
        h: out.height,
        alpha,
        data,
    })
}

/// Produce a thumbnail texture for `path`, using and filling the freedesktop cache.
pub fn produce(path: &Path, kind: Kind, size: u32) -> Result<Produced, String> {
    if size == 0 || size > 4096 {
        return Err(format!("invalid thumbnail size {size}"));
    }
    let uri = uri_for(path);
    let hash = md5_hex(&uri);
    let cache = cache_dir(size).join(format!("{hash}.png"));
    let stamp = file_stamp(path).ok_or("stat failed")?;
    if let Some(px) = read_cached(&cache, &uri, stamp, size) {
        let duration = if kind == Kind::Video {
            probe_duration(path)
        } else {
            None
        };
        let texture_bytes = px.data.len();
        return Ok(Produced {
            texture: px.texture(),
            texture_bytes,
            duration,
        });
    }
    let fail = fail_dir().join(format!("{hash}.png"));
    if read_cached(&fail, &uri, stamp, 1).is_some() {
        return Err("previously failed".into());
    }
    let res = match kind {
        Kind::Image => {
            init_vips()?;
            vips_thumbnail(path, size).map(|px| (px, None))
        }
        Kind::Video => ffmpeg_thumbnail(path, size),
    };
    match res {
        Ok((px, duration)) => {
            let _ = write_cached(&cache, &px, &uri, stamp.mtime, Some(stamp.size));
            let texture_bytes = px.data.len();
            Ok(Produced {
                texture: px.texture(),
                texture_bytes,
                duration,
            })
        }
        Err(e) => {
            if e.starts_with("temporary thumbnail failure:") {
                return Err(e);
            }
            let marker = Pixels {
                w: 1,
                h: 1,
                alpha: false,
                data: vec![0, 0, 0],
            };
            let _ = write_cached(&fail, &marker, &uri, stamp.mtime, Some(stamp.size));
            Err(e)
        }
    }
}

/// `mzera --warm <dir>`: build the cache for a whole tree with all cores.
pub fn warm(root: &Path, size: u32) -> i32 {
    if !root.is_dir() {
        eprintln!("mzera --warm: not a directory: {}", root.display());
        return 2;
    }
    if size == 0 || size > 4096 {
        eprintln!("mzera --warm: --size must be between 1 and 4096");
        return 2;
    }
    let t0 = std::time::Instant::now();
    let entries = crate::scan::scan_blocking(root);
    let n = entries.len();
    eprintln!("mzera --warm: {n} files under {}", root.display());
    if n == 0 {
        return 0;
    }
    let workers = std::thread::available_parallelism()
        .map(|x| x.get())
        .unwrap_or(8);
    let svc = ThumbService::new(workers);
    let jobs: Vec<Job> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| Job {
            idx: i as u32,
            path: e.path.clone(),
            kind: e.kind(),
            size,
            prio: i as u32,
        })
        .collect();
    svc.set_wanted(jobs);
    let (mut done, mut failed) = (0usize, 0usize);
    let rx = svc.receiver();
    while done < n {
        match rx.recv_blocking() {
            Ok(r) => {
                done += 1;
                if r.texture.is_none() {
                    failed += 1;
                    if failed <= 5 {
                        eprintln!(
                            "  fail: {} — {}",
                            entries[r.idx as usize].path.display(),
                            r.error.unwrap_or_default()
                        );
                    }
                }
                if done % 500 == 0 || done == n {
                    eprintln!(
                        "  {done}/{n}  ({failed} failed)  {:.1}s",
                        t0.elapsed().as_secs_f64()
                    );
                }
            }
            Err(_) => break,
        }
    }
    svc.shutdown();
    eprintln!(
        "done: {done} thumbnails, {failed} failed, {:.1}s",
        t0.elapsed().as_secs_f64()
    );
    i32::from(failed > 0)
}

/// Read one file's byte size and mtime for captions.
pub fn file_info(path: &Path) -> Option<(u64, i64)> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(path).ok()?;
    Some((m.len(), m.mtime()))
}

pub fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / 1024.0 / 1024.0)
    } else {
        format!("{:.2} GB", b / 1024.0 / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_png_requires_matching_identity_and_stamp() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let cache = directory.path().join("thumbnail.png");
        let pixels = Pixels {
            w: 2,
            h: 1,
            alpha: false,
            data: vec![1, 2, 3, 4, 5, 6],
        };
        let stamp = FileStamp { mtime: 42, size: 6 };
        write_cached(
            &cache,
            &pixels,
            "file:///image.png",
            stamp.mtime,
            Some(stamp.size),
        )
        .expect("write thumbnail");

        let decoded =
            read_cached(&cache, "file:///image.png", stamp, 2).expect("valid cached thumbnail");
        assert_eq!((decoded.w, decoded.h, decoded.data), (2, 1, pixels.data));
        assert!(read_cached(&cache, "file:///other.png", stamp, 2).is_none());
        assert!(
            read_cached(
                &cache,
                "file:///image.png",
                FileStamp { size: 7, ..stamp },
                2,
            )
            .is_none()
        );
        assert!(read_cached(&cache, "file:///image.png", stamp, 1).is_none());
    }

    #[test]
    fn grayscale_thumbnail_is_expanded_to_neutral_rgb() {
        init_vips().expect("libvips initialization");
        let directory = tempfile::tempdir().expect("temporary directory");
        let source = directory.path().join("gray.png");
        {
            let file = std::fs::File::create(&source).expect("source image");
            let mut encoder = png::Encoder::new(file, 2, 1);
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG header");
            writer.write_image_data(&[64, 192]).expect("PNG pixels");
        }

        let pixels = vips_thumbnail(&source, 2).expect("vips thumbnail");
        assert!(!pixels.alpha);
        assert_eq!((pixels.w, pixels.h), (2, 1));
        assert_eq!(pixels.data.len(), 6);
        assert_eq!(pixels.data[0], pixels.data[1]);
        assert_eq!(pixels.data[1], pixels.data[2]);
        assert_eq!(pixels.data[3], pixels.data[4]);
        assert_eq!(pixels.data[4], pixels.data[5]);
    }

    #[test]
    fn human_sizes_are_stable_at_unit_boundaries() {
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
    }
}
