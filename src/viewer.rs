//! ImageView: custom widget drawing one texture with scale modes, zoom, pan and rotation.
//! Decoder: glycin with a shared process pool, preload ring, byte-capped LRU.

use gtk::{gdk, gio, glib, graphene, gsk, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    Fit,
    Fill,
    Width,
    Height,
    Real,
    Optimal,
}

pub const SCALE_MODES: [ScaleMode; 6] = [
    ScaleMode::Fit,
    ScaleMode::Fill,
    ScaleMode::Width,
    ScaleMode::Height,
    ScaleMode::Real,
    ScaleMode::Optimal,
];

impl ScaleMode {
    pub fn label(self) -> &'static str {
        match self {
            ScaleMode::Fit => "Fit",
            ScaleMode::Fill => "Fill",
            ScaleMode::Width => "Width",
            ScaleMode::Height => "Height",
            ScaleMode::Real => "1:1",
            ScaleMode::Optimal => "Optimal",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            ScaleMode::Fit => "fit",
            ScaleMode::Fill => "fill",
            ScaleMode::Width => "width",
            ScaleMode::Height => "height",
            ScaleMode::Real => "real",
            ScaleMode::Optimal => "optimal",
        }
    }
    pub fn from_id(s: &str) -> ScaleMode {
        SCALE_MODES
            .iter()
            .copied()
            .find(|m| m.id() == s)
            .unwrap_or(ScaleMode::Fit)
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ImageView {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub mode: Cell<Option<ScaleMode>>,
        pub zoom: Cell<Option<f64>>,
        pub pan: Cell<(f64, f64)>,
        pub rotation: Cell<i32>,
        pub antialias: Cell<bool>,
        pub black: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ImageView {
        const NAME: &'static str = "MzImageView";
        type Type = super::ImageView;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for ImageView {
        fn constructed(&self) {
            self.parent_constructed();
            self.mode.set(Some(ScaleMode::Fit));
            self.antialias.set(true);
            self.black.set(true);
            self.obj().set_hexpand(true);
            self.obj().set_vexpand(true);
        }
    }

    impl WidgetImpl for ImageView {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }
}

glib::wrapper! {
    pub struct ImageView(ObjectSubclass<imp::ImageView>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ImageView {
    fn default() -> Self {
        Self::new()
    }
}

struct Geometry {
    scale: f64,
    /// drawn size after rotation
    dw: f64,
    dh: f64,
    x: f64,
    y: f64,
}

impl ImageView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_texture(&self, t: Option<gdk::Texture>) {
        *self.imp().texture.borrow_mut() = t;
        self.imp().zoom.set(None);
        self.imp().pan.set((0.0, 0.0));
        self.imp().rotation.set(0);
        self.queue_draw();
    }

    pub fn texture(&self) -> Option<gdk::Texture> {
        self.imp().texture.borrow().clone()
    }

    pub fn mode(&self) -> ScaleMode {
        self.imp().mode.get().unwrap_or(ScaleMode::Fit)
    }

    pub fn set_mode(&self, m: ScaleMode) {
        self.imp().mode.set(Some(m));
        self.imp().zoom.set(None);
        self.imp().pan.set((0.0, 0.0));
        self.queue_draw();
    }

    pub fn cycle_mode(&self, dir: i32) -> ScaleMode {
        let i = SCALE_MODES
            .iter()
            .position(|&m| m == self.mode())
            .unwrap_or(0) as i32;
        let n = SCALE_MODES.len() as i32;
        let m = SCALE_MODES[((i + dir) % n + n) as usize % n as usize];
        self.set_mode(m);
        m
    }

    pub fn set_antialias(&self, on: bool) {
        self.imp().antialias.set(on);
        self.queue_draw();
    }
    pub fn antialias(&self) -> bool {
        self.imp().antialias.get()
    }

    pub fn set_black(&self, b: bool) {
        self.imp().black.set(b);
        self.queue_draw();
    }

    pub fn rotate(&self, deg: i32) {
        let r = ((self.imp().rotation.get() + deg) % 360 + 360) % 360;
        self.imp().rotation.set(r);
        self.imp().pan.set((0.0, 0.0));
        self.queue_draw();
    }

    fn image_size(&self) -> Option<(f64, f64)> {
        let t = self.imp().texture.borrow();
        let t = t.as_ref()?;
        let (w, h) = (t.width() as f64, t.height() as f64);
        let r = self.imp().rotation.get();
        Some(if r == 90 || r == 270 { (h, w) } else { (w, h) })
    }

    fn mode_scale(&self, iw: f64, ih: f64) -> f64 {
        let (w, h) = (self.width() as f64, self.height() as f64);
        if iw <= 0.0 || ih <= 0.0 || w <= 0.0 || h <= 0.0 {
            return 1.0;
        }
        let fit = (w / iw).min(h / ih);
        match self.mode() {
            ScaleMode::Fit => fit,
            ScaleMode::Fill => (w / iw).max(h / ih),
            ScaleMode::Width => w / iw,
            ScaleMode::Height => h / ih,
            ScaleMode::Real => 1.0,
            ScaleMode::Optimal => fit.min(1.0),
        }
    }

    fn geometry(&self) -> Option<Geometry> {
        let (iw, ih) = self.image_size()?;
        let (w, h) = (self.width() as f64, self.height() as f64);
        let scale = self
            .imp()
            .zoom
            .get()
            .unwrap_or_else(|| self.mode_scale(iw, ih));
        let (dw, dh) = (iw * scale, ih * scale);
        let (px, py) = self.imp().pan.get();
        let x = if dw <= w {
            (w - dw) / 2.0
        } else {
            ((w - dw) / 2.0 + px).clamp(w - dw, 0.0)
        };
        let y = if dh <= h {
            (h - dh) / 2.0
        } else {
            ((h - dh) / 2.0 + py).clamp(h - dh, 0.0)
        };
        Some(Geometry {
            scale,
            dw,
            dh,
            x,
            y,
        })
    }

    pub fn effective_scale(&self) -> f64 {
        self.geometry().map(|g| g.scale).unwrap_or(1.0)
    }

    pub fn overflows(&self) -> bool {
        match self.geometry() {
            Some(g) => g.dw > self.width() as f64 + 0.5 || g.dh > self.height() as f64 + 0.5,
            None => false,
        }
    }

    pub fn is_zoomed(&self) -> bool {
        self.imp().zoom.get().is_some()
    }

    pub fn pan_by(&self, dx: f64, dy: f64) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        let (px, py) = self.imp().pan.get();
        self.imp().pan.set((px + dx, py + dy));
        self.normalize_pan();
        self.queue_draw();
    }

    fn normalize_pan(&self) {
        // keep pan within the range that geometry() would clamp to
        if let Some(g) = self.geometry() {
            let (w, h) = (self.width() as f64, self.height() as f64);
            let px = if g.dw <= w {
                0.0
            } else {
                g.x - (w - g.dw) / 2.0
            };
            let py = if g.dh <= h {
                0.0
            } else {
                g.y - (h - g.dh) / 2.0
            };
            self.imp().pan.set((px, py));
        }
    }

    /// Zoom by `factor` keeping the image point under (ax, ay) fixed (widget coords).
    pub fn zoom_by(&self, factor: f64, around: Option<(f64, f64)>) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let Some(g) = self.geometry() else { return };
        let new_scale = (g.scale * factor).clamp(0.02, 64.0);
        self.set_zoom(new_scale, around);
    }

    pub fn set_zoom(&self, new_scale: f64, around: Option<(f64, f64)>) {
        if !new_scale.is_finite() || new_scale <= 0.0 {
            return;
        }
        let new_scale = new_scale.clamp(0.02, 64.0);
        let Some(g) = self.geometry() else { return };
        let (w, h) = (self.width() as f64, self.height() as f64);
        let (ax, ay) = around
            .filter(|(x, y)| x.is_finite() && y.is_finite())
            .unwrap_or((w / 2.0, h / 2.0));
        let ix = (ax - g.x) / g.scale;
        let iy = (ay - g.y) / g.scale;
        let (iw, ih) = self.image_size().unwrap();
        let (dw, dh) = (iw * new_scale, ih * new_scale);
        let nx = ax - ix * new_scale;
        let ny = ay - iy * new_scale;
        self.imp().zoom.set(Some(new_scale));
        self.imp()
            .pan
            .set((nx - (w - dw) / 2.0, ny - (h - dh) / 2.0));
        self.normalize_pan();
        self.queue_draw();
    }

    pub fn reset_zoom(&self) {
        self.imp().zoom.set(None);
        self.imp().pan.set((0.0, 0.0));
        self.queue_draw();
    }

    /// Toggle between the mode scale and 1:1 around a point.
    pub fn toggle_real(&self, around: Option<(f64, f64)>) {
        let Some((iw, ih)) = self.image_size() else {
            return;
        };
        let ms = self.mode_scale(iw, ih);
        let cur = self.effective_scale();
        if (cur - 1.0).abs() < 0.001 && (ms - 1.0).abs() > 0.001 {
            self.reset_zoom();
        } else {
            self.set_zoom(1.0, around);
        }
    }

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let (w, h) = (self.width() as f32, self.height() as f32);
        let bg = if self.imp().black.get() {
            gdk::RGBA::BLACK
        } else {
            crate::theme::colors().bg_darker
        };
        snapshot.append_color(&bg, &graphene::Rect::new(0.0, 0.0, w, h));
        let tex = self.imp().texture.borrow().clone();
        let Some(tex) = tex else { return };
        let Some(g) = self.geometry() else { return };
        let filter = if !self.imp().antialias.get() {
            gsk::ScalingFilter::Nearest
        } else if g.scale < 1.0 {
            gsk::ScalingFilter::Trilinear
        } else {
            gsk::ScalingFilter::Linear
        };
        let rot = self.imp().rotation.get();
        // snap to device pixels for crisp rendering
        let (x, y) = (g.x.round() as f32, g.y.round() as f32);
        snapshot.save();
        if rot == 0 {
            snapshot.append_scaled_texture(
                &tex,
                filter,
                &graphene::Rect::new(x, y, g.dw as f32, g.dh as f32),
            );
        } else {
            let (cx, cy) = (x + g.dw as f32 / 2.0, y + g.dh as f32 / 2.0);
            snapshot.translate(&graphene::Point::new(cx, cy));
            snapshot.rotate(rot as f32);
            // unrotated draw size
            let (uw, uh) = if rot == 90 || rot == 270 {
                (g.dh as f32, g.dw as f32)
            } else {
                (g.dw as f32, g.dh as f32)
            };
            snapshot.append_scaled_texture(
                &tex,
                filter,
                &graphene::Rect::new(-uw / 2.0, -uh / 2.0, uw, uh),
            );
        }
        snapshot.restore();
    }
}

// ---------------- decoder ----------------

const CACHE_BYTES: usize = 1024 * 1024 * 1024;

struct InFlight {
    request_id: u64,
    cancellable: gio::Cancellable,
}

fn remove_inflight_if_current(
    inflight: &mut HashMap<u32, InFlight>,
    idx: u32,
    request_id: u64,
) -> bool {
    if !inflight
        .get(&idx)
        .is_some_and(|request| request.request_id == request_id)
    {
        return false;
    }
    inflight.remove(&idx);
    true
}

struct Cached {
    texture: gdk::Texture,
    bytes: usize,
    tick: u64,
}

type ReadyCallback = Rc<dyn Fn(u32, Option<gdk::Texture>)>;

pub struct Decoder {
    pool: Arc<glycin::Pool>,
    cache: RefCell<HashMap<u32, Cached>>,
    total: Cell<usize>,
    tick: Cell<u64>,
    inflight: RefCell<HashMap<u32, InFlight>>,
    next_request_id: Cell<u64>,
    on_ready: RefCell<Option<ReadyCallback>>,
    generation: Cell<u64>,
}

impl Decoder {
    pub fn new() -> Rc<Decoder> {
        let mut cfg = glycin::PoolConfig::new();
        cfg.max_parallel_operations(4);
        let pool = glycin::Pool::new(cfg);
        Rc::new(Decoder {
            pool,
            cache: RefCell::new(HashMap::new()),
            total: Cell::new(0),
            tick: Cell::new(0),
            inflight: RefCell::new(HashMap::new()),
            next_request_id: Cell::new(0),
            on_ready: RefCell::new(None),
            generation: Cell::new(0),
        })
    }

    pub fn set_on_ready(&self, f: impl Fn(u32, Option<gdk::Texture>) + 'static) {
        *self.on_ready.borrow_mut() = Some(Rc::new(f));
    }

    pub fn set_generation(&self, g: u64) {
        self.generation.set(g);
        self.clear();
    }

    pub fn clear(&self) {
        for (_, request) in self.inflight.borrow_mut().drain() {
            request.cancellable.cancel();
        }
        self.cache.borrow_mut().clear();
        self.total.set(0);
    }

    pub fn get(&self, idx: u32) -> Option<gdk::Texture> {
        let mut c = self.cache.borrow_mut();
        let t = self.tick.get() + 1;
        self.tick.set(t);
        c.get_mut(&idx).map(|e| {
            e.tick = t;
            e.texture.clone()
        })
    }

    fn insert_with_bytes(&self, idx: u32, tex: gdk::Texture, bytes: usize) {
        let t = self.tick.get() + 1;
        self.tick.set(t);
        let mut c = self.cache.borrow_mut();
        if let Some(old) = c.insert(
            idx,
            Cached {
                texture: tex,
                bytes,
                tick: t,
            },
        ) {
            self.total.set(self.total.get() - old.bytes);
        }
        self.total.set(self.total.get() + bytes);
        while self.total.get() > CACHE_BYTES && c.len() > 1 {
            let oldest = c
                .iter()
                .min_by_key(|(_, e)| e.tick)
                .map(|(k, _)| *k)
                .unwrap();
            if let Some(e) = c.remove(&oldest) {
                self.total.set(self.total.get() - e.bytes);
            }
        }
    }

    /// Cancel in-flight loads not in `keep`.
    pub fn cancel_except(&self, keep: &[u32]) {
        let mut inf = self.inflight.borrow_mut();
        let drop_keys: Vec<u32> = inf.keys().filter(|k| !keep.contains(k)).copied().collect();
        for k in drop_keys {
            if let Some(request) = inf.remove(&k) {
                request.cancellable.cancel();
            }
        }
    }

    pub fn request(self: &Rc<Self>, idx: u32, path: PathBuf) {
        if self.cache.borrow().contains_key(&idx) {
            if let Some(f) = self.on_ready.borrow().clone() {
                f(idx, self.get(idx));
            }
            return;
        }
        if self.inflight.borrow().contains_key(&idx) {
            return;
        }
        let cancellable = gio::Cancellable::new();
        let request_id = self.next_request_id.get().wrapping_add(1);
        self.next_request_id.set(request_id);
        self.inflight.borrow_mut().insert(
            idx,
            InFlight {
                request_id,
                cancellable: cancellable.clone(),
            },
        );
        let this = self.clone();
        let generation = self.generation.get();
        glib::spawn_future_local(async move {
            let file = gio::File::for_path(&path);
            let mut loader = glycin::Loader::new(file);
            loader.pool(this.pool.clone());
            loader.cancellable(cancellable.clone());
            loader.accepted_memory_formats(
                glycin::MemoryFormatSelection::R8g8b8 | glycin::MemoryFormatSelection::R8g8b8a8,
            );
            let result: Option<(gdk::Texture, usize)> = match loader.load().await {
                Ok(image) => {
                    let details = image.details();
                    let max_bytes = (details.width() as usize)
                        .checked_mul(details.height() as usize)
                        .and_then(|pixels| pixels.checked_mul(4));
                    if max_bytes.is_none_or(|bytes| bytes > CACHE_BYTES) {
                        eprintln!(
                            "mzera: image is too large to decode safely: {} ({}x{})",
                            path.display(),
                            details.width(),
                            details.height()
                        );
                        None
                    } else {
                        match image.next_frame().await {
                            Ok(frame) => {
                                let bytes = frame.buf_bytes().len();
                                (bytes <= CACHE_BYTES).then(|| (frame.texture(), bytes))
                            }
                            Err(e) => {
                                eprintln!("mzera: frame error for {}: {e}", path.display());
                                None
                            }
                        }
                    }
                }
                Err(e) => {
                    if !cancellable.is_cancelled() {
                        eprintln!("mzera: decode error for {}: {e}", path.display());
                    }
                    None
                }
            };
            let is_current =
                remove_inflight_if_current(&mut this.inflight.borrow_mut(), idx, request_id);
            if !is_current || cancellable.is_cancelled() || this.generation.get() != generation {
                return;
            }
            if let Some((texture, bytes)) = &result {
                this.insert_with_bytes(idx, texture.clone(), *bytes);
            }
            if let Some(f) = this.on_ready.borrow().clone() {
                f(idx, result.map(|(texture, _)| texture));
            }
        });
    }
}

#[cfg(test)]
mod decoder_tests {
    use super::*;

    #[test]
    fn stale_completion_cannot_remove_replacement_request() {
        let mut inflight = HashMap::new();
        inflight.insert(
            7,
            InFlight {
                request_id: 2,
                cancellable: gio::Cancellable::new(),
            },
        );

        assert!(!remove_inflight_if_current(&mut inflight, 7, 1));
        assert_eq!(inflight.get(&7).map(|request| request.request_id), Some(2));
        assert!(remove_inflight_if_current(&mut inflight, 7, 2));
        assert!(!inflight.contains_key(&7));
    }
}
