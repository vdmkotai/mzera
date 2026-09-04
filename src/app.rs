//! Application glue: window, pages, actions, navigation, status, menus.

use crate::actions::{ACTIONS, GALLERY, VIDEO, VIEWER};
use crate::filters::{FilterPopover, group_digits};
use crate::gallery::{DEFAULT_TILE_STEP, GalleryView};
use crate::model::{Filter, Model, N_PRESETS, preset};
use crate::overlays::{Captions, Help, MetaPanel, Toolbar};
use crate::scan::{self, Kind, Msg};
use crate::settings::Settings;
use crate::theme::Theme;
use crate::thumbs::{self, ThumbService};
use crate::video::VideoView;
use crate::viewer::{Decoder, ImageView, SCALE_MODES, ScaleMode};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const APP_ID: &str = "dev.vdm.mzera";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Gallery,
    Image,
    Video,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaptionMode {
    Brief,
    Metadata,
    Clean,
}

struct Binding {
    key: gdk::Key,
    mods: gdk::ModifierType,
    name: &'static str,
    modes: u8,
}

pub struct App {
    window: adw::ApplicationWindow,
    toast: adw::ToastOverlay,
    stack: gtk::Stack,
    // gallery page
    gallery: GalleryView,
    pill_flat: gtk::ToggleButton,
    pill_grouped: gtk::ToggleButton,
    pill_filters: gtk::Button,
    status_root: gtk::Button,
    status_counts: gtk::Button,
    status_filter: gtk::Button,
    status_thumbs: gtk::Button,
    status_section: gtk::Button,
    jump_revealer: gtk::Revealer,
    jump_list: gtk::ListBox,
    jump_syncing: Cell<bool>,
    menu_open: Rc<Cell<bool>>,
    filter_popover: FilterPopover,
    gallery_menu: gio::Menu,
    // viewer page
    viewer_page: gtk::Overlay,
    viewer_stack: gtk::Stack,
    image: ImageView,
    video: VideoView,
    captions: Captions,
    meta: MetaPanel,
    toolbar: Toolbar,
    edge_prev: gtk::Button,
    edge_next: gtk::Button,
    help: Help,
    viewer_menu: gio::Menu,
    decoder: Rc<Decoder>,
    // state
    model: Rc<RefCell<Model>>,
    thumbs: Rc<ThumbService>,
    mode: Cell<Mode>,
    caption_mode: Cell<CaptionMode>,
    vpos: Cell<Option<u32>>,
    slideshow: RefCell<Option<glib::SourceId>>,
    slideshow_interval: Cell<u64>,
    toolbar_timer: RefCell<Option<glib::SourceId>>,
    pointer: Cell<(f64, f64)>,
    scroll_acc: Cell<f64>,
    zoom_base: Cell<f64>,
    bindings: Vec<Binding>,
    actions: RefCell<HashMap<&'static str, gio::SimpleAction>>,
    pending_start: RefCell<Option<PathBuf>>,
    pending_start_viewer: Cell<bool>,
    meta_token: Cell<u64>,
    jump_timer: RefCell<Option<glib::SourceId>>,
    _theme: Rc<Theme>,
}

fn usage() {
    eprintln!(
        "mzera — fast photo & WebM viewer\n\n  mzera [--gallery | --viewer] [DIR | FILE ...]\n                                 open a folder or file recursively\n  mzera --warm DIR [--size N]  pre-build the thumbnail cache for a tree\n  mzera --version"
    );
}

thread_local! {
    static T0: std::time::Instant = std::time::Instant::now();
}

fn debug() -> bool {
    std::env::var_os("MZERA_DEBUG").is_some()
}

fn tlog(msg: &str) {
    if debug() {
        T0.with(|t| eprintln!("[{:>7.1} ms] {msg}", t.elapsed().as_secs_f64() * 1000.0));
    }
}

pub fn run() -> glib::ExitCode {
    T0.with(|_| {});
    glib::set_application_name("Mzera");
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("mzera {}", env!("CARGO_PKG_VERSION"));
        return glib::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        usage();
        return glib::ExitCode::SUCCESS;
    }
    if let Some(i) = args.iter().position(|a| a == "--warm") {
        let Some(dir) = args.get(i + 1) else {
            usage();
            return glib::ExitCode::FAILURE;
        };
        let size = args
            .iter()
            .position(|a| a == "--size")
            .and_then(|j| args.get(j + 1))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(256);
        let code = thumbs::warm(Path::new(dir), size);
        return if code == 0 {
            glib::ExitCode::SUCCESS
        } else {
            glib::ExitCode::FAILURE
        };
    }

    let gallery_flag = args.iter().any(|arg| arg == "--gallery");
    let viewer_flag = args.iter().any(|arg| arg == "--viewer");
    if gallery_flag && viewer_flag {
        eprintln!("mzera: --gallery and --viewer are mutually exclusive");
        return glib::ExitCode::FAILURE;
    }
    let start_mode = Rc::new(Cell::new(if gallery_flag {
        Some(Mode::Gallery)
    } else if viewer_flag {
        Some(Mode::Image)
    } else {
        None
    }));
    let app_args: Vec<String> = args
        .into_iter()
        .filter(|arg| arg != "--gallery" && arg != "--viewer")
        .collect();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();
    let state: Rc<RefCell<Option<Rc<App>>>> = Rc::new(RefCell::new(None));

    let ensure = {
        let state = state.clone();
        move |app: &adw::Application| -> Rc<App> {
            if let Some(a) = state.borrow().as_ref() {
                return a.clone();
            }
            let a = App::new(app);
            *state.borrow_mut() = Some(a.clone());
            a
        }
    };
    {
        let ensure = ensure.clone();
        app.connect_activate(move |app| {
            let a = ensure(app);
            a.window.present();
            if a.model.borrow().root.as_os_str().is_empty() {
                a.pick_folder();
            }
        });
    }
    {
        let ensure = ensure.clone();
        let start_mode = start_mode.clone();
        app.connect_open(move |app, files, _hint| {
            let a = ensure(app);
            a.window.present();
            if let Some(p) = files.iter().find_map(gio::File::path) {
                a.open_path_in_mode(p, start_mode.get());
                if files.len() > 1 {
                    a.toast(&format!(
                        "Opened the first of {} requested paths",
                        group_digits(files.len() as u32)
                    ));
                }
            }
        });
    }
    {
        let state = state.clone();
        app.connect_shutdown(move |_| {
            if let Some(instance) = state.borrow_mut().take() {
                instance.shutdown();
            }
        });
    }
    app.run_with_args(&app_args)
}

fn short_home(p: &Path) -> String {
    if let Ok(rest) = p.strip_prefix(glib::home_dir()) {
        if rest.as_os_str().is_empty() {
            "~".into()
        } else {
            format!("~/{}", rest.display())
        }
    } else {
        p.to_string_lossy().into_owned()
    }
}

fn menu_item(label: &str, action: &str) -> gio::MenuItem {
    gio::MenuItem::new(Some(label), Some(action))
}

impl App {
    fn shutdown(&self) {
        self.save_settings();
        for source in [
            self.slideshow.borrow_mut().take(),
            self.toolbar_timer.borrow_mut().take(),
            self.jump_timer.borrow_mut().take(),
        ]
        .into_iter()
        .flatten()
        {
            source.remove();
        }
        self.video.stop();
        self.decoder.clear();
        self.thumbs.request_shutdown();
    }

    fn new(app: &adw::Application) -> Rc<App> {
        let settings = Settings::load();
        let theme = Theme::install();
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .clamp(2, 12);
        let thumbs = Rc::new(ThumbService::new(workers));
        let model = Rc::new(RefCell::new(Model::empty()));

        // ---- menus ----
        let main_menu = gio::Menu::new();
        let s1 = gio::Menu::new();
        s1.append_item(&menu_item("Open folder…", "win.open-dir"));
        s1.append_item(&menu_item("Rescan", "win.rescan"));
        main_menu.append_section(None, &s1);
        let s2 = gio::Menu::new();
        s2.append_item(&menu_item("Slideshow", "win.slideshow"));
        s2.append_item(&menu_item("Fullscreen", "win.fullscreen"));
        main_menu.append_section(None, &s2);
        let s3 = gio::Menu::new();
        s3.append_item(&menu_item("Keys & mouse", "win.help"));
        s3.append_item(&menu_item("Quit", "win.quit"));
        main_menu.append_section(None, &s3);

        let gallery_menu = gio::Menu::new();
        let g1 = gio::Menu::new();
        g1.append_item(&menu_item("Open", "win.open"));
        g1.append_item(&menu_item("Open folder in file manager", "win.open-folder"));
        g1.append_item(&menu_item("Open with default app", "win.open-with"));
        g1.append_item(&menu_item("Copy path", "win.copy-path"));
        gallery_menu.append_section(None, &g1);
        let g2 = gio::Menu::new();
        g2.append_item(&menu_item("Select all", "win.select-all"));
        g2.append_item(&menu_item("Select none", "win.select-none"));
        gallery_menu.append_section(None, &g2);
        let g3 = gio::Menu::new();
        g3.append_item(&menu_item("Flat", "win.layout-set::flat"));
        g3.append_item(&menu_item("By folder", "win.layout-set::grouped"));
        gallery_menu.append_section(None, &g3);
        let g4 = gio::Menu::new();
        g4.append_item(&menu_item("Filters…", "win.filters"));
        g4.append_item(&menu_item("Rescan", "win.rescan"));
        g4.append_item(&menu_item("Open folder…", "win.open-dir"));
        gallery_menu.append_section(None, &g4);

        let viewer_menu = gio::Menu::new();
        let v1 = gio::Menu::new();
        v1.append_item(&menu_item("Next", "win.next"));
        v1.append_item(&menu_item("Previous", "win.prev"));
        viewer_menu.append_section(None, &v1);
        let scale_sub = gio::Menu::new();
        for m in SCALE_MODES {
            scale_sub.append_item(&menu_item(m.label(), &format!("win.scale-set::{}", m.id())));
        }
        let cap_sub = gio::Menu::new();
        cap_sub.append_item(&menu_item("Brief", "win.captions-set::brief"));
        cap_sub.append_item(&menu_item("Metadata", "win.captions-set::metadata"));
        cap_sub.append_item(&menu_item("Clean", "win.captions-set::clean"));
        let v2 = gio::Menu::new();
        v2.append_submenu(Some("Scale mode"), &scale_sub);
        v2.append_submenu(Some("Captions"), &cap_sub);
        v2.append_item(&menu_item("Rotate clockwise", "win.rotate-cw"));
        v2.append_item(&menu_item("Rotate counter-clockwise", "win.rotate-ccw"));
        viewer_menu.append_section(None, &v2);
        let v3 = gio::Menu::new();
        v3.append_item(&menu_item("Copy path", "win.copy-path"));
        v3.append_item(&menu_item("Open folder in file manager", "win.open-folder"));
        v3.append_item(&menu_item("Open with default app", "win.open-with"));
        viewer_menu.append_section(None, &v3);
        let v4 = gio::Menu::new();
        v4.append_item(&menu_item("Slideshow", "win.slideshow"));
        v4.append_item(&menu_item("Back to gallery", "win.back"));
        viewer_menu.append_section(None, &v4);

        // ---- gallery page ----
        let gallery = GalleryView::new();
        gallery.set_model(model.clone());
        gallery.set_thumbs(thumbs.clone());
        gallery.set_tile_step(DEFAULT_TILE_STEP);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&gallery));
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_kinetic_scrolling(true);
        scroller.add_css_class("mz-gallery-bg");

        let jump_list = gtk::ListBox::new();
        jump_list.set_selection_mode(gtk::SelectionMode::Single);
        jump_list.add_css_class("mz-jump");
        let jump_scroller = gtk::ScrolledWindow::new();
        jump_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        jump_scroller.set_child(Some(&jump_list));
        jump_scroller.set_size_request(200, -1);
        let jump_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        jump_box.add_css_class("mz-jump");
        let jh = gtk::Label::new(Some("FOLDERS"));
        jh.add_css_class("h");
        jh.set_halign(gtk::Align::Start);
        jh.set_margin_top(44);
        jump_box.append(&jh);
        jump_box.append(&jump_scroller);
        let jump_revealer = gtk::Revealer::new();
        jump_revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        jump_revealer.set_child(Some(&jump_box));
        jump_revealer.set_reveal_child(false);

        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        hbox.append(&scroller);
        hbox.append(&jump_revealer);

        let status = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        status.add_css_class("mz-status");
        let mk_status = |b: bool| {
            let x = gtk::Button::with_label("");
            x.set_can_focus(false);
            if b {
                x.add_css_class("b");
            }
            x
        };
        let status_root = mk_status(true);
        status_root.set_tooltip_text(Some("Open the root folder in the file manager"));
        let status_counts = mk_status(false);
        status_counts.set_tooltip_text(Some("Rescan"));
        let status_filter = mk_status(false);
        status_filter.set_tooltip_text(Some("Filters"));
        let status_thumbs = mk_status(false);
        status_thumbs.set_tooltip_text(Some("Thumbnail size (Ctrl+wheel)"));
        let status_section = mk_status(false);
        status_section.set_tooltip_text(Some("Folder jump list (j)"));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        for w in [&status_root, &status_counts] {
            status.append(w);
        }
        status.append(&spacer);
        for w in [&status_filter, &status_thumbs, &status_section] {
            status.append(w);
        }

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.append(&hbox);
        vbox.append(&status);

        let pill = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        pill.add_css_class("mz-pill");
        pill.set_halign(gtk::Align::Center);
        pill.set_valign(gtk::Align::Start);
        pill.set_margin_top(10);
        let pill_flat = gtk::ToggleButton::with_label("Flat");
        pill_flat.set_active(true);
        let pill_grouped = gtk::ToggleButton::with_label("By folder");
        pill_grouped.set_group(Some(&pill_flat));
        let sep = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sep.add_css_class("sep");
        let pill_filters = gtk::Button::with_label("Filters");
        let pill_help = gtk::Button::with_label("?");
        pill_help.set_action_name(Some("win.help"));
        let pill_menu = gtk::MenuButton::new();
        pill_menu.set_icon_name("view-more-symbolic");
        pill_menu.set_menu_model(Some(&main_menu));
        for w in [&pill_flat, &pill_grouped] {
            pill.append(w);
        }
        pill.append(&sep);
        pill.append(&pill_filters);
        pill.append(&pill_help);
        pill.append(&pill_menu);

        let gallery_page = gtk::Overlay::new();
        gallery_page.set_child(Some(&vbox));
        gallery_page.add_overlay(&pill);

        let filter_popover = FilterPopover::new();
        filter_popover.popover.set_parent(&pill_filters);

        // ---- viewer page ----
        let image = ImageView::new();
        image.set_focusable(true);
        let video = VideoView::new();
        let viewer_stack = gtk::Stack::new();
        viewer_stack.add_named(&image, Some("image"));
        viewer_stack.add_named(&video.widget, Some("video"));
        viewer_stack.set_hexpand(true);
        viewer_stack.set_vexpand(true);
        viewer_stack.add_css_class("mz-viewer-bg");
        let meta = MetaPanel::new();
        let vh = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        vh.append(&viewer_stack);
        vh.append(&meta.revealer);
        let captions = Captions::new();
        let toolbar = Toolbar::new(&main_menu);
        let help = Help::new();
        let viewer_page = gtk::Overlay::new();
        viewer_page.set_child(Some(&vh));
        viewer_page.add_overlay(&captions.tl);
        viewer_page.add_overlay(&captions.tr);
        viewer_page.add_overlay(&captions.bl);
        viewer_page.add_overlay(&toolbar.revealer);
        let edge_button = |label: &str, action: &str, align: gtk::Align| {
            let button = gtk::Button::with_label(label);
            button.add_css_class("mz-edge");
            button.set_action_name(Some(action));
            button.set_halign(align);
            button.set_valign(gtk::Align::Center);
            button.set_visible(false);
            button.set_can_focus(false);
            button
        };
        let edge_prev = edge_button("‹", "win.prev", gtk::Align::Start);
        let edge_next = edge_button("›", "win.next", gtk::Align::End);
        viewer_page.add_overlay(&edge_prev);
        viewer_page.add_overlay(&edge_next);

        // ---- window ----
        let stack = gtk::Stack::new();
        stack.add_named(&gallery_page, Some("gallery"));
        stack.add_named(&viewer_page, Some("viewer"));
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(120);
        let root_overlay = gtk::Overlay::new();
        root_overlay.set_child(Some(&stack));
        root_overlay.add_overlay(&help.root);
        let toast = adw::ToastOverlay::new();
        toast.set_child(Some(&root_overlay));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Mzera")
            .default_width(1280)
            .default_height(800)
            .content(&toast)
            .build();
        window.add_css_class("mz");

        // key bindings from the action table
        let mut bindings = Vec::new();
        for a in ACTIONS {
            for acc in a.accels {
                if let Some((key, mods)) = gtk::accelerator_parse(*acc) {
                    bindings.push(Binding {
                        key,
                        mods,
                        name: a.name,
                        modes: a.modes,
                    });
                }
            }
        }

        let decoder = Decoder::new();

        tlog("ui built");
        let this = Rc::new(App {
            window,
            toast,
            stack,
            gallery,
            pill_flat,
            pill_grouped,
            pill_filters,
            status_root,
            status_counts,
            status_filter,
            status_thumbs,
            status_section,
            jump_revealer,
            jump_list,
            jump_syncing: Cell::new(false),
            menu_open: Rc::new(Cell::new(false)),
            filter_popover,
            gallery_menu,
            viewer_page,
            viewer_stack,
            image,
            video,
            captions,
            meta,
            toolbar,
            edge_prev,
            edge_next,
            help,
            viewer_menu,
            decoder,
            model,
            thumbs,
            mode: Cell::new(Mode::Gallery),
            caption_mode: Cell::new(CaptionMode::Brief),
            vpos: Cell::new(None),
            slideshow: RefCell::new(None),
            slideshow_interval: Cell::new(settings.slideshow_seconds),
            toolbar_timer: RefCell::new(None),
            pointer: Cell::new((0.0, 0.0)),
            scroll_acc: Cell::new(0.0),
            zoom_base: Cell::new(1.0),
            bindings,
            actions: RefCell::new(HashMap::new()),
            pending_start: RefCell::new(None),
            pending_start_viewer: Cell::new(false),
            meta_token: Cell::new(0),
            jump_timer: RefCell::new(None),
            _theme: theme.clone(),
        });
        this.wire(app, &scroller);
        this.gallery.set_tile_step(settings.tile_step);
        this.set_grouped(settings.grouped);
        this.set_scale_mode(ScaleMode::from_id(&settings.scale_mode));
        let caption_mode = match settings.caption_mode.as_str() {
            "metadata" => CaptionMode::Metadata,
            "clean" => CaptionMode::Clean,
            _ => CaptionMode::Brief,
        };
        this.set_caption_mode(caption_mode);
        {
            let weak = Rc::downgrade(&this);
            theme.on_change(move || {
                if let Some(this) = weak.upgrade() {
                    this.gallery.queue_draw();
                    this.image.queue_draw();
                }
            });
        }
        this.update_status();
        this
    }

    // ------------------------------------------------------------------ wiring

    fn wire(self: &Rc<Self>, app: &adw::Application, scroller: &gtk::ScrolledWindow) {
        // actions
        for a in ACTIONS {
            let action = gio::SimpleAction::new(a.name, None);
            let this = self.clone();
            let name = a.name;
            action.connect_activate(move |_, _| this.act(name));
            self.window.add_action(&action);
            self.actions.borrow_mut().insert(a.name, action);
            // accelerators with a modifier are safe to register globally (menus show them)
            let acc: Vec<&str> = a
                .accels
                .iter()
                .copied()
                .filter(|s| s.contains("<Ctrl>") || s.contains("<Alt>"))
                .collect();
            if !acc.is_empty() {
                app.set_accels_for_action(&format!("win.{}", a.name), &acc);
            }
        }
        // stateful radio actions for menus
        let scale_set = gio::SimpleAction::new_stateful(
            "scale-set",
            Some(glib::VariantTy::STRING),
            &"fit".to_variant(),
        );
        {
            let this = self.clone();
            scale_set.connect_activate(move |a, p| {
                if let Some(s) = p.and_then(|v| v.get::<String>()) {
                    a.set_state(&s.to_variant());
                    this.set_scale_mode(ScaleMode::from_id(&s));
                }
            });
        }
        self.window.add_action(&scale_set);
        let captions_set = gio::SimpleAction::new_stateful(
            "captions-set",
            Some(glib::VariantTy::STRING),
            &"brief".to_variant(),
        );
        {
            let this = self.clone();
            captions_set.connect_activate(move |a, p| {
                if let Some(s) = p.and_then(|v| v.get::<String>()) {
                    a.set_state(&s.to_variant());
                    let m = match s.as_str() {
                        "metadata" => CaptionMode::Metadata,
                        "clean" => CaptionMode::Clean,
                        _ => CaptionMode::Brief,
                    };
                    this.set_caption_mode(m);
                }
            });
        }
        self.window.add_action(&captions_set);
        let layout_set = gio::SimpleAction::new_stateful(
            "layout-set",
            Some(glib::VariantTy::STRING),
            &"flat".to_variant(),
        );
        {
            let this = self.clone();
            layout_set.connect_activate(move |a, p| {
                if let Some(s) = p.and_then(|v| v.get::<String>()) {
                    a.set_state(&s.to_variant());
                    this.set_grouped(s == "grouped");
                }
            });
        }
        self.window.add_action(&layout_set);

        // window-level key capture
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        let this = self.clone();
        key.connect_key_pressed(move |_, keyval, keycode, state| {
            if this.on_key(keyval, keycode, state) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        self.window.add_controller(key);

        let mapped = Rc::new(Cell::new(false));
        let m2 = mapped.clone();
        self.window.connect_map(move |_| {
            if !m2.get() {
                m2.set(true);
                tlog("window mapped");
            }
        });
        let this = Rc::downgrade(self);
        self.window.connect_close_request(move |_| {
            if let Some(this) = this.upgrade() {
                this.quit();
            }
            glib::Propagation::Proceed
        });

        // gallery callbacks
        let this = Rc::downgrade(self);
        self.gallery.connect_activate(move |pos| {
            if let Some(this) = this.upgrade() {
                this.show(pos);
            }
        });
        let this = Rc::downgrade(self);
        self.gallery.connect_current_changed(move || {
            if let Some(this) = this.upgrade() {
                this.update_status();
            }
        });
        let this = Rc::downgrade(self);
        self.gallery.connect_scrolled(move || {
            if let Some(this) = this.upgrade() {
                this.on_gallery_scrolled();
            }
        });
        let this = Rc::downgrade(self);
        self.gallery.connect_context_menu(move |x, y, _pos| {
            if let Some(this) = this.upgrade() {
                this.popup_menu_at(x, y);
            }
        });
        let this = Rc::downgrade(self);
        self.gallery.connect_side_button(move |fwd| {
            if let Some(this) = this.upgrade() {
                this.section_step(if fwd { 1 } else { -1 });
            }
        });
        let this = Rc::downgrade(self);
        self.gallery.connect_header_clicked(move |section, action| {
            let Some(this) = this.upgrade() else { return };
            let (folder, name, path) = {
                let model = this.model.borrow();
                let Some(section) = model.sections.get(section) else {
                    return;
                };
                let folder = section.folder;
                let Some(info) = model.folders.get(folder as usize) else {
                    return;
                };
                (
                    folder,
                    model.folder_display(folder).to_string(),
                    info.path.clone(),
                )
            };
            if action == 2 {
                this.open_in_file_manager(Some(path));
            } else {
                let mut filter = this.model.borrow().filter;
                filter.folder_only = Some(folder);
                filter.root_only = false;
                let label = format!("Folder: {name}");
                this.apply_filter(filter, Some(&label));
            }
        });

        // Ctrl+wheel / Shift+wheel on the gallery scroller (capture, so it wins over scrolling)
        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        sc.set_propagation_phase(gtk::PropagationPhase::Capture);
        let this = self.clone();
        sc.connect_scroll(move |c, _dx, dy| {
            let state = c.current_event_state();
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                if dy != 0.0 {
                    this.gallery.zoom_step(if dy < 0.0 { 1 } else { -1 });
                    this.update_status();
                }
                return glib::Propagation::Stop;
            }
            if state.contains(gdk::ModifierType::SHIFT_MASK) {
                if dy != 0.0 {
                    this.section_step(if dy > 0.0 { 1 } else { -1 });
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        scroller.add_controller(sc);

        // pinch on the gallery
        let zoom = gtk::GestureZoom::new();
        let this = self.clone();
        let last = Rc::new(Cell::new(1.0f64));
        let last2 = last.clone();
        zoom.connect_begin(move |_, _| last2.set(1.0));
        zoom.connect_scale_changed(move |_, s| {
            let l = last.get();
            if s > l * 1.25 {
                this.gallery.zoom_step(1);
                last.set(s);
                this.update_status();
            } else if s < l * 0.8 {
                this.gallery.zoom_step(-1);
                last.set(s);
                this.update_status();
            }
        });
        scroller.add_controller(zoom);

        // pill + status buttons
        let this = self.clone();
        self.pill_flat.connect_toggled(move |b| {
            if b.is_active() {
                this.set_grouped(false);
            }
        });
        let this = self.clone();
        self.pill_grouped.connect_toggled(move |b| {
            if b.is_active() {
                this.set_grouped(true);
            }
        });
        let this = self.clone();
        self.pill_filters
            .connect_clicked(move |_| this.act("filters"));
        // wheel on the Filters chip cycles presets, middle click resets
        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let this = self.clone();
        sc.connect_scroll(move |_, _, dy| {
            this.filter_step(if dy > 0.0 { 1 } else { -1 });
            glib::Propagation::Stop
        });
        self.pill_filters.add_controller(sc);
        let mc = gtk::GestureClick::new();
        mc.set_button(2);
        let this = self.clone();
        mc.connect_released(move |_, _, _, _| this.act("filter-reset"));
        self.pill_filters.add_controller(mc);

        let this = self.clone();
        self.status_root
            .connect_clicked(move |_| this.open_in_file_manager(None));
        let this = self.clone();
        self.status_counts
            .connect_clicked(move |_| this.act("rescan"));
        let this = self.clone();
        self.status_filter
            .connect_clicked(move |_| this.act("filters"));
        let this = self.clone();
        self.status_thumbs.connect_clicked(move |_| {
            if !this.gallery.zoom_step(1) {
                this.gallery.set_tile_step(0);
            }
            this.update_status();
        });
        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let this = self.clone();
        sc.connect_scroll(move |_, _, dy| {
            this.gallery.zoom_step(if dy < 0.0 { 1 } else { -1 });
            this.update_status();
            glib::Propagation::Stop
        });
        self.status_thumbs.add_controller(sc);
        let this = self.clone();
        self.status_section
            .connect_clicked(move |_| this.act("jump-list"));

        // filter popover
        let this = Rc::downgrade(self);
        *self.filter_popover.on_change.borrow_mut() = Some(Rc::new(move |f| {
            if let Some(this) = this.upgrade() {
                this.apply_filter(f, None);
            }
        }));

        // jump list
        let this = self.clone();
        self.jump_list.connect_row_selected(move |_, row| {
            if this.jump_syncing.get() {
                return;
            }
            if let Some(r) = row {
                let i = r.index();
                if i >= 0 {
                    this.gallery.scroll_to_section(i as usize);
                    this.gallery.grab_focus();
                }
            }
        });

        // viewer: toolbar buttons
        let this = self.clone();
        self.toolbar.grid.connect_clicked(move |_| this.act("back"));
        let this = self.clone();
        self.toolbar.prev.connect_clicked(move |_| this.act("prev"));
        let this = self.clone();
        self.toolbar.next.connect_clicked(move |_| this.act("next"));
        let this = self.clone();
        self.toolbar
            .scale
            .connect_clicked(move |_| this.act("scale-next"));
        let sm = gtk::GestureClick::new();
        sm.set_button(3);
        let scale_menu = gio::Menu::new();
        for m in SCALE_MODES {
            scale_menu.append_item(&menu_item(m.label(), &format!("win.scale-set::{}", m.id())));
        }
        let scale_pop = gtk::PopoverMenu::from_model(Some(&scale_menu));
        scale_pop.set_parent(&self.toolbar.scale);
        sm.connect_released(move |_, _, _, _| {
            scale_pop.popup();
        });
        self.toolbar.scale.add_controller(sm);
        let this = self.clone();
        self.toolbar
            .rotate
            .connect_clicked(move |_| this.act("rotate-cw"));
        let this = self.clone();
        self.toolbar
            .captions
            .connect_clicked(move |_| this.act("captions"));
        let this = self.clone();
        self.toolbar
            .filters
            .connect_clicked(move |_| this.act("filters"));
        let this = self.clone();
        self.toolbar.help.connect_clicked(move |_| this.act("help"));
        let this = Rc::downgrade(self);
        *self.help.on_close.borrow_mut() = Some(Box::new(move || {
            if let Some(this) = this.upgrade() {
                this.set_help(false);
            }
        }));
        let this = Rc::downgrade(self);
        *self.meta.on_row.borrow_mut() = Some(Rc::new(move |k, v| {
            if let Some(this) = this.upgrade() {
                if k == "Folder" {
                    this.open_in_file_manager(None);
                } else {
                    this.copy_text(v);
                }
            }
        }));
        let this = Rc::downgrade(self);
        *self.video.on_error.borrow_mut() = Some(Rc::new(move |message| {
            if let Some(this) = this.upgrade() {
                this.toast(&message);
            }
        }));

        // viewer: decoder callback
        let this = Rc::downgrade(self);
        self.decoder.set_on_ready(move |idx, tex| {
            if let Some(this) = this.upgrade() {
                this.on_decoded(idx, tex);
            }
        });

        // viewer: pointer motion reveals the toolbar
        let motion = gtk::EventControllerMotion::new();
        let this = self.clone();
        motion.connect_motion(move |_, x, y| {
            this.pointer.set((x, y));
            let width = f64::from(this.viewer_page.width());
            this.edge_prev.set_visible(x < 64.0);
            this.edge_next.set_visible(x > width - 64.0);
            this.reveal_toolbar();
        });
        self.viewer_page.add_controller(motion);

        // viewer: clicks
        let click = gtk::GestureClick::new();
        click.set_button(0);
        let this = self.clone();
        click.connect_pressed(move |g, n_press, x, y| {
            let b = g.current_button();
            match b {
                1 => {
                    if n_press == 2 && this.mode.get() == Mode::Image {
                        this.image.toggle_real(Some((x, y)));
                        this.update_captions();
                    }
                }
                3 => this.popup_viewer_menu(x, y),
                8 | 11 => this.act("prev"),
                9 | 10 => this.act("next"),
                _ => {}
            }
        });
        let this = self.clone();
        click.connect_released(move |g, n_press, _, _| {
            if g.current_button() == 1 && n_press == 1 && this.mode.get() == Mode::Video {
                this.video.toggle();
            }
        });
        self.viewer_stack.add_controller(click);

        // viewer: left drag pans, middle drag pans / middle click goes back
        for button in [1u32, 2u32] {
            let drag = gtk::GestureDrag::new();
            drag.set_button(button);
            let this = self.clone();
            let last = Rc::new(Cell::new((0.0f64, 0.0f64)));
            let l2 = last.clone();
            drag.connect_drag_begin(move |_, _, _| l2.set((0.0, 0.0)));
            let l3 = last.clone();
            let t2 = self.clone();
            drag.connect_drag_update(move |_, dx, dy| {
                if t2.mode.get() != Mode::Image {
                    return;
                }
                let (lx, ly) = l3.get();
                if t2.image.overflows() {
                    t2.image.pan_by(dx - lx, dy - ly);
                    t2.image.set_cursor_from_name(Some("grabbing"));
                }
                l3.set((dx, dy));
            });
            drag.connect_drag_end(move |_, dx, dy| {
                this.image.set_cursor_from_name(None);
                if button == 2 && dx.abs() < 4.0 && dy.abs() < 4.0 {
                    this.act("back");
                }
            });
            self.viewer_stack.add_controller(drag);
        }

        // viewer: wheel
        let sc = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        let this = self.clone();
        sc.connect_scroll(move |c, dx, dy| {
            let state = c.current_event_state();
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
            let alt = state.contains(gdk::ModifierType::ALT_MASK);
            let wheel = c.unit() == gdk::ScrollUnit::Wheel;
            let d = if dy != 0.0 { dy } else { dx };
            if this.mode.get() == Mode::Image && ctrl {
                let f = if wheel {
                    if d < 0.0 { 1.1 } else { 1.0 / 1.1 }
                } else {
                    (1.0 - d * 0.01).clamp(0.5, 2.0)
                };
                this.image.zoom_by(f, Some(this.pointer.get()));
                this.update_captions();
                return glib::Propagation::Stop;
            }
            let navigate = alt || !(this.mode.get() == Mode::Image && this.image.overflows());
            if navigate {
                let step = if wheel {
                    d.signum()
                } else {
                    let acc = this.scroll_acc.get() + d;
                    if acc.abs() >= 40.0 {
                        this.scroll_acc.set(0.0);
                        acc.signum()
                    } else {
                        this.scroll_acc.set(acc);
                        0.0
                    }
                };
                if step > 0.0 {
                    this.act("next")
                } else if step < 0.0 {
                    this.act("prev")
                }
            } else {
                let px = if wheel { 60.0 } else { 1.0 };
                if shift {
                    this.image.pan_by(-dy * px, 0.0)
                } else {
                    this.image.pan_by(-dx * px, -dy * px)
                }
            }
            glib::Propagation::Stop
        });
        self.viewer_stack.add_controller(sc);

        // viewer: pinch
        let zoom = gtk::GestureZoom::new();
        let this = self.clone();
        zoom.connect_begin(move |_, _| this.zoom_base.set(this.image.effective_scale()));
        let this = self.clone();
        zoom.connect_scale_changed(move |g, s| {
            if this.mode.get() != Mode::Image {
                return;
            }
            let center = g.bounding_box_center();
            this.image.set_zoom(this.zoom_base.get() * s, center);
            this.update_captions();
        });
        self.viewer_stack.add_controller(zoom);

        // fullscreen changes from the compositor
        let this = self.clone();
        self.window
            .connect_fullscreened_notify(move |_| this.update_status());
        self.update_action_enabled();
    }

    // ------------------------------------------------------------------ keys

    fn mode_bits(&self) -> u8 {
        match self.mode.get() {
            Mode::Gallery => GALLERY,
            Mode::Image => VIEWER,
            Mode::Video => VIDEO,
        }
    }

    fn update_action_enabled(&self) {
        let mode = self.mode_bits();
        let actions = self.actions.borrow();
        for spec in ACTIONS {
            if let Some(action) = actions.get(spec.name) {
                action.set_enabled(spec.modes & mode != 0);
            }
        }
    }

    fn find_binding(&self, key: gdk::Key, mods: gdk::ModifierType) -> Option<&Binding> {
        let bits = self.mode_bits();
        self.bindings
            .iter()
            .find(|b| b.key == key && b.mods == mods && b.modes & bits != 0)
    }

    fn on_key(self: &Rc<Self>, keyval: gdk::Key, keycode: u32, state: gdk::ModifierType) -> bool {
        let mods = state
            & (gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::SHIFT_MASK
                | gdk::ModifierType::ALT_MASK);
        if let Some(focus) = GtkWindowExt::focus(&self.window) {
            let focus_cycle = mods.contains(gdk::ModifierType::CONTROL_MASK)
                && matches!(keyval, gdk::Key::Tab | gdk::Key::ISO_Left_Tab);
            let native_controls = focus.is::<gtk::Editable>()
                || focus.is::<gtk::Text>()
                || focus.is::<gtk::Entry>()
                || focus.is_ancestor(&self.filter_popover.popover)
                || focus.is_ancestor(&self.jump_list)
                || self.meta.contains(&focus);
            if native_controls && !focus_cycle {
                return false;
            }
        }
        if self.filter_popover.popover.is_visible() || self.menu_open.get() {
            return false;
        }
        let mut candidates: Vec<(gdk::Key, gdk::ModifierType)> = vec![(keyval, mods)];
        if let Some(l) = keyval.to_lower().into() {
            candidates.push((l, mods));
        }
        candidates.push((keyval, mods & !gdk::ModifierType::SHIFT_MASK));
        // keyboard-layout fallback: the same physical key in the first layout
        if let Some(display) = gdk::Display::default()
            && let Some(list) = display.map_keycode(keycode)
        {
            for (kk, key) in list {
                if kk.group() == 0 && kk.level() == 0 {
                    candidates.push((key, mods));
                    candidates.push((key, mods & !gdk::ModifierType::SHIFT_MASK));
                }
            }
        }
        let debug = debug();
        if debug {
            eprintln!(
                "key: {:?} mods={:?} mode={:?} candidates={:?}",
                keyval,
                mods,
                self.mode.get(),
                candidates
            );
        }
        for (k, m) in candidates {
            if let Some(b) = self.find_binding(k, m) {
                if debug {
                    eprintln!("  -> action {}", b.name);
                }
                let name = b.name;
                if self.help.root.is_visible() && name != "help" && name != "escape" {
                    self.set_help(false);
                }
                self.act(name);
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------ actions

    fn act(self: &Rc<Self>, name: &str) {
        match name {
            "next" => self.viewer_step(1),
            "space-next" => self.viewer_step(1),
            "prev" => self.viewer_step(-1),
            "skip-fwd" => self.viewer_step(10),
            "skip-back" => self.viewer_step(-10),
            "first" => self.viewer_goto(0),
            "last" => {
                let n = self.model.borrow().n_visible();
                if n > 0 {
                    self.viewer_goto(n - 1);
                }
            }
            "open" => self.gallery.activate_current(),
            "back" => self.show_gallery(),
            "next-folder" => self.section_step(1),
            "prev-folder" => self.section_step(-1),
            "parent" => {
                let root = self.model.borrow().root.clone();
                if let Some(p) = root.parent() {
                    let p = p.to_path_buf();
                    self.open_root(p, Some(root));
                }
            }
            "pan-left" => self.image.pan_by(50.0, 0.0),
            "pan-right" => self.image.pan_by(-50.0, 0.0),
            "pan-up" => self.image.pan_by(0.0, 50.0),
            "pan-down" => self.image.pan_by(0.0, -50.0),
            "scale-next" => {
                let m = self.image.cycle_mode(1);
                self.after_scale_change(m);
            }
            "scale-prev" => {
                let m = self.image.cycle_mode(-1);
                self.after_scale_change(m);
            }
            "zoom-in" => {
                self.image.zoom_by(1.25, None);
                self.update_captions();
            }
            "zoom-out" => {
                self.image.zoom_by(0.8, None);
                self.update_captions();
            }
            "zoom-reset" => {
                self.image.reset_zoom();
                self.update_captions();
            }
            "zoom-100" => {
                self.image.set_zoom(1.0, None);
                self.update_captions();
            }
            "zoom-200" => {
                self.image.set_zoom(2.0, None);
                self.update_captions();
            }
            "rotate-cw" => self.image.rotate(90),
            "rotate-ccw" => self.image.rotate(-90),
            "antialias" => {
                let on = !self.image.antialias();
                self.image.set_antialias(on);
                self.toast(if on {
                    "Antialiasing on"
                } else {
                    "Antialiasing off"
                });
            }
            "captions" => {
                let m = match self.caption_mode.get() {
                    CaptionMode::Brief => CaptionMode::Metadata,
                    CaptionMode::Metadata => CaptionMode::Clean,
                    CaptionMode::Clean => CaptionMode::Brief,
                };
                self.set_caption_mode(m);
                self.toast(match m {
                    CaptionMode::Brief => "Captions: brief",
                    CaptionMode::Metadata => "Captions: metadata",
                    CaptionMode::Clean => "Captions: clean",
                });
            }
            "metadata" => {
                let m = if self.caption_mode.get() == CaptionMode::Metadata {
                    CaptionMode::Brief
                } else {
                    CaptionMode::Metadata
                };
                self.set_caption_mode(m);
            }
            "layout" => self.set_grouped(!self.gallery.grouped()),
            "thumb-bigger" => {
                self.gallery.zoom_step(1);
                self.update_status();
            }
            "thumb-smaller" => {
                self.gallery.zoom_step(-1);
                self.update_status();
            }
            "thumb-reset" => {
                self.gallery.set_tile_step(DEFAULT_TILE_STEP);
                self.update_status();
            }
            "jump-list" => {
                let show = !self.jump_revealer.reveals_child();
                self.jump_revealer.set_reveal_child(show);
                if show {
                    self.rebuild_jump();
                    self.sync_jump();
                }
            }
            "fullscreen" => {
                if self.window.is_fullscreen() {
                    self.window.unfullscreen();
                } else {
                    self.window.fullscreen();
                }
            }
            "slideshow" => self.toggle_slideshow(),
            "focus-next" => self.focus_next(),
            "filter-next" => self.filter_step(1),
            "filter-prev" => self.filter_step(-1),
            "filter-reset" => self.apply_preset(1),
            "filters" => {
                let parent: gtk::Widget = if self.mode.get() == Mode::Gallery {
                    self.pill_filters.clone().upcast()
                } else {
                    self.reveal_toolbar();
                    self.toolbar.filters.clone().upcast()
                };
                self.filter_popover.popover.unparent();
                self.filter_popover.popover.set_parent(&parent);
                self.filter_popover.update(&self.model.borrow());
                self.filter_popover.popover.popup();
            }
            n if n.starts_with("filter-") => {
                if let Ok(k) = n["filter-".len()..].parse::<u8>() {
                    self.apply_preset(k);
                }
            }
            "select-all" => self.gallery.select_all(),
            "select-none" => self.gallery.select_none(),
            "copy-path" => {
                let paths = self.selected_paths();
                if !paths.is_empty() {
                    self.copy_paths(&paths);
                    self.toast(&format!(
                        "Copied {} path{}",
                        paths.len(),
                        if paths.len() == 1 { "" } else { "s" }
                    ));
                }
            }
            "open-folder" => self.open_in_file_manager(self.current_path()),
            "open-with" => self.open_with(),
            "open-dir" => self.pick_folder(),
            "rescan" => {
                let root = self.model.borrow().root.clone();
                if !root.as_os_str().is_empty() {
                    let cur = self.current_path();
                    self.open_root(root, cur);
                }
            }
            "play-pause" => self.video.toggle(),
            "seek-fwd" => self.video.seek_by(5.0),
            "seek-back" => self.video.seek_by(-5.0),
            "seek-fwd-big" => self.video.seek_by(30.0),
            "seek-back-big" => self.video.seek_by(-30.0),
            "seek-start" => self.video.seek_to_start(),
            "seek-end" => self.video.seek_to_end(),
            "frame-fwd" => self.video.seek_by(0.04),
            "frame-back" => self.video.seek_by(-0.04),
            "volume-up" => self.video.volume_by(0.1),
            "volume-down" => self.video.volume_by(-0.1),
            "mute" => self.video.toggle_mute(),
            "loop" => self.video.toggle_loop(),
            "help" => self.set_help(!self.help.root.is_visible()),
            "context-menu" => {
                if self.mode.get() == Mode::Gallery {
                    let (w, h) = (self.gallery.width() as f64, self.gallery.height() as f64);
                    self.popup_menu_at(w / 2.0, h / 2.0);
                } else {
                    let (w, h) = (
                        self.viewer_stack.width() as f64,
                        self.viewer_stack.height() as f64,
                    );
                    self.popup_viewer_menu(w / 2.0, h / 2.0);
                }
            }
            "escape" => self.escape(),
            "quit" => self.quit(),
            _ => {}
        }
    }

    fn focus_next(&self) {
        let focus = GtkWindowExt::focus(&self.window);
        if self.mode.get() == Mode::Gallery {
            if focus
                .as_ref()
                .is_some_and(|f| f.is_ancestor(&self.jump_list))
            {
                self.gallery.grab_focus();
            } else if focus.as_ref().is_some_and(|f| {
                f == &self.pill_flat.clone().upcast::<gtk::Widget>()
                    || f == &self.pill_grouped.clone().upcast::<gtk::Widget>()
                    || f == &self.pill_filters.clone().upcast::<gtk::Widget>()
            }) {
                if self.jump_revealer.reveals_child() {
                    let row = self
                        .jump_list
                        .selected_row()
                        .or_else(|| self.jump_list.row_at_index(0));
                    if !row.is_some_and(|row| row.grab_focus()) {
                        self.gallery.grab_focus();
                    }
                } else {
                    self.gallery.grab_focus();
                }
            } else {
                self.pill_flat.grab_focus();
            }
        } else if focus
            .as_ref()
            .is_some_and(|f| f.is_ancestor(&self.toolbar.revealer))
        {
            if !self.meta.revealer.reveals_child() || !self.meta.focus_first() {
                self.focus_viewer();
            }
        } else if focus.as_ref().is_some_and(|f| self.meta.contains(f)) {
            self.focus_viewer();
        } else {
            self.toolbar.grid.grab_focus();
        }
    }

    fn focus_viewer(&self) {
        if self.mode.get() == Mode::Video {
            self.video.widget.grab_focus();
        } else {
            self.image.grab_focus();
        }
    }

    fn escape(self: &Rc<Self>) {
        if self.help.root.is_visible() {
            self.set_help(false);
        } else if self.filter_popover.popover.is_visible() {
            self.filter_popover.popover.popdown();
        } else if self.slideshow.borrow().is_some() {
            self.toggle_slideshow();
        } else if self.mode.get() != Mode::Gallery {
            self.show_gallery();
        } else if !self.model.borrow().selected.is_empty() {
            self.gallery.select_none();
        } else if self.window.is_fullscreen() {
            self.window.unfullscreen();
        } else {
            self.quit();
        }
    }

    fn quit(&self) {
        self.save_settings();
        if let Some(app) = self.window.application() {
            app.quit();
        }
    }

    fn save_settings(&self) {
        let caption_mode = match self.caption_mode.get() {
            CaptionMode::Brief => "brief",
            CaptionMode::Metadata => "metadata",
            CaptionMode::Clean => "clean",
        };
        let settings = Settings {
            grouped: self.gallery.grouped(),
            tile_step: self.gallery.tile_step(),
            scale_mode: self.image.mode().id().into(),
            caption_mode: caption_mode.into(),
            slideshow_seconds: self.slideshow_interval.get(),
        };
        if let Err(error) = settings.save() {
            eprintln!("mzera: could not save settings: {error}");
        }
    }

    fn toast(&self, msg: &str) {
        let t = adw::Toast::new(msg);
        t.set_timeout(2);
        self.toast.add_toast(t);
    }

    fn copy_text(&self, text: &str) {
        if let Some(d) = gdk::Display::default() {
            d.clipboard().set_text(text);
        }
    }

    fn copy_paths(&self, paths: &[PathBuf]) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let text = paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        let uri_list = paths
            .iter()
            .map(|path| gio::File::for_path(path).uri())
            .collect::<Vec<_>>()
            .join("\r\n");
        let providers = [
            gdk::ContentProvider::for_value(&text.to_value()),
            gdk::ContentProvider::for_bytes(
                "text/uri-list",
                &glib::Bytes::from_owned(format!("{uri_list}\r\n").into_bytes()),
            ),
        ];
        let provider = gdk::ContentProvider::new_union(&providers);
        if display.clipboard().set_content(Some(&provider)).is_err() {
            display.clipboard().set_text(&text);
        }
    }

    fn set_help(&self, show: bool) {
        self.help.root.set_visible(show);
    }

    fn open_with(self: &Rc<Self>) {
        let Some(path) = self.current_path() else {
            return;
        };
        let file = gio::File::for_path(path);
        let launcher = gtk::FileLauncher::new(Some(&file));
        launcher.set_always_ask(true);
        let this = self.clone();
        launcher.launch(Some(&self.window), gio::Cancellable::NONE, move |result| {
            if let Err(error) = result {
                this.toast(&format!("Could not open file: {error}"));
            }
        });
    }

    // ------------------------------------------------------------------ opening

    fn pick_folder(self: &Rc<Self>) {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Choose a folder to browse");
        let start = {
            let r = self.model.borrow().root.clone();
            if r.as_os_str().is_empty() {
                glib::home_dir().join("Pictures")
            } else {
                r
            }
        };
        if start.is_dir() {
            dialog.set_initial_folder(Some(&gio::File::for_path(&start)));
        }
        let this = self.clone();
        dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, move |res| {
            if let Ok(f) = res
                && let Some(p) = f.path()
            {
                this.open_root(p, None);
            }
        });
    }

    fn open_path_in_mode(self: &Rc<Self>, p: PathBuf, forced: Option<Mode>) {
        if p.is_dir() {
            if forced == Some(Mode::Image) {
                self.open_root_with_mode(p.clone(), Some(p), true);
            } else {
                self.open_root(p, None);
            }
        } else if p.is_file()
            && scan::classify_path(&p).is_some()
            && let Some(parent) = p.parent()
        {
            if forced == Some(Mode::Gallery) {
                self.open_root(parent.to_path_buf(), None);
            } else {
                self.open_root_with_mode(parent.to_path_buf(), Some(p.clone()), true);
            }
        } else {
            self.toast(&format!("Cannot open {}", p.display()));
        }
    }

    fn open_root(self: &Rc<Self>, root: PathBuf, start: Option<PathBuf>) {
        let start_in_viewer = start.as_ref().is_some_and(|path| path.is_file());
        self.open_root_with_mode(root, start, start_in_viewer);
    }

    fn open_root_with_mode(
        self: &Rc<Self>,
        root: PathBuf,
        start: Option<PathBuf>,
        start_in_viewer: bool,
    ) {
        let generation = {
            let mut m = self.model.borrow_mut();
            m.reset(root.clone());
            m.generation
        };
        self.thumbs.set_generation(generation);
        self.gallery.set_generation(generation);
        self.decoder.set_generation(generation);
        *self.pending_start.borrow_mut() = start;
        self.pending_start_viewer.set(start_in_viewer);
        self.vpos.set(None);
        self.show_gallery();
        self.gallery.items_changed();
        self.window
            .set_title(Some(&format!("{} — Mzera", self.model.borrow().root_name)));
        self.update_status();
        self.rebuild_jump();

        let rx = scan::spawn(root);
        let this = self.clone();
        let t_scan = std::time::Instant::now();
        let mut first_batch = true;
        glib::spawn_future_local(async move {
            loop {
                let Ok(first) = rx.recv().await else { break };
                if this.model.borrow().generation != generation {
                    break;
                }
                let mut msgs = vec![first];
                while let Ok(m) = rx.try_recv() {
                    msgs.push(m);
                }
                let mut done = false;
                let mut warnings = 0u32;
                {
                    let mut m = this.model.borrow_mut();
                    for msg in msgs {
                        match msg {
                            Msg::Folder(f) => m.add_folder(f),
                            Msg::Entries(v) => m.append(v),
                            Msg::Warnings(n) => warnings = warnings.saturating_add(n),
                            Msg::Done => {
                                m.scanning = false;
                                done = true;
                            }
                        }
                    }
                }
                if warnings > 0 {
                    this.toast(&format!(
                        "Could not read {} filesystem entr{}",
                        group_digits(warnings),
                        if warnings == 1 { "y" } else { "ies" }
                    ));
                }
                this.gallery.items_changed();
                this.update_status();
                this.check_pending_start();
                if first_batch && !this.model.borrow().entries.is_empty() {
                    first_batch = false;
                    tlog(&format!(
                        "first scan batch applied ({} entries) after {:.1} ms",
                        this.model.borrow().entries.len(),
                        t_scan.elapsed().as_secs_f64() * 1000.0
                    ));
                }
                if done {
                    tlog(&format!(
                        "scan done: {} entries, {} folders in {:.1} ms",
                        this.model.borrow().entries.len(),
                        this.model.borrow().folders.len(),
                        t_scan.elapsed().as_secs_f64() * 1000.0
                    ));
                    this.filter_popover.update(&this.model.borrow());
                    this.rebuild_jump();
                    break;
                } else {
                    this.schedule_jump_rebuild();
                }
            }
        });
    }

    fn check_pending_start(self: &Rc<Self>) {
        let want = self.pending_start.borrow().clone();
        let Some(want) = want else { return };
        let pos = {
            let m = self.model.borrow();
            let is_dir = want.is_dir();
            m.entries
                .iter()
                .position(|e| {
                    if is_dir {
                        e.path.starts_with(&want)
                    } else {
                        e.path == want
                    }
                })
                .and_then(|i| m.pos_of_master(i as u32))
        };
        if let Some(pos) = pos {
            *self.pending_start.borrow_mut() = None;
            if self.pending_start_viewer.replace(false) {
                self.show(pos);
            } else {
                self.gallery.set_current(Some(pos), true);
            }
        }
    }

    // ------------------------------------------------------------------ viewer

    fn show(self: &Rc<Self>, pos: u32) {
        let (path, kind, master, n) = {
            let m = self.model.borrow();
            if pos >= m.n_visible() {
                return;
            }
            let e = m.entry_at(pos);
            (e.path.clone(), e.kind(), m.master(pos), m.n_visible())
        };
        let _ = n;
        self.vpos.set(Some(pos));
        {
            let mut m = self.model.borrow_mut();
            m.selected.clear();
            m.selected.insert(master);
        }
        self.gallery.set_current(Some(pos), true);
        if self.stack.visible_child_name().as_deref() != Some("viewer") {
            self.stack.set_visible_child_name("viewer");
        }
        match kind {
            Kind::Video => {
                self.mode.set(Mode::Video);
                self.viewer_stack.set_visible_child_name("video");
                self.video.open(&path);
            }
            Kind::Image => {
                self.mode.set(Mode::Image);
                self.video.stop();
                self.viewer_stack.set_visible_child_name("image");
                match self.decoder.get(master) {
                    Some(t) => self.image.set_texture(Some(t)),
                    None => {
                        self.image.set_texture(None);
                        self.decoder.request(master, path.clone());
                    }
                }
            }
        }
        self.update_action_enabled();
        self.image.grab_focus();
        // preload ring
        let mut keep = vec![master];
        {
            let m = self.model.borrow();
            for d in [1i64, -1, 2, -2] {
                let p = pos as i64 + d;
                if p < 0 || p >= m.n_visible() as i64 {
                    continue;
                }
                let e = m.entry_at(p as u32);
                if e.kind() == Kind::Image {
                    let mm = m.master(p as u32);
                    keep.push(mm);
                    self.decoder.request(mm, e.path.clone());
                }
            }
        }
        self.decoder.cancel_except(&keep);
        self.update_captions();
        self.update_meta();
        self.reveal_toolbar();
    }

    fn on_decoded(self: &Rc<Self>, idx: u32, tex: Option<gdk::Texture>) {
        if self.mode.get() != Mode::Image {
            return;
        }
        let Some(pos) = self.vpos.get() else { return };
        let m = self.model.borrow();
        if pos >= m.n_visible() || m.master(pos) != idx {
            return;
        }
        drop(m);
        match tex {
            Some(t) => self.image.set_texture(Some(t)),
            None => {
                self.image.set_texture(None);
                self.toast("Could not decode this image");
            }
        }
        self.update_captions();
        self.update_meta();
    }

    fn viewer_step(self: &Rc<Self>, delta: i64) {
        let n = self.model.borrow().n_visible() as i64;
        if n == 0 {
            return;
        }
        if self.mode.get() == Mode::Gallery {
            return;
        }
        let cur = self.vpos.get().map(|p| p as i64).unwrap_or(0);
        let target = (cur + delta).clamp(0, n - 1);
        if target != cur {
            self.show(target as u32);
            self.restart_slideshow_timer();
        }
    }

    fn viewer_goto(self: &Rc<Self>, pos: u32) {
        if self.mode.get() == Mode::Gallery {
            return;
        }
        self.show(pos);
    }

    fn show_gallery(self: &Rc<Self>) {
        self.video.stop();
        if let Some(source) = self.slideshow.borrow_mut().take() {
            source.remove();
        }
        self.mode.set(Mode::Gallery);
        self.update_action_enabled();
        self.stack.set_visible_child_name("gallery");
        self.image.set_cursor_from_name(None);
        if let Some(p) = self.vpos.get() {
            self.gallery.set_current(Some(p), true);
        }
        self.gallery.grab_focus();
        self.update_status();
    }

    fn after_scale_change(&self, m: ScaleMode) {
        self.toolbar.scale.set_label(m.label());
        if let Some(a) = self.window.lookup_action("scale-set")
            && let Some(a) = a.downcast_ref::<gio::SimpleAction>()
        {
            a.set_state(&m.id().to_variant());
        }
        self.toast(&format!("Scale: {}", m.label()));
        self.update_captions();
    }

    fn set_scale_mode(&self, m: ScaleMode) {
        self.image.set_mode(m);
        self.toolbar.scale.set_label(m.label());
        if let Some(action) = self.window.lookup_action("scale-set")
            && let Some(action) = action.downcast_ref::<gio::SimpleAction>()
        {
            action.set_state(&m.id().to_variant());
        }
        self.update_captions();
    }

    fn set_caption_mode(self: &Rc<Self>, m: CaptionMode) {
        self.caption_mode.set(m);
        if let Some(a) = self.window.lookup_action("captions-set")
            && let Some(a) = a.downcast_ref::<gio::SimpleAction>()
        {
            let id = match m {
                CaptionMode::Brief => "brief",
                CaptionMode::Metadata => "metadata",
                CaptionMode::Clean => "clean",
            };
            a.set_state(&id.to_variant());
        }
        self.meta
            .revealer
            .set_reveal_child(m == CaptionMode::Metadata);
        self.update_captions();
        self.update_meta();
    }

    fn update_captions(&self) {
        let mode = self.caption_mode.get();
        let Some(pos) = self.vpos.get() else {
            self.captions.set(None, None, None);
            return;
        };
        let m = self.model.borrow();
        if pos >= m.n_visible() {
            self.captions.set(None, None, None);
            return;
        }
        let e = m.entry_at(pos);
        let n = m.n_visible();
        match mode {
            CaptionMode::Clean => self.captions.set(None, None, None),
            CaptionMode::Brief => {
                self.captions
                    .set(Some(&e.name), Some(&format!("{} / {}", pos + 1, n)), None)
            }
            CaptionMode::Metadata => {
                let bl = if self.mode.get() == Mode::Image && self.image.texture().is_some() {
                    format!(
                        "{} · {:.0} %",
                        self.image.mode().label(),
                        self.image.effective_scale() * 100.0
                    )
                } else {
                    String::new()
                };
                self.captions.set(
                    Some(&e.name),
                    Some(&format!("{} / {}", pos + 1, n)),
                    Some(&bl),
                );
            }
        }
    }

    fn update_meta(self: &Rc<Self>) {
        if self.caption_mode.get() != CaptionMode::Metadata {
            return;
        }
        let Some(pos) = self.vpos.get() else { return };
        let (path, name, folder, group) = {
            let m = self.model.borrow();
            if pos >= m.n_visible() {
                return;
            }
            let e = m.entry_at(pos);
            (
                e.path.clone(),
                e.name.to_string(),
                m.folder_display(e.folder).to_string(),
                e.group.label(),
            )
        };
        let mut rows: Vec<(String, String)> = vec![
            ("File".into(), name),
            ("Folder".into(), folder),
            ("Path".into(), short_home(&path)),
        ];
        let dims = if self.mode.get() == Mode::Image {
            self.image
                .texture()
                .map(|t| format!(" · {} × {}", t.width(), t.height()))
                .unwrap_or_default()
        } else {
            String::new()
        };
        rows.push(("Format".into(), format!("{group}{dims}")));
        if let Some((size, mtime)) = thumbs::file_info(&path) {
            rows.push(("Size".into(), thumbs::human_size(size)));
            if let Ok(dt) = glib::DateTime::from_unix_local(mtime)
                && let Ok(s) = dt.format("%Y-%m-%d %H:%M")
            {
                rows.push(("Modified".into(), s.to_string()));
            }
        }
        if self.mode.get() == Mode::Image {
            let pos_s = {
                let m = self.model.borrow();
                format!("{} / {}", pos + 1, m.n_visible())
            };
            rows.push(("Position".into(), pos_s));
        }
        self.meta.set(&rows);
        // EXIF off-thread
        let token = self.meta_token.get() + 1;
        self.meta_token.set(token);
        if self.mode.get() != Mode::Image {
            return;
        }
        let this = self.clone();
        let p2 = path.clone();
        glib::spawn_future_local(async move {
            let info = gio::spawn_blocking(move || crate::exifinfo::read(&p2))
                .await
                .unwrap_or_default();
            if this.meta_token.get() != token {
                return;
            }
            let mut extra: Vec<(String, String)> = Vec::new();
            if let Some(t) = info.taken {
                extra.push(("Taken".into(), t));
            }
            if let Some(c) = info.camera {
                extra.push(("Camera".into(), c));
            }
            if let Some(l) = info.lens {
                extra.push(("Lens".into(), l));
            }
            if let Some(x) = info.exposure {
                extra.push(("Exposure".into(), x));
            }
            if !extra.is_empty() {
                let mut all = rows.clone();
                all.extend(extra);
                this.meta.set(&all);
            }
        });
    }

    fn reveal_toolbar(self: &Rc<Self>) {
        self.toolbar.revealer.set_reveal_child(true);
        self.image.set_cursor_from_name(None);
        if let Some(s) = self.toolbar_timer.borrow_mut().take() {
            s.remove();
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(2000), move || {
            *this.toolbar_timer.borrow_mut() = None;
            if this.mode.get() == Mode::Gallery {
                return;
            }
            let (_, y) = this.pointer.get();
            if y < 90.0 {
                this.reveal_toolbar();
                return;
            }
            this.toolbar.revealer.set_reveal_child(false);
            this.edge_prev.set_visible(false);
            this.edge_next.set_visible(false);
            if this.mode.get() == Mode::Image {
                this.image.set_cursor_from_name(Some("none"));
            }
        });
        *self.toolbar_timer.borrow_mut() = Some(id);
    }

    fn toggle_slideshow(self: &Rc<Self>) {
        if let Some(s) = self.slideshow.borrow_mut().take() {
            s.remove();
            self.toast("Slideshow stopped");
            return;
        }
        if self.mode.get() == Mode::Gallery {
            self.gallery.activate_current();
            if self.mode.get() == Mode::Gallery {
                return;
            }
        }
        self.toast("Slideshow · 4 s · s to stop");
        self.start_slideshow_timer();
    }

    fn restart_slideshow_timer(self: &Rc<Self>) {
        // only re-arm when a slideshow is running
        if self.slideshow.borrow().is_none() {
            return;
        }
        self.start_slideshow_timer();
    }

    fn start_slideshow_timer(self: &Rc<Self>) {
        if let Some(s) = self.slideshow.borrow_mut().take() {
            s.remove();
        }
        let this = self.clone();
        let interval = self.slideshow_interval.get();
        let id = glib::timeout_add_local(std::time::Duration::from_secs(interval), move || {
            let n = this.model.borrow().n_visible() as i64;
            let cur = this.vpos.get().map(|p| p as i64).unwrap_or(0);
            if n == 0 {
                return glib::ControlFlow::Continue;
            }
            let next = if cur + 1 >= n { 0 } else { cur + 1 };
            this.show(next as u32);
            glib::ControlFlow::Continue
        });
        *self.slideshow.borrow_mut() = Some(id);
    }

    // ------------------------------------------------------------------ gallery helpers

    fn set_grouped(&self, grouped: bool) {
        if self.gallery.grouped() == grouped {
            return;
        }
        self.gallery.set_grouped(grouped);
        if grouped {
            self.pill_grouped.set_active(true);
        } else {
            self.pill_flat.set_active(true);
        }
        if let Some(a) = self.window.lookup_action("layout-set")
            && let Some(a) = a.downcast_ref::<gio::SimpleAction>()
        {
            a.set_state(&(if grouped { "grouped" } else { "flat" }).to_variant());
        }
        self.update_status();
    }

    fn section_step(self: &Rc<Self>, dir: i64) {
        let n_sections = self.model.borrow().sections.len() as i64;
        if n_sections == 0 {
            return;
        }
        if self.mode.get() == Mode::Gallery {
            let cur = self.gallery.top_section().unwrap_or(0) as i64;
            let cur = match self.gallery.current() {
                Some(c) => self.model.borrow().section_of(c) as i64,
                None => cur,
            };
            let t = (cur + dir).clamp(0, n_sections - 1);
            self.gallery.scroll_to_section(t as usize);
            self.update_status();
        } else {
            let Some(pos) = self.vpos.get() else { return };
            let (cur, start) = {
                let m = self.model.borrow();
                let s = m.section_of(pos) as i64;
                let t = (s + dir).clamp(0, n_sections - 1);
                (s, m.sections[t as usize].start)
            };
            let _ = cur;
            self.show(start);
        }
    }

    fn filter_step(self: &Rc<Self>, dir: i8) {
        let groups = self.model.borrow().filter.groups;
        let cur = (1..=N_PRESETS)
            .find(|&k| preset(k).map(|p| p.1 == groups).unwrap_or(false))
            .unwrap_or(1) as i8;
        let mut k = cur + dir;
        if k < 1 {
            k = N_PRESETS as i8;
        }
        if k > N_PRESETS as i8 {
            k = 1;
        }
        self.apply_preset(k as u8);
    }

    fn apply_preset(self: &Rc<Self>, k: u8) {
        let Some((label, groups)) = preset(k) else {
            return;
        };
        let mut f = self.model.borrow().filter;
        f.groups = groups;
        self.apply_filter(f, Some(label));
    }

    fn apply_filter(self: &Rc<Self>, f: Filter, label: Option<&str>) {
        let keep_master = if self.mode.get() == Mode::Gallery {
            self.gallery.current_master()
        } else {
            self.vpos.get().and_then(|p| {
                let m = self.model.borrow();
                if p < m.n_visible() {
                    Some(m.master(p))
                } else {
                    None
                }
            })
        };
        {
            let mut m = self.model.borrow_mut();
            m.set_filter(f);
        }
        self.gallery.items_changed();
        if self.mode.get() == Mode::Gallery
            && let Some(master) = keep_master
        {
            let pos = self.model.borrow().nearest_pos(master);
            self.gallery.set_current(pos, false);
        }
        self.filter_popover.update(&self.model.borrow());
        let (n, shown) = {
            let m = self.model.borrow();
            (m.entries.len(), m.n_visible())
        };
        let desc = label
            .map(|s| s.to_string())
            .unwrap_or_else(|| self.model.borrow().filter.describe());
        self.toast(&format!(
            "{desc} — {} of {}",
            group_digits(shown),
            group_digits(n as u32)
        ));
        // keep the viewer on the same item if still visible, else nearest
        if self.mode.get() != Mode::Gallery
            && let Some(km) = keep_master
        {
            let (pos, hidden) = {
                let m = self.model.borrow();
                match m.pos_of_master(km) {
                    Some(p) => (Some(p), false),
                    None => (m.nearest_pos(km), true),
                }
            };
            match pos {
                Some(p) => {
                    if hidden {
                        self.show(p);
                    } else {
                        self.vpos.set(Some(p));
                        self.update_captions();
                    }
                }
                None => self.show_gallery(),
            }
        }
        self.update_status();
        self.rebuild_jump();
    }

    fn selected_paths(&self) -> Vec<PathBuf> {
        let m = self.model.borrow();
        if self.mode.get() != Mode::Gallery {
            return self
                .vpos
                .get()
                .filter(|&p| p < m.n_visible())
                .map(|p| vec![m.entry_at(p).path.clone()])
                .unwrap_or_default();
        }
        if !m.selected.is_empty() {
            let mut v: Vec<u32> = m.selected.iter().copied().collect();
            v.sort_unstable();
            return v
                .iter()
                .map(|&i| m.entries[i as usize].path.clone())
                .collect();
        }
        self.gallery
            .current()
            .filter(|&p| p < m.n_visible())
            .map(|p| vec![m.entry_at(p).path.clone()])
            .unwrap_or_default()
    }

    fn current_path(&self) -> Option<PathBuf> {
        self.selected_paths().into_iter().next()
    }

    fn open_in_file_manager(&self, file: Option<PathBuf>) {
        let root = self.model.borrow().root.clone();
        let target = file.unwrap_or(root);
        let uri = gio::File::for_path(&target).uri().to_string();
        // Try FileManager1.ShowItems (selects the file), fall back to opening the folder.
        let shown = (|| -> Option<()> {
            let conn = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
            let args = (vec![uri.clone()], String::new()).to_variant();
            conn.call_sync(
                Some("org.freedesktop.FileManager1"),
                "/org/freedesktop/FileManager1",
                "org.freedesktop.FileManager1",
                if target.is_dir() {
                    "ShowFolders"
                } else {
                    "ShowItems"
                },
                Some(&args),
                None,
                gio::DBusCallFlags::NONE,
                2000,
                gio::Cancellable::NONE,
            )
            .ok()?;
            Some(())
        })();
        if shown.is_none() {
            let folder = if target.is_dir() {
                target
            } else {
                target.parent().map(|p| p.to_path_buf()).unwrap_or(target)
            };
            let _ = gio::AppInfo::launch_default_for_uri(
                &gio::File::for_path(&folder).uri(),
                gio::AppLaunchContext::NONE,
            );
        }
    }

    fn popup_menu_at(&self, x: f64, y: f64) {
        let pop = gtk::PopoverMenu::from_model(Some(&self.gallery_menu));
        pop.set_parent(&self.gallery);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        self.menu_open.set(true);
        let menu_open = self.menu_open.clone();
        pop.connect_closed(move |p| {
            menu_open.set(false);
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
    }

    fn popup_viewer_menu(&self, x: f64, y: f64) {
        let pop = gtk::PopoverMenu::from_model(Some(&self.viewer_menu));
        pop.set_parent(&self.viewer_stack);
        pop.set_has_arrow(false);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        self.menu_open.set(true);
        let menu_open = self.menu_open.clone();
        pop.connect_closed(move |p| {
            menu_open.set(false);
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
    }

    fn on_gallery_scrolled(&self) {
        if self.gallery.grouped() || self.jump_revealer.reveals_child() {
            self.sync_jump();
            self.update_status();
        }
    }

    fn sync_jump(&self) {
        if !self.jump_revealer.reveals_child() {
            return;
        }
        if let Some(i) = self.gallery.top_section()
            && let Some(row) = self.jump_list.row_at_index(i as i32)
            && self.jump_list.selected_row().as_ref() != Some(&row)
        {
            self.jump_syncing.set(true);
            self.jump_list.select_row(Some(&row));
            self.jump_syncing.set(false);
        }
    }

    fn schedule_jump_rebuild(self: &Rc<Self>) {
        if self.jump_timer.borrow().is_some() {
            return;
        }
        let this = self.clone();
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
            *this.jump_timer.borrow_mut() = None;
            this.rebuild_jump();
        });
        *self.jump_timer.borrow_mut() = Some(id);
    }

    fn rebuild_jump(&self) {
        if !self.jump_revealer.reveals_child() {
            return;
        }
        self.jump_syncing.set(true);
        while let Some(c) = self.jump_list.first_child() {
            self.jump_list.remove(&c);
        }
        {
            let m = self.model.borrow();
            for s in &m.sections {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                let name = m.folder_display(s.folder);
                let depth = m.folders[s.folder as usize].rel.matches(" / ").count();
                let l = gtk::Label::new(Some(name.rsplit(" / ").next().unwrap_or(name)));
                l.set_xalign(0.0);
                l.set_hexpand(true);
                l.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                l.set_margin_start((depth as i32) * 12);
                let n = gtk::Label::new(Some(&group_digits(s.len)));
                n.add_css_class("n");
                row.append(&l);
                row.append(&n);
                self.jump_list.append(&row);
            }
        }
        self.jump_syncing.set(false);
        self.sync_jump();
    }

    fn update_status(&self) {
        let m = self.model.borrow();
        self.status_root.set_label(&m.root_name);
        let mut counts = format!(
            "{} files · {} folders",
            group_digits(m.entries.len() as u32),
            m.folders.len()
        );
        if m.scanning {
            counts.push_str(" · scanning…");
        }
        self.status_counts.set_label(&counts);
        let excluded = crate::scan::GROUPS
            .iter()
            .filter(|g| m.filter.groups & g.bit() == 0)
            .count();
        let active_scopes =
            m.filter.root_only as usize + usize::from(m.filter.folder_only.is_some());
        let chip = if excluded == 0 && active_scopes == 0 {
            "Filters".to_string()
        } else {
            format!("Filters · {}", excluded + active_scopes)
        };
        self.pill_filters.set_label(&chip);
        self.toolbar.filters.set_label(&chip);
        let f = if m.filter == Filter::default() {
            format!("{} shown", group_digits(m.n_visible()))
        } else {
            format!(
                "{} — {} shown",
                m.filter.describe(),
                group_digits(m.n_visible())
            )
        };
        self.status_filter.set_label(&f);
        self.status_thumbs
            .set_label(&format!("thumbs {} px", self.gallery.tile_size()));
        if self.gallery.grouped() {
            let si = self
                .gallery
                .current()
                .map(|c| m.section_of(c))
                .or_else(|| self.gallery.top_section())
                .unwrap_or(0);
            self.status_section
                .set_label(&format!("folder {} / {}", si + 1, m.sections.len()));
            self.status_section.set_visible(true);
        } else {
            self.status_section.set_visible(false);
        }
    }
}
