//! GalleryView: a custom scrollable widget that lays out fixed-size tiles itself
//! (flat or grouped by folder with sticky headers) and draws textures directly.

use crate::model::Model;
use crate::scan::Kind;
use crate::theme;
use crate::thumbs::{Job, ThumbService};
use gtk::{gdk, glib, graphene, gsk, pango, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub const TILE_STEPS: [i32; 9] = [96, 128, 160, 200, 240, 280, 340, 400, 512];
pub const DEFAULT_TILE_STEP: usize = 4;
const GAP: i32 = 6;
const PAD: i32 = 12;
const HEADER_H: i32 = 30;
const SECTION_GAP: i32 = 10;
const TOP_PAD: i32 = 48; // room for the floating pill
const BOTTOM_PAD: i32 = 34; // room for the status bar
const MAX_TEXTURE_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Tile(u32),
    Header(usize),
    Empty,
}

#[derive(Default, Clone)]
pub struct Layout {
    width: i32,
    height: i32,
    cols: i32,
    tile: i32,
    grouped: bool,
    n: u32,
    /// grouped: y of each section's header
    sec_y: Vec<i32>,
    total_h: i32,
}

impl Layout {
    fn stride(&self) -> i32 {
        self.tile + GAP
    }
    fn section_h(&self, len: u32) -> i32 {
        let rows = (len as i32 + self.cols - 1) / self.cols;
        HEADER_H + rows * self.stride() + SECTION_GAP
    }
}

struct CachedTile {
    texture: gdk::Texture,
    size: u32,
    bytes: usize,
}

pub struct TileCache {
    textures: HashMap<u32, CachedTile>,
    total_bytes: usize,
    pub durations: HashMap<u32, f64>,
    pub failed: HashSet<u32>,
}

type ActivateHandler = Box<dyn Fn(u32)>;
type VoidHandler = Box<dyn Fn()>;
type ContextHandler = Box<dyn Fn(f64, f64, Option<u32>)>;
type SideButtonHandler = Box<dyn Fn(bool)>;
type HeaderHandler = Box<dyn Fn(usize, u32)>;

mod imp {
    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::GalleryView)]
    pub struct GalleryView {
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_hadjustment, nullable)]
        pub hadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_vadjustment, nullable)]
        pub vadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(override_interface = gtk::Scrollable, get, set, builder(gtk::ScrollablePolicy::Minimum))]
        pub hscroll_policy: Cell<gtk::ScrollablePolicy>,
        #[property(override_interface = gtk::Scrollable, get, set, builder(gtk::ScrollablePolicy::Minimum))]
        pub vscroll_policy: Cell<gtk::ScrollablePolicy>,

        pub model: RefCell<Option<Rc<RefCell<Model>>>>,
        pub thumbs: RefCell<Option<Rc<ThumbService>>>,
        pub cache: RefCell<TileCache>,
        pub layout: RefCell<Layout>,
        pub grouped: Cell<bool>,
        pub tile_step: Cell<usize>,
        pub current: Cell<Option<u32>>,
        pub anchor: Cell<Option<u32>>,
        pub hover: Cell<Option<u32>>,
        pub rubber: RefCell<Option<(f64, f64, f64, f64)>>,
        pub rubber_base: RefCell<HashSet<u32>>,
        pub wanted_source: RefCell<Option<glib::SourceId>>,
        pub generation: Cell<u64>,
        pub last_top_section: Cell<Option<usize>>,

        pub on_activate: RefCell<Option<ActivateHandler>>,
        pub on_current: RefCell<Option<VoidHandler>>,
        pub on_context: RefCell<Option<ContextHandler>>,
        pub on_scroll: RefCell<Option<VoidHandler>>,
        pub on_side_button: RefCell<Option<SideButtonHandler>>,
        pub on_header: RefCell<Option<HeaderHandler>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GalleryView {
        const NAME: &'static str = "MzGalleryView";
        type Type = super::GalleryView;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Scrollable,);

        fn new() -> Self {
            GalleryView {
                hadjustment: RefCell::new(None),
                vadjustment: RefCell::new(None),
                hscroll_policy: Cell::new(gtk::ScrollablePolicy::Minimum),
                vscroll_policy: Cell::new(gtk::ScrollablePolicy::Minimum),
                model: RefCell::new(None),
                thumbs: RefCell::new(None),
                cache: RefCell::new(TileCache {
                    textures: HashMap::new(),
                    total_bytes: 0,
                    durations: HashMap::new(),
                    failed: HashSet::new(),
                }),
                layout: RefCell::new(Layout::default()),
                grouped: Cell::new(false),
                tile_step: Cell::new(DEFAULT_TILE_STEP),
                current: Cell::new(None),
                anchor: Cell::new(None),
                hover: Cell::new(None),
                rubber: RefCell::new(None),
                rubber_base: RefCell::new(HashSet::new()),
                wanted_source: RefCell::new(None),
                generation: Cell::new(0),
                last_top_section: Cell::new(None),
                on_activate: RefCell::new(None),
                on_current: RefCell::new(None),
                on_context: RefCell::new(None),
                on_scroll: RefCell::new(None),
                on_side_button: RefCell::new(None),
                on_header: RefCell::new(None),
            }
        }

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("mzgallery");
        }
    }

    impl GalleryView {
        fn set_hadjustment(&self, adj: Option<gtk::Adjustment>) {
            *self.hadjustment.borrow_mut() = adj;
        }
        fn set_vadjustment(&self, adj: Option<gtk::Adjustment>) {
            if let Some(a) = &adj {
                let obj = self.obj().downgrade();
                a.connect_value_changed(move |_| {
                    if let Some(o) = obj.upgrade() {
                        o.on_scrolled();
                    }
                });
            }
            *self.vadjustment.borrow_mut() = adj;
            self.obj().configure_adjustment();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for GalleryView {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_focusable(true);
            obj.set_can_focus(true);
            obj.set_has_tooltip(true);
            obj.setup_controllers();
        }
    }

    impl WidgetImpl for GalleryView {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    (TILE_STEPS[0] + 2 * PAD, TILE_STEPS[0] + 2 * PAD, -1, -1)
                }
                _ => (100, 100, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            self.obj().relayout(width, height);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.obj().draw(snapshot);
        }
    }

    impl ScrollableImpl for GalleryView {}
}

glib::wrapper! {
    pub struct GalleryView(ObjectSubclass<imp::GalleryView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl Default for GalleryView {
    fn default() -> Self {
        Self::new()
    }
}

impl GalleryView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    // ----- wiring -----

    pub fn set_model(&self, model: Rc<RefCell<Model>>) {
        *self.imp().model.borrow_mut() = Some(model);
    }

    pub fn set_thumbs(&self, svc: Rc<ThumbService>) {
        *self.imp().thumbs.borrow_mut() = Some(svc.clone());
        // Drain at most one batch on each real frame; sleep without wakeups when idle.
        let this = self.downgrade();
        let rx = svc.receiver();
        glib::spawn_future_local(async move {
            loop {
                let Ok(first) = rx.recv().await else { break };
                let Some(g) = this.upgrade() else { break };
                let (done_tx, done_rx) = async_channel::bounded(1);
                let svc = svc.clone();
                let first = RefCell::new(Some(first));
                g.add_tick_callback(move |g, _| {
                    g.drain_results(first.borrow_mut().take());
                    if svc.results_pending() {
                        glib::ControlFlow::Continue
                    } else {
                        let _ = done_tx.try_send(());
                        glib::ControlFlow::Break
                    }
                });
                if done_rx.recv().await.is_err() {
                    break;
                }
            }
        });
    }

    pub fn connect_activate(&self, f: impl Fn(u32) + 'static) {
        *self.imp().on_activate.borrow_mut() = Some(Box::new(f));
    }
    pub fn connect_current_changed(&self, f: impl Fn() + 'static) {
        *self.imp().on_current.borrow_mut() = Some(Box::new(f));
    }
    pub fn connect_context_menu(&self, f: impl Fn(f64, f64, Option<u32>) + 'static) {
        *self.imp().on_context.borrow_mut() = Some(Box::new(f));
    }
    pub fn connect_scrolled(&self, f: impl Fn() + 'static) {
        *self.imp().on_scroll.borrow_mut() = Some(Box::new(f));
    }
    pub fn connect_side_button(&self, f: impl Fn(bool) + 'static) {
        *self.imp().on_side_button.borrow_mut() = Some(Box::new(f));
    }
    pub fn connect_header_clicked(&self, f: impl Fn(usize, u32) + 'static) {
        *self.imp().on_header.borrow_mut() = Some(Box::new(f));
    }

    fn model(&self) -> Rc<RefCell<Model>> {
        self.imp().model.borrow().clone().expect("model set")
    }

    // ----- state -----

    pub fn grouped(&self) -> bool {
        self.imp().grouped.get()
    }

    pub fn set_grouped(&self, g: bool) {
        if self.imp().grouped.get() != g {
            let cur = self.current();
            self.imp().grouped.set(g);
            self.relayout(self.width(), self.height());
            if let Some(c) = cur {
                self.scroll_to_pos(c);
            }
            self.queue_draw();
        }
    }

    pub fn tile_size(&self) -> i32 {
        TILE_STEPS[self.imp().tile_step.get()]
    }

    pub fn tile_step(&self) -> usize {
        self.imp().tile_step.get()
    }

    pub fn zoom_step(&self, dir: i32) -> bool {
        let s = self.imp().tile_step.get() as i32 + dir;
        if s < 0 || s >= TILE_STEPS.len() as i32 {
            return false;
        }
        self.set_tile_step(s as usize);
        true
    }

    pub fn set_tile_step(&self, s: usize) {
        let s = s.min(TILE_STEPS.len() - 1);
        if s == self.imp().tile_step.get() {
            return;
        }
        let cur = self.current();
        self.imp().tile_step.set(s);
        self.relayout(self.width(), self.height());
        if let Some(c) = cur {
            self.scroll_to_pos(c);
        }
        self.queue_draw();
        self.schedule_wanted(0);
    }

    pub fn current(&self) -> Option<u32> {
        self.imp().current.get()
    }

    pub fn current_master(&self) -> Option<u32> {
        let m = self.model();
        let m = m.borrow();
        self.current()
            .filter(|&p| p < m.n_visible())
            .map(|p| m.master(p))
    }

    /// Set the current item; optionally scroll to it. Does not touch selection.
    pub fn set_current(&self, pos: Option<u32>, scroll: bool) {
        let n = self.model().borrow().n_visible();
        let pos = pos.filter(|&p| p < n);
        if pos != self.current() {
            self.imp().current.set(pos);
            if let Some(f) = self.imp().on_current.borrow().as_ref() {
                f();
            }
            self.queue_draw();
        }
        if scroll && let Some(p) = pos {
            self.scroll_to_pos(p);
        }
    }

    /// Keyboard/mouse semantics: current = pos, selection = {pos}.
    /// The anchor is a stable master index so filtering cannot invalidate it.
    fn go_to(&self, pos: u32, extend: bool) {
        let imp = self.imp();
        let model = self.model();
        {
            let mut m = model.borrow_mut();
            if pos >= m.n_visible() {
                return;
            }
            if extend {
                let a = imp
                    .anchor
                    .get()
                    .and_then(|master| m.pos_of_master(master))
                    .unwrap_or(pos);
                let (lo, hi) = if a <= pos { (a, pos) } else { (pos, a) };
                m.selected.clear();
                for p in lo..=hi {
                    let mm = m.master(p);
                    m.selected.insert(mm);
                }
            } else {
                let mm = m.master(pos);
                m.selected.clear();
                m.selected.insert(mm);
                imp.anchor.set(Some(mm));
            }
        }
        self.set_current(Some(pos), true);
        self.queue_draw();
    }

    pub fn select_all(&self) {
        let model = self.model();
        let mut m = model.borrow_mut();
        m.selected = m.visible.iter().copied().collect();
        drop(m);
        self.queue_draw();
    }

    pub fn select_none(&self) {
        self.model().borrow_mut().selected.clear();
        self.queue_draw();
    }

    fn toggle_select(&self, pos: u32) {
        let model = self.model();
        let mut m = model.borrow_mut();
        if pos >= m.n_visible() {
            return;
        }
        let mm = m.master(pos);
        if !m.selected.remove(&mm) {
            m.selected.insert(mm);
        }
        self.imp().anchor.set(Some(mm));
        drop(m);
        self.queue_draw();
    }

    pub fn set_generation(&self, g: u64) {
        let imp = self.imp();
        imp.generation.set(g);
        let mut c = imp.cache.borrow_mut();
        c.textures.clear();
        c.total_bytes = 0;
        c.durations.clear();
        c.failed.clear();
        drop(c);
        imp.current.set(None);
        imp.anchor.set(None);
        imp.hover.set(None);
        imp.last_top_section.set(None);
        if let Some(a) = imp.vadjustment.borrow().as_ref() {
            a.set_value(0.0);
        }
    }

    /// The model changed (appended, filtered, rebuilt).
    pub fn items_changed(&self) {
        let keep_master = self.current_master_raw();
        self.relayout(self.width(), self.height());
        // keep the current item on the same master entry if possible
        let n = self.model().borrow().n_visible();
        let new_pos = match keep_master {
            Some(m) => self.model().borrow().nearest_pos(m),
            None => None,
        };
        match new_pos {
            Some(p) if p < n => {
                if Some(p) != self.current() {
                    self.imp().current.set(Some(p));
                    if let Some(f) = self.imp().on_current.borrow().as_ref() {
                        f();
                    }
                }
            }
            _ => {
                if self.current().is_some() && n == 0 {
                    self.imp().current.set(None);
                    if let Some(f) = self.imp().on_current.borrow().as_ref() {
                        f();
                    }
                }
            }
        }
        self.queue_draw();
        self.schedule_wanted(0);
    }

    fn current_master_raw(&self) -> Option<u32> {
        // uses the *previous* visible list stored in the layout size; safe because
        // model.visible may already have changed: guard the index.
        let m = self.model();
        let m = m.borrow();
        self.current()
            .and_then(|p| m.visible.get(p as usize).copied())
    }

    pub fn textures_loaded(&self) -> usize {
        self.imp().cache.borrow().textures.len()
    }

    // ----- layout -----

    fn relayout(&self, width: i32, height: i32) {
        let model = self.model();
        let m = model.borrow();
        let tile = self.tile_size();
        let cols = ((width - 2 * PAD + GAP) / (tile + GAP)).max(1);
        let grouped = self.grouped();
        let mut lay = Layout {
            width,
            height,
            cols,
            tile,
            grouped,
            n: m.n_visible(),
            sec_y: Vec::new(),
            total_h: 0,
        };
        let stride = tile + GAP;
        if grouped {
            let mut y = TOP_PAD;
            lay.sec_y.reserve(m.sections.len());
            for s in &m.sections {
                lay.sec_y.push(y);
                y += lay.section_h(s.len);
            }
            lay.total_h = y + BOTTOM_PAD;
        } else {
            let rows = (lay.n as i32 + cols - 1) / cols;
            lay.total_h = TOP_PAD + rows * stride + BOTTOM_PAD;
        }
        drop(m);
        *self.imp().layout.borrow_mut() = lay;
        self.configure_adjustment();
    }

    fn configure_adjustment(&self) {
        let imp = self.imp();
        let lay = imp.layout.borrow();
        if let Some(adj) = imp.vadjustment.borrow().as_ref() {
            let page = lay.height as f64;
            let upper = (lay.total_h as f64).max(page);
            let value = adj.value().min((upper - page).max(0.0));
            adj.configure(value, 0.0, upper, lay.stride() as f64, page * 0.9, page);
        }
        if let Some(h) = imp.hadjustment.borrow().as_ref() {
            h.configure(0.0, 0.0, lay.width as f64, 1.0, 1.0, lay.width as f64);
        }
    }

    fn scroll_value(&self) -> f64 {
        self.imp()
            .vadjustment
            .borrow()
            .as_ref()
            .map(|a| a.value())
            .unwrap_or(0.0)
    }

    /// (x, y) of a visible position in content coordinates.
    fn tile_xy(&self, lay: &Layout, m: &Model, pos: u32) -> (i32, i32) {
        let stride = lay.stride();
        if lay.grouped {
            let si = m.section_of(pos);
            let s = &m.sections[si];
            let local = (pos - s.start) as i32;
            let (r, c) = (local / lay.cols, local % lay.cols);
            (PAD + c * stride, lay.sec_y[si] + HEADER_H + r * stride)
        } else {
            let (r, c) = (pos as i32 / lay.cols, pos as i32 % lay.cols);
            (PAD + c * stride, TOP_PAD + r * stride)
        }
    }

    fn hit(&self, lay: &Layout, m: &Model, x: f64, y: f64) -> Hit {
        let stride = lay.stride() as f64;
        let col = ((x - PAD as f64) / stride).floor();
        let in_col = col >= 0.0
            && col < lay.cols as f64
            && (x - PAD as f64) - col * stride < lay.tile as f64;
        if lay.grouped {
            if lay.sec_y.is_empty() {
                return Hit::Empty;
            }
            let si = match lay
                .sec_y
                .binary_search_by(|sy| f64::from(*sy).total_cmp(&y))
            {
                Ok(i) => i,
                Err(i) => i.saturating_sub(1),
            };
            if si >= m.sections.len() {
                return Hit::Empty;
            }
            let s = &m.sections[si];
            let top = lay.sec_y[si] as f64;
            if y < top {
                return Hit::Empty;
            }
            if y < top + HEADER_H as f64 {
                return Hit::Header(si);
            }
            let ly = y - top - HEADER_H as f64;
            let row = (ly / stride).floor();
            if ly - row * stride >= lay.tile as f64 || !in_col {
                return Hit::Empty;
            }
            let local = row as i64 * lay.cols as i64 + col as i64;
            if local >= 0 && (local as u32) < s.len {
                Hit::Tile(s.start + local as u32)
            } else {
                Hit::Empty
            }
        } else {
            let ly = y - TOP_PAD as f64;
            if ly < 0.0 || !in_col {
                return Hit::Empty;
            }
            let row = (ly / stride).floor();
            if ly - row * stride >= lay.tile as f64 {
                return Hit::Empty;
            }
            let pos = row as i64 * lay.cols as i64 + col as i64;
            if pos >= 0 && (pos as u32) < lay.n {
                Hit::Tile(pos as u32)
            } else {
                Hit::Empty
            }
        }
    }

    pub fn scroll_to_pos(&self, pos: u32) {
        let imp = self.imp();
        let lay = imp.layout.borrow();
        let model = self.model();
        let m = model.borrow();
        if pos >= m.n_visible() || lay.cols == 0 {
            return;
        }
        let (_, y) = self.tile_xy(&lay, &m, pos);
        let (top, bottom) = (y - HEADER_H - 8, y + lay.tile + 8);
        drop(m);
        let Some(adj) = imp.vadjustment.borrow().clone() else {
            return;
        };
        let v = adj.value();
        let page = adj.page_size();
        if (top as f64) < v + TOP_PAD as f64 * 0.5 {
            adj.set_value((top as f64 - 8.0).max(0.0));
        } else if bottom as f64 > v + page - BOTTOM_PAD as f64 {
            adj.set_value((bottom as f64 + BOTTOM_PAD as f64 - page).max(0.0));
        }
    }

    pub fn scroll_to_section(&self, si: usize) {
        let imp = self.imp();
        let lay = imp.layout.borrow();
        let model = self.model();
        let m = model.borrow();
        if si >= m.sections.len() {
            return;
        }
        let start = m.sections[si].start;
        let y = if lay.grouped {
            lay.sec_y[si] as f64 - 6.0
        } else {
            let (_, ty) = self.tile_xy(&lay, &m, start);
            ty as f64 - HEADER_H as f64
        };
        drop(m);
        drop(lay);
        if let Some(adj) = imp.vadjustment.borrow().as_ref() {
            adj.set_value(y.max(0.0));
        }
        self.set_current(Some(start), false);
        let model = self.model();
        let mut mm = model.borrow_mut();
        let master = mm.master(start);
        mm.selected.clear();
        mm.selected.insert(master);
        drop(mm);
        imp.anchor.set(Some(master));
    }

    /// Section index currently at the top of the viewport.
    pub fn top_section(&self) -> Option<usize> {
        let imp = self.imp();
        let lay = imp.layout.borrow();
        let model = self.model();
        let m = model.borrow();
        if m.sections.is_empty() {
            return None;
        }
        let v = self.scroll_value() + TOP_PAD as f64;
        if lay.grouped {
            let i = match lay
                .sec_y
                .binary_search_by(|sy| f64::from(*sy).total_cmp(&v))
            {
                Ok(i) => i,
                Err(i) => i.saturating_sub(1),
            };
            Some(i.min(m.sections.len() - 1))
        } else {
            let row = ((v - TOP_PAD as f64) / lay.stride() as f64)
                .floor()
                .max(0.0) as u32;
            let pos = (row * lay.cols as u32).min(m.n_visible().saturating_sub(1));
            Some(m.section_of(pos))
        }
    }

    /// Visible position range (inclusive) currently on screen.
    fn visible_range(&self, lay: &Layout, m: &Model) -> Option<(u32, u32)> {
        if lay.n == 0 {
            return None;
        }
        let v = self.scroll_value();
        let (y0, y1) = (v, v + lay.height as f64);
        let first = self.hit_row_first(lay, m, y0).unwrap_or_default();
        let last = match self.hit_row_first(lay, m, y1) {
            Some(p) => (p + lay.cols as u32).min(lay.n - 1),
            None => lay.n - 1,
        };
        Some((first.min(last), last.max(first)))
    }

    /// First position of the row at content y (or the next row below y).
    fn hit_row_first(&self, lay: &Layout, m: &Model, y: f64) -> Option<u32> {
        let stride = lay.stride() as f64;
        if lay.grouped {
            if lay.sec_y.is_empty() {
                return None;
            }
            let si = match lay
                .sec_y
                .binary_search_by(|sy| f64::from(*sy).total_cmp(&y))
            {
                Ok(i) => i,
                Err(i) => i.saturating_sub(1),
            };
            let si = si.min(m.sections.len() - 1);
            let s = &m.sections[si];
            let ly = y - lay.sec_y[si] as f64 - HEADER_H as f64;
            let row = (ly / stride).floor().max(0.0) as u32;
            let rows = s.len.div_ceil(lay.cols as u32);
            if row >= rows {
                // next section
                return m.sections.get(si + 1).map(|n| n.start);
            }
            Some(s.start + row * lay.cols as u32)
        } else {
            let row = ((y - TOP_PAD as f64) / stride).floor().max(0.0) as u32;
            Some((row * lay.cols as u32).min(lay.n.saturating_sub(1)))
        }
    }

    // ----- thumbnails -----

    fn schedule_wanted(&self, delay_ms: u64) {
        let imp = self.imp();
        if let Some(s) = imp.wanted_source.borrow_mut().take() {
            s.remove();
        }
        let this = self.downgrade();
        let id =
            glib::timeout_add_local_once(std::time::Duration::from_millis(delay_ms), move || {
                if let Some(g) = this.upgrade() {
                    *g.imp().wanted_source.borrow_mut() = None;
                    g.update_wanted();
                }
            });
        *imp.wanted_source.borrow_mut() = Some(id);
    }

    fn update_wanted(&self) {
        let imp = self.imp();
        let Some(svc) = imp.thumbs.borrow().clone() else {
            return;
        };
        let lay = imp.layout.borrow();
        let model = self.model();
        let m = model.borrow();
        let Some((first, last)) = self.visible_range(&lay, &m) else {
            svc.clear();
            return;
        };
        let per_screen = (last - first + 1).max(lay.cols as u32);
        let lo = first.saturating_sub(per_screen);
        let hi = (last + per_screen).min(lay.n - 1);
        let centre = (first + last) / 2;
        let need_size: u32 = if lay.tile > 256 { 512 } else { 256 };
        let cache = imp.cache.borrow();
        let mut jobs = Vec::with_capacity((hi - lo + 1) as usize);
        for pos in lo..=hi {
            let master = m.master(pos);
            if cache.failed.contains(&master) {
                continue;
            }
            if let Some(tile) = cache.textures.get(&master)
                && tile.size >= need_size
            {
                continue;
            }
            let e = &m.entries[master as usize];
            let dist = (pos as i64 - centre as i64).unsigned_abs() as u32 / lay.cols as u32;
            // on-screen tiles first, then by distance
            let prio = if pos >= first && pos <= last {
                dist
            } else {
                1000 + dist
            };
            jobs.push(Job {
                idx: master,
                path: e.path.clone(),
                kind: e.kind(),
                size: need_size,
                prio,
            });
        }
        drop(cache);
        svc.set_wanted(jobs);
    }

    fn drain_results(&self, first: Option<crate::thumbs::ThumbResult>) {
        let imp = self.imp();
        let Some(svc) = imp.thumbs.borrow().clone() else {
            return;
        };
        let mut got = 0;
        let mut saw_result = false;
        let mut pending = first;
        let generation = imp.generation.get();
        let mut cache = imp.cache.borrow_mut();
        while got < 32 {
            let Some(r) = pending.take().or_else(|| svc.try_recv()) else {
                break;
            };
            saw_result = true;
            got += 1;
            if r.generation != generation {
                continue;
            }
            match r.texture {
                Some(t) => {
                    let should_replace = cache
                        .textures
                        .get(&r.idx)
                        .is_none_or(|tile| tile.size < r.size);
                    if should_replace {
                        if let Some(old) = cache.textures.remove(&r.idx) {
                            cache.total_bytes = cache.total_bytes.saturating_sub(old.bytes);
                        }
                        cache.total_bytes = cache.total_bytes.saturating_add(r.texture_bytes);
                        cache.textures.insert(
                            r.idx,
                            CachedTile {
                                texture: t,
                                size: r.size,
                                bytes: r.texture_bytes,
                            },
                        );
                    }
                }
                None => {
                    cache.failed.insert(r.idx);
                }
            }
            if let Some(d) = r.duration {
                cache.durations.insert(r.idx, d);
            }
        }
        if cache.total_bytes > MAX_TEXTURE_BYTES {
            // evict by distance from the viewport
            let lay = imp.layout.borrow();
            let model = self.model();
            let m = model.borrow();
            if let Some((first, last)) = self.visible_range(&lay, &m) {
                let per_screen = (last - first + 1) as i64;
                let (lo, hi) = (first as i64 - 3 * per_screen, last as i64 + 3 * per_screen);
                let outside: Vec<u32> = cache
                    .textures
                    .keys()
                    .copied()
                    .filter(|master| match m.pos_of_master(*master) {
                        Some(p) => (p as i64) < lo || (p as i64) > hi,
                        None => true,
                    })
                    .collect();
                for master in outside {
                    if let Some(tile) = cache.textures.remove(&master) {
                        cache.total_bytes = cache.total_bytes.saturating_sub(tile.bytes);
                    }
                }
                if cache.total_bytes > MAX_TEXTURE_BYTES {
                    let centre = (first + last) / 2;
                    let mut farthest: Vec<(u32, u32)> = cache
                        .textures
                        .keys()
                        .copied()
                        .map(|master| {
                            let distance = m
                                .pos_of_master(master)
                                .map(|pos| pos.abs_diff(centre))
                                .unwrap_or(u32::MAX);
                            (distance, master)
                        })
                        .collect();
                    farthest.sort_unstable_by(|a, b| b.cmp(a));
                    for (_, master) in farthest {
                        if cache.total_bytes <= MAX_TEXTURE_BYTES {
                            break;
                        }
                        if let Some(tile) = cache.textures.remove(&master) {
                            cache.total_bytes = cache.total_bytes.saturating_sub(tile.bytes);
                        }
                    }
                }
            } else {
                cache.textures.clear();
                cache.total_bytes = 0;
            }
        }
        drop(cache);
        if got > 0 {
            self.queue_draw();
        }
        // A stale in-flight job may have suppressed a current-generation request.
        if saw_result && svc.pending() == 0 {
            self.schedule_wanted(0);
        }
    }

    fn on_scrolled(&self) {
        self.queue_draw();
        self.schedule_wanted(40);
        let top = self.top_section();
        if top != self.imp().last_top_section.get() {
            self.imp().last_top_section.set(top);
        }
        if let Some(f) = self.imp().on_scroll.borrow().as_ref() {
            f();
        }
    }

    // ----- drawing -----

    fn draw(&self, snapshot: &gtk::Snapshot) {
        let imp = self.imp();
        let lay = imp.layout.borrow().clone();
        let model = self.model();
        let m = model.borrow();
        let colors = theme::colors();
        let w = lay.width as f32;
        let h = lay.height as f32;
        snapshot.append_color(&colors.bg_dark, &graphene::Rect::new(0.0, 0.0, w, h));
        if lay.n == 0 {
            let text = if m.scanning {
                "Scanning…"
            } else if m.entries.is_empty() {
                "No images here"
            } else {
                "Nothing matches this filter — press 0"
            };
            let layout = self.create_pango_layout(Some(text));
            layout.set_font_description(Some(&pango::FontDescription::from_string("Sans 13")));
            let (tw, th) = layout.pixel_size();
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                (w - tw as f32) / 2.0,
                (h - th as f32) / 2.0,
            ));
            snapshot.append_layout(&layout, &colors.fg_dark);
            snapshot.restore();
            return;
        }
        let v = self.scroll_value();
        snapshot.save();
        snapshot.translate(&graphene::Point::new(0.0, -v as f32));

        let cache = imp.cache.borrow();
        let current = imp.current.get();
        let hover = imp.hover.get();
        let focus_visible = self.has_focus();
        let (y0, y1) = (v as f32, (v + lay.height as f64) as f32);

        let Some((first, last)) = self.visible_range(&lay, &m) else {
            snapshot.restore();
            return;
        };
        for pos in first..=last {
            let (x, y) = self.tile_xy(&lay, &m, pos);
            let (xf, yf, t) = (x as f32, y as f32, lay.tile as f32);
            if yf + t < y0 || yf > y1 {
                continue;
            }
            let master = m.master(pos);
            let e = &m.entries[master as usize];
            let rect = graphene::Rect::new(xf, yf, t, t);
            let selected = m.selected.contains(&master);
            let is_current = current == Some(pos);
            let lifted = is_current || hover == Some(pos);
            let draw_rect = if lifted {
                let d = t * 0.02;
                graphene::Rect::new(xf - d, yf - d, t + 2.0 * d, t + 2.0 * d)
            } else {
                rect
            };
            let draw_rr = gsk::RoundedRect::from_rect(draw_rect, 6.0);
            if lifted {
                snapshot.append_outset_shadow(
                    &draw_rr,
                    &gdk::RGBA::new(0.0, 0.0, 0.0, 0.45),
                    0.0,
                    4.0,
                    2.0,
                    10.0,
                );
            }
            snapshot.push_rounded_clip(&draw_rr);
            match cache.textures.get(&master) {
                Some(tile) => {
                    let tex = &tile.texture;
                    let (iw, ih) = (tex.width() as f32, tex.height() as f32);
                    let s = (draw_rect.width() / iw).max(draw_rect.height() / ih);
                    let (dw, dh) = (iw * s, ih * s);
                    let dst = graphene::Rect::new(
                        draw_rect.x() + (draw_rect.width() - dw) / 2.0,
                        draw_rect.y() + (draw_rect.height() - dh) / 2.0,
                        dw,
                        dh,
                    );
                    let filter = if s < 1.0 {
                        gsk::ScalingFilter::Trilinear
                    } else {
                        gsk::ScalingFilter::Linear
                    };
                    snapshot.append_scaled_texture(tex, filter, &dst);
                }
                None => {
                    let c = if cache.failed.contains(&master) {
                        theme::mix(&colors.bg_light, &colors.red, 0.15)
                    } else {
                        colors.bg_light
                    };
                    snapshot.append_color(&c, &draw_rect);
                    let label = if cache.failed.contains(&master) {
                        "!"
                    } else {
                        e.group.label()
                    };
                    let l = self.create_pango_layout(Some(label));
                    l.set_font_description(Some(&pango::FontDescription::from_string("Sans 9")));
                    let (tw, th) = l.pixel_size();
                    snapshot.save();
                    snapshot.translate(&graphene::Point::new(
                        draw_rect.x() + (t - tw as f32) / 2.0,
                        draw_rect.y() + (t - th as f32) / 2.0,
                    ));
                    snapshot.append_layout(&l, &colors.fg_dark);
                    snapshot.restore();
                }
            }
            if e.kind() == Kind::Video {
                self.draw_play_badge(
                    snapshot,
                    &draw_rect,
                    cache.durations.get(&master).copied(),
                    &colors,
                );
            }
            snapshot.pop();
            if selected {
                snapshot.append_border(&draw_rr, &[3.0; 4], &[colors.accent; 4]);
            } else if is_current && focus_visible {
                snapshot.append_border(&draw_rr, &[2.0; 4], &[theme::alpha(&colors.fg, 0.6); 4]);
            } else if hover == Some(pos) {
                snapshot.append_border(&draw_rr, &[1.0; 4], &[theme::alpha(&colors.fg, 0.35); 4]);
            }
            if selected || hover == Some(pos) {
                let checkbox = graphene::Rect::new(xf + t - 25.0, yf + 7.0, 18.0, 18.0);
                let checkbox_rr = gsk::RoundedRect::from_rect(checkbox, 9.0);
                let fill = if selected {
                    colors.accent
                } else {
                    gdk::RGBA::new(0.0, 0.0, 0.0, 0.55)
                };
                snapshot.append_color(&fill, &checkbox);
                snapshot.append_border(&checkbox_rr, &[1.0; 4], &[colors.fg_bright; 4]);
                if selected {
                    let check = self.create_pango_layout(Some("✓"));
                    check.set_font_description(Some(&pango::FontDescription::from_string(
                        "Sans Bold 11",
                    )));
                    let (cw, ch) = check.pixel_size();
                    snapshot.save();
                    snapshot.translate(&graphene::Point::new(
                        checkbox.x() + (checkbox.width() - cw as f32) / 2.0,
                        checkbox.y() + (checkbox.height() - ch as f32) / 2.0,
                    ));
                    snapshot.append_layout(&check, &colors.bg_darker);
                    snapshot.restore();
                }
            }
        }

        // headers (grouped): each section header at its place, the top one sticky
        if lay.grouped {
            let top_si = self.top_section().unwrap_or(0);
            let s_first = m.section_of(first);
            let s_last = m.section_of(last);
            for si in s_first.min(top_si)..=s_last {
                let s = &m.sections[si];
                let mut hy = lay.sec_y[si] as f32;
                let mut sticky = false;
                if si == top_si {
                    let next_y = lay
                        .sec_y
                        .get(si + 1)
                        .map(|&n| n as f32 - HEADER_H as f32)
                        .unwrap_or(f32::MAX);
                    let sy = (v as f32).max(hy).min(next_y);
                    if sy > hy {
                        sticky = true;
                    }
                    hy = sy;
                }
                if (hy + HEADER_H as f32) < y0 || hy > y1 {
                    continue;
                }
                let hrect = graphene::Rect::new(0.0, hy, w, HEADER_H as f32);
                if sticky {
                    snapshot.append_color(&theme::alpha(&colors.bg_dark, 0.92), &hrect);
                } else {
                    snapshot.append_color(&colors.bg_dark, &hrect);
                }
                let name = m.folder_display(s.folder).to_string();
                let l = self.create_pango_layout(Some(&name));
                l.set_font_description(Some(&pango::FontDescription::from_string("Sans Bold 11")));
                l.set_ellipsize(pango::EllipsizeMode::Middle);
                l.set_width(((w * 0.6) as i32) * pango::SCALE);
                let (tw, th) = l.pixel_size();
                snapshot.save();
                snapshot.translate(&graphene::Point::new(
                    PAD as f32,
                    hy + (HEADER_H as f32 - th as f32) / 2.0 + 2.0,
                ));
                snapshot.append_layout(&l, &colors.fg_bright);
                snapshot.restore();
                let c = self.create_pango_layout(Some(&format!("{} files", s.len)));
                c.set_font_description(Some(&pango::FontDescription::from_string("Sans 10")));
                let (_, ch) = c.pixel_size();
                snapshot.save();
                snapshot.translate(&graphene::Point::new(
                    PAD as f32 + tw as f32 + 10.0,
                    hy + (HEADER_H as f32 - ch as f32) / 2.0 + 2.0,
                ));
                snapshot.append_layout(&c, &colors.fg_dark);
                snapshot.restore();
                let line_y = hy + HEADER_H as f32 - 4.0;
                snapshot.append_color(
                    &colors.bg_light,
                    &graphene::Rect::new(PAD as f32, line_y, w - 2.0 * PAD as f32, 1.0),
                );
            }
        }

        // rubber band
        if let Some((x0, ya, x1, yb)) = *imp.rubber.borrow() {
            let r = graphene::Rect::new(
                x0.min(x1) as f32,
                ya.min(yb) as f32,
                (x1 - x0).abs() as f32,
                (yb - ya).abs() as f32,
            );
            snapshot.append_color(&theme::alpha(&colors.accent, 0.15), &r);
            snapshot.append_border(
                &gsk::RoundedRect::from_rect(r, 2.0),
                &[1.0; 4],
                &[colors.accent; 4],
            );
        }
        snapshot.restore();
    }

    fn draw_play_badge(
        &self,
        snapshot: &gtk::Snapshot,
        rect: &graphene::Rect,
        duration: Option<f64>,
        colors: &theme::Colors,
    ) {
        let r = (rect.width() * 0.13).max(10.0);
        let (cx, cy) = (
            rect.x() + rect.width() / 2.0,
            rect.y() + rect.height() / 2.0,
        );
        let pb = gsk::PathBuilder::new();
        pb.add_circle(&graphene::Point::new(cx, cy), r);
        snapshot.append_fill(
            &pb.to_path(),
            gsk::FillRule::Winding,
            &gdk::RGBA::new(0.0, 0.0, 0.0, 0.55),
        );
        let tri = gsk::PathBuilder::new();
        tri.move_to(cx - r * 0.35, cy - r * 0.5);
        tri.line_to(cx + r * 0.55, cy);
        tri.line_to(cx - r * 0.35, cy + r * 0.5);
        tri.close();
        snapshot.append_fill(&tri.to_path(), gsk::FillRule::Winding, &gdk::RGBA::WHITE);
        if let Some(d) = duration {
            let text = format_duration(d);
            let l = self.create_pango_layout(Some(&text));
            l.set_font_description(Some(&pango::FontDescription::from_string("Monospace 8")));
            let (tw, th) = l.pixel_size();
            let bx = rect.x() + rect.width() - tw as f32 - 10.0;
            let by = rect.y() + rect.height() - th as f32 - 7.0;
            snapshot.append_color(
                &gdk::RGBA::new(0.0, 0.0, 0.0, 0.6),
                &graphene::Rect::new(bx - 4.0, by - 1.0, tw as f32 + 8.0, th as f32 + 2.0),
            );
            snapshot.save();
            snapshot.translate(&graphene::Point::new(bx, by));
            snapshot.append_layout(&l, &gdk::RGBA::WHITE);
            snapshot.restore();
        }
        let _ = colors;
    }

    // ----- input -----

    fn setup_controllers(&self) {
        // clicks: any button
        let click = gtk::GestureClick::new();
        click.set_button(0);
        let this = self.downgrade();
        click.connect_pressed(move |g, n_press, x, y| {
            let Some(view) = this.upgrade() else { return };
            let button = g.current_button();
            let state = g.current_event_state();
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
            view.grab_focus();
            let hit = view.hit_at(x, y);
            match button {
                1 => match hit {
                    Hit::Tile(pos) => {
                        if n_press >= 2 {
                            view.activate_pos(pos);
                        } else if ctrl || view.checkbox_at(pos, x, y) {
                            view.toggle_select(pos);
                            view.set_current(Some(pos), false);
                        } else {
                            view.go_to(pos, shift);
                        }
                    }
                    Hit::Header(si) => {
                        if n_press >= 2 {
                            if let Some(f) = view.imp().on_header.borrow().as_ref() {
                                f(si, 2);
                            }
                        } else {
                            view.scroll_to_section(si);
                            if let Some(f) = view.imp().on_header.borrow().as_ref() {
                                f(si, 1);
                            }
                        }
                    }
                    Hit::Empty => {
                        if !ctrl && !shift {
                            view.select_none();
                        }
                    }
                },
                2 => {
                    if let Hit::Tile(pos) = hit {
                        view.toggle_select(pos);
                        view.set_current(Some(pos), false);
                    }
                }
                3 => {
                    let pos = if let Hit::Tile(pos) = hit {
                        let sel = view
                            .model()
                            .borrow()
                            .selected
                            .contains(&view.model().borrow().master(pos));
                        if !sel {
                            view.go_to(pos, false);
                        } else {
                            view.set_current(Some(pos), false);
                        }
                        Some(pos)
                    } else {
                        None
                    };
                    if let Some(f) = view.imp().on_context.borrow().as_ref() {
                        f(x, y, pos);
                    }
                }
                8 | 11 => {
                    if let Some(f) = view.imp().on_side_button.borrow().as_ref() {
                        f(false);
                    }
                }
                9 | 10 => {
                    if let Some(f) = view.imp().on_side_button.borrow().as_ref() {
                        f(true);
                    }
                }
                _ => {}
            }
        });
        self.add_controller(click);

        // rubber band on empty space
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let this = self.downgrade();
        drag.connect_drag_begin(move |g, x, y| {
            let Some(view) = this.upgrade() else { return };
            if let Hit::Empty = view.hit_at(x, y) {
                let state = g.current_event_state();
                let base = if state.contains(gdk::ModifierType::CONTROL_MASK) {
                    view.model().borrow().selected.clone()
                } else {
                    HashSet::new()
                };
                *view.imp().rubber_base.borrow_mut() = base;
                let cy = y + view.scroll_value();
                *view.imp().rubber.borrow_mut() = Some((x, cy, x, cy));
            }
        });
        let this = self.downgrade();
        drag.connect_drag_update(move |_, dx, dy| {
            let Some(view) = this.upgrade() else { return };
            let mut r = view.imp().rubber.borrow_mut();
            if let Some((x0, y0, _, _)) = *r {
                *r = Some((x0, y0, x0 + dx, y0 + dy));
                drop(r);
                view.apply_rubber();
                view.queue_draw();
            }
        });
        let this = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            let Some(view) = this.upgrade() else { return };
            *view.imp().rubber.borrow_mut() = None;
            view.queue_draw();
        });
        self.add_controller(drag);

        // hover
        let motion = gtk::EventControllerMotion::new();
        let this = self.downgrade();
        motion.connect_motion(move |_, x, y| {
            let Some(view) = this.upgrade() else { return };
            let h = match view.hit_at(x, y) {
                Hit::Tile(p) => Some(p),
                _ => None,
            };
            if h != view.imp().hover.get() {
                view.imp().hover.set(h);
                view.queue_draw();
            }
        });
        let this = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(view) = this.upgrade()
                && view.imp().hover.get().is_some()
            {
                view.imp().hover.set(None);
                view.queue_draw();
            }
        });
        self.add_controller(motion);

        // keyboard
        let key = gtk::EventControllerKey::new();
        let this = self.downgrade();
        key.connect_key_pressed(move |_, keyval, _, state| {
            let Some(view) = this.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if view.handle_key(keyval, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.add_controller(key);

        // tooltip
        let this = self.downgrade();
        self.connect_query_tooltip(move |_, x, y, _, tip| {
            let Some(view) = this.upgrade() else {
                return false;
            };
            if let Hit::Tile(pos) = view.hit_at(x as f64, y as f64) {
                let model = view.model();
                let m = model.borrow();
                let e = m.entry_at(pos);
                let mut text = e.name.to_string();
                if let Some((sz, _)) = crate::thumbs::file_info(&e.path) {
                    text.push_str(&format!("\n{}", crate::thumbs::human_size(sz)));
                }
                tip.set_text(Some(&text));
                true
            } else {
                false
            }
        });
    }

    fn hit_at(&self, x: f64, y: f64) -> Hit {
        let lay = self.imp().layout.borrow();
        let model = self.model();
        let m = model.borrow();
        let scroll = self.scroll_value();
        if lay.grouped
            && (0.0..HEADER_H as f64).contains(&y)
            && let Some(section) = self.top_section()
            && scroll > lay.sec_y[section] as f64
        {
            return Hit::Header(section);
        }
        self.hit(&lay, &m, x, y + scroll)
    }

    fn checkbox_at(&self, pos: u32, x: f64, viewport_y: f64) -> bool {
        let lay = self.imp().layout.borrow();
        let model = self.model();
        let m = model.borrow();
        if pos >= m.n_visible() {
            return false;
        }
        let (tile_x, tile_y) = self.tile_xy(&lay, &m, pos);
        let content_y = viewport_y + self.scroll_value();
        let hit_size = 30.0;
        x >= f64::from(tile_x + lay.tile) - hit_size
            && x <= f64::from(tile_x + lay.tile)
            && content_y >= f64::from(tile_y)
            && content_y <= f64::from(tile_y) + hit_size
    }

    fn apply_rubber(&self) {
        let imp = self.imp();
        let Some((x0, y0, x1, y1)) = *imp.rubber.borrow() else {
            return;
        };
        let (rx0, rx1, ry0, ry1) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
        let lay = imp.layout.borrow();
        if lay.n == 0 {
            return;
        }
        let model = self.model();
        let mut m = model.borrow_mut();
        let mut sel = imp.rubber_base.borrow().clone();
        // rows intersecting the band
        let first = self.hit_row_first(&lay, &m, ry0).unwrap_or(0);
        let last = self
            .hit_row_first(&lay, &m, ry1)
            .map(|p| (p + lay.cols as u32).min(lay.n.saturating_sub(1)))
            .unwrap_or(lay.n.saturating_sub(1));
        for pos in first..=last.min(lay.n.saturating_sub(1)) {
            let (x, y) = self.tile_xy(&lay, &m, pos);
            let (xf, yf, t) = (x as f64, y as f64, lay.tile as f64);
            if xf < rx1 && xf + t > rx0 && yf < ry1 && yf + t > ry0 {
                sel.insert(m.master(pos));
            }
        }
        m.selected = sel;
    }

    fn activate_pos(&self, pos: u32) {
        self.set_current(Some(pos), false);
        if let Some(f) = self.imp().on_activate.borrow().as_ref() {
            f(pos);
        }
    }

    pub fn activate_current(&self) {
        if let Some(p) = self.current() {
            self.activate_pos(p);
        } else if self.model().borrow().n_visible() > 0 {
            self.activate_pos(0);
        }
    }

    /// Move current by a visual step: dx columns, dy rows, or a page.
    fn nav(&self, dx: i32, dy: i32, page: bool, extend: bool) {
        let n = self.model().borrow().n_visible();
        if n == 0 {
            return;
        }
        let cur = match self.current() {
            Some(c) => c,
            None => {
                self.go_to(0, false);
                return;
            }
        };
        let target = {
            let lay = self.imp().layout.borrow();
            let model = self.model();
            let m = model.borrow();
            let (x, y) = self.tile_xy(&lay, &m, cur);
            let stride = lay.stride();
            let (cx, mut cy) = (
                x as f64 + lay.tile as f64 / 2.0,
                y as f64 + lay.tile as f64 / 2.0,
            );
            if dx != 0 {
                let t = cur as i64 + dx as i64;
                Some(t.clamp(0, n as i64 - 1) as u32)
            } else {
                let step = if page {
                    (lay.height - TOP_PAD - BOTTOM_PAD).max(stride) as f64
                } else {
                    stride as f64
                };
                let dir = if dy > 0 { 1.0 } else { -1.0 };
                cy += dir * step;
                // walk until we land on a tile (skip headers / gaps)
                let mut found = None;
                for _ in 0..6 {
                    match self.hit(&lay, &m, cx, cy) {
                        Hit::Tile(p) => {
                            found = Some(p);
                            break;
                        }
                        Hit::Header(si) => {
                            // jump to the row above the header (previous section's last row) or below it
                            if dir < 0.0 {
                                let s = &m.sections[si.saturating_sub(1)];
                                if si == 0 {
                                    found = Some(0);
                                    break;
                                }
                                let last = s.start + s.len - 1;
                                let col =
                                    ((cx - PAD as f64) / stride as f64).floor().max(0.0) as u32;
                                let last_row_start = last - ((last - s.start) % lay.cols as u32);
                                found = Some((last_row_start + col).min(last));
                            } else {
                                let s = &m.sections[si];
                                let col =
                                    ((cx - PAD as f64) / stride as f64).floor().max(0.0) as u32;
                                found = Some((s.start + col).min(s.start + s.len - 1));
                            }
                            break;
                        }
                        Hit::Empty => {
                            cy += dir * stride as f64 * 0.5;
                            if cy < 0.0 {
                                found = Some(0);
                                break;
                            }
                            if cy > lay.total_h as f64 {
                                found = Some(n - 1);
                                break;
                            }
                        }
                    }
                }
                found.or(Some(if dir < 0.0 { 0 } else { n - 1 }))
            }
        };
        if let Some(t) = target {
            self.go_to(t, extend);
        }
    }

    fn handle_key(&self, keyval: gdk::Key, state: gdk::ModifierType) -> bool {
        use gdk::Key;
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let n = self.model().borrow().n_visible();
        match keyval {
            Key::Left | Key::KP_Left if !ctrl => self.nav(-1, 0, false, shift),
            Key::Right | Key::KP_Right if !ctrl => self.nav(1, 0, false, shift),
            Key::Up | Key::KP_Up if !ctrl => self.nav(0, -1, false, shift),
            Key::Down | Key::KP_Down if !ctrl => self.nav(0, 1, false, shift),
            Key::Page_Up | Key::KP_Page_Up if !ctrl => self.nav(0, -1, true, shift),
            Key::Page_Down | Key::KP_Page_Down if !ctrl => self.nav(0, 1, true, shift),
            Key::Home | Key::KP_Home => {
                if n > 0 {
                    self.go_to(0, shift)
                }
            }
            Key::End | Key::KP_End => {
                if n > 0 {
                    self.go_to(n - 1, shift)
                }
            }
            Key::Return | Key::KP_Enter => self.activate_current(),
            Key::space | Key::Insert | Key::KP_Insert => {
                if let Some(c) = self.current() {
                    self.toggle_select(c)
                }
            }
            _ => return false,
        }
        true
    }
}

pub fn format_duration(secs: f64) -> String {
    let s = secs.round() as u64;
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
