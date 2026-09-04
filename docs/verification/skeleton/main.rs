use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const N: u32 = 35_000;
const PER_FOLDER: u32 = 500;

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

fn folder_of(s: &str) -> String {
    match s.rfind('/') {
        Some(i) => s[..i].to_string(),
        None => String::new(),
    }
}

fn main() -> glib::ExitCode {
    let t0 = Instant::now();
    let app = adw::Application::builder()
        .application_id("dev.mzera.Verify")
        .build();
    app.connect_activate(move |app| build_ui(app, t0));
    app.run()
}

fn build_ui(app: &adw::Application, t0: Instant) {
    eprintln!("[{:>7.1} ms] activate", ms(t0));

    // ---- model: 35 000 StringObjects, in folder order ----
    let store = gio::ListStore::new::<gtk::StringObject>();
    let items: Vec<gtk::StringObject> = (0..N)
        .map(|i| gtk::StringObject::new(&format!("folder{:03}/file{:05}.webp", i / PER_FOLDER, i)))
        .collect();
    store.extend_from_slice(&items);
    eprintln!("[{:>7.1} ms] model filled: {} items", ms(t0), store.n_items());

    // ---- sections by folder: SortListModel implements SectionModel when a section sorter is set ----
    let section_sorter = gtk::CustomSorter::new(|a, b| {
        let fa = folder_of(&a.downcast_ref::<gtk::StringObject>().unwrap().string());
        let fb = folder_of(&b.downcast_ref::<gtk::StringObject>().unwrap().string());
        fa.cmp(&fb).into()
    });
    let sorted = gtk::SortListModel::builder()
        .model(&store)
        .section_sorter(&section_sorter)
        .build();
    let selection = gtk::NoSelection::new(Some(sorted.clone()));

    // ---- tile factory ----
    let tile_factory = gtk::SignalListItemFactory::new();
    tile_factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let label = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .width_chars(14)
            .margin_top(24)
            .margin_bottom(24)
            .build();
        item.set_child(Some(&label));
    });
    tile_factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<gtk::StringObject>().unwrap();
        let label = item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_text(&obj.string());
    });

    // ---- header factory (GridView::set_header_factory, GTK >= 4.12) ----
    let header_factory = gtk::SignalListItemFactory::new();
    header_factory.connect_setup(|_, item| {
        let header = item.downcast_ref::<gtk::ListHeader>().unwrap();
        let label = gtk::Label::builder()
            .xalign(0.0)
            .css_classes(["heading"])
            .margin_top(12)
            .margin_bottom(4)
            .margin_start(8)
            .build();
        header.set_child(Some(&label));
    });
    header_factory.connect_bind(|_, item| {
        let header = item.downcast_ref::<gtk::ListHeader>().unwrap();
        let obj = header.item().and_downcast::<gtk::StringObject>().unwrap();
        let label = header.child().and_downcast::<gtk::Label>().unwrap();
        let text = format!("{}  ({} items)", folder_of(&obj.string()), header.n_items());
        static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
        if FIRST.swap(false, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("header factory bound (sections work): first header = {:?}, start={} end={}", text, header.start(), header.end());
        }
        label.set_text(&text);
    });

    let grid = gtk::GridView::builder()
        .model(&selection)
        .factory(&tile_factory)
        .min_columns(4)
        .max_columns(12)
        .build();

    // GTK 4.22.4 has no gtk_grid_view_set_header_factory (checked headers + libgtk-4 symbols);
    // sections/header factories exist only on ListView, ColumnView and DropDown.
    // So sections are exercised on a ListView sidebar bound to the same section-sorted model.
    let list = gtk::ListView::builder()
        .model(&gtk::NoSelection::new(Some(sorted)))
        .factory(&tile_factory)
        .header_factory(&header_factory)
        .build();
    let list_scroller = gtk::ScrolledWindow::builder().child(&list).width_request(260).build();
    let grid_scroller = gtk::ScrolledWindow::builder().child(&grid).hexpand(true).vexpand(true).build();
    let paned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .start_child(&list_scroller)
        .end_child(&grid_scroller)
        .position(280)
        .shrink_start_child(false)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&paned));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("mzera-verify")
        .default_width(1200)
        .default_height(800)
        .content(&toolbar)
        .build();

    // ---- timing: map, first tick, first after-paint ----
    window.connect_map(move |_| eprintln!("[{:>7.1} ms] window map", ms(t0)));
    let ticked = Rc::new(Cell::new(false));
    window.add_tick_callback(move |_, clock| {
        if !ticked.get() {
            ticked.set(true);
            eprintln!("[{:>7.1} ms] first tick (frame {})", ms(t0), clock.frame_counter());
            let painted = Rc::new(Cell::new(false));
            clock.connect_after_paint(move |_| {
                if !painted.get() {
                    painted.set(true);
                    eprintln!("[{:>7.1} ms] first after-paint", ms(t0));
                }
            });
        }
        glib::ControlFlow::Continue
    });

    // ---- optional glycin runtime check: MZERA_GLYCIN_TEST=/path/to/image ----
    if let Ok(path) = std::env::var("MZERA_GLYCIN_TEST") {
        glib::spawn_future_local(async move {
            let t = Instant::now();
            let loader = glycin::Loader::new(gio::File::for_path(&path));
            match loader.load().await {
                Ok(image) => match image.next_frame().await {
                    Ok(frame) => {
                        let tex = frame.texture();
                        eprintln!(
                            "glycin: {} -> {}x{} texture in {:.1} ms (format {:?}, mime {:?})",
                            path,
                            tex.width(),
                            tex.height(),
                            ms(t),
                            image.details().info_format_name(), image.mime_type(),
                        );
                    }
                    Err(e) => eprintln!("glycin next_frame error: {e}"),
                },
                Err(e) => eprintln!("glycin load error: {e}"),
            }
        });
    }

    let app2 = app.clone();
    glib::timeout_add_local_once(Duration::from_millis(1000), move || app2.quit());

    window.present();
    eprintln!("[{:>7.1} ms] present() returned", ms(t0));
}

// ---- API presence check (task 4): only needs to type-check ----
#[allow(dead_code)]
fn api_presence_check() {
    // GridView::set_header_factory does NOT exist (neither in gtk4-rs 0.11.4 nor in GTK 4.22.4 C API)
    let _ = |l: &gtk::ListView, f: &gtk::SignalListItemFactory| l.set_header_factory(Some(f));
    let _ = |m: &gtk::SectionModel| m.section(0);
    let _ = |m: &gtk::FilterListModel| m.set_incremental(true);
    let _ = |p: &str| gdk::Texture::from_filename(p);
    let _ = |b: &glib::Bytes| gdk::MemoryTexture::new(1, 1, gdk::MemoryFormat::R8g8b8a8, b, 4);
    let _ = || gdk::DmabufTextureBuilder::new();
    let _ = |p: &gtk::Picture, t: &gdk::Texture| p.set_paintable(Some(t));
    let _ = |p: &str| gtk::MediaFile::for_filename(p);
    let _ = || gtk::Video::new();
    let _ = || gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let _ = || gtk::GestureDrag::new();
    let _ = || gtk::GestureZoom::new();
    let _ = || gtk::ShortcutController::new();
    let _ = |f: gio::File| glycin::Loader::new(f);
}
