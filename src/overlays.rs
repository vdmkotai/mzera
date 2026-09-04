//! Help overlay, floating toolbar, captions and metadata panel.

use crate::actions::{self, ACTIONS};
use gtk::prelude::*;

type CloseCallback = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn()>>>>;
type MetaRowCallback = std::rc::Rc<std::cell::RefCell<Option<std::rc::Rc<dyn Fn(&str, &str)>>>>;

/// Full-window help overlay generated from the action table.
pub struct Help {
    pub root: gtk::Box,
    pub on_close: CloseCallback,
}

impl Help {
    pub fn new() -> Help {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("mz-dim");
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_halign(gtk::Align::Fill);
        root.set_valign(gtk::Align::Fill);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
        card.add_css_class("mz-help");
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Center);
        card.set_margin_top(40);
        card.set_margin_bottom(40);

        let title = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let t = gtk::Label::new(Some("Keys & mouse"));
        t.add_css_class("t");
        t.set_halign(gtk::Align::Start);
        t.set_hexpand(true);
        let hint = gtk::Label::new(Some("h · ? · Esc close"));
        hint.add_css_class("m");
        title.append(&t);
        title.append(&hint);
        card.append(&title);

        let columns = gtk::Box::new(gtk::Orientation::Horizontal, 28);
        let groups = [
            "Navigate", "View", "Filters", "Gallery", "Files", "Video", "App",
        ];
        let col_a = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let col_b = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let col_c = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for (i, g) in groups.iter().enumerate() {
            let target = match i {
                0 => &col_a,
                1 | 2 => &col_b,
                _ => &col_c,
            };
            let gl = gtk::Label::new(Some(&g.to_uppercase()));
            gl.add_css_class("g");
            gl.set_halign(gtk::Align::Start);
            target.append(&gl);
            let grid = gtk::Grid::new();
            grid.set_column_spacing(12);
            grid.set_row_spacing(1);
            let mut row = 0;
            for a in ACTIONS.iter().filter(|a| a.group == *g) {
                if a.name == "escape" {
                    continue;
                }
                let l = gtk::Label::new(Some(a.label));
                l.add_css_class("a");
                l.set_halign(gtk::Align::Start);
                l.set_xalign(0.0);
                let keys = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                let mut seen = std::collections::HashSet::new();
                for k in a.accels.iter().take(3) {
                    let label = actions::accel_label(k);
                    if !seen.insert(label.clone()) {
                        continue;
                    }
                    let kl = gtk::Label::new(Some(&label));
                    kl.add_css_class("k");
                    keys.append(&kl);
                }
                let m = gtk::Label::new(Some(a.mouse));
                m.add_css_class("m");
                m.set_halign(gtk::Align::Start);
                m.set_xalign(0.0);
                grid.attach(&l, 0, row, 1, 1);
                grid.attach(&keys, 1, row, 1, 1);
                grid.attach(&m, 2, row, 1, 1);
                row += 1;
            }
            target.append(&grid);
        }
        columns.append(&col_a);
        columns.append(&col_b);
        columns.append(&col_c);
        card.append(&columns);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&card));
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        root.append(&scroller);
        root.set_visible(false);

        let on_close: CloseCallback = Default::default();
        let click = gtk::GestureClick::new();
        let oc = on_close.clone();
        let card_w = card.clone();
        click.connect_released(move |g, _, x, y| {
            // close when clicking outside the card
            let Some(w) = g.widget() else {
                return;
            };
            if let Some(p) =
                w.compute_point(&card_w, &gtk::graphene::Point::new(x as f32, y as f32))
            {
                let (cw, ch) = (card_w.width() as f32, card_w.height() as f32);
                if p.x() >= 0.0 && p.y() >= 0.0 && p.x() <= cw && p.y() <= ch {
                    return;
                }
            }
            if let Some(f) = oc.borrow().as_ref() {
                f();
            }
        });
        root.add_controller(click);
        Help { root, on_close }
    }
}

pub struct Toolbar {
    pub revealer: gtk::Revealer,
    pub grid: gtk::Button,
    pub prev: gtk::Button,
    pub next: gtk::Button,
    pub scale: gtk::Button,
    pub rotate: gtk::Button,
    pub captions: gtk::Button,
    pub filters: gtk::Button,
    pub help: gtk::Button,
}

fn tool_button(icon: &str, tooltip: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.set_tooltip_text(Some(tooltip));
    b
}

impl Toolbar {
    pub fn new(menu: &gtk::gio::Menu) -> Toolbar {
        let pill = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        pill.add_css_class("mz-pill");
        let grid = tool_button("view-grid-symbolic", "Back to gallery (Esc, g)");
        let prev = tool_button("go-previous-symbolic", "Previous (←)");
        let next = tool_button("go-next-symbolic", "Next (→)");
        let scale = gtk::Button::with_label("Fit");
        scale.set_tooltip_text(Some("Scale mode (w)"));
        let rotate = tool_button("object-rotate-right-symbolic", "Rotate (r)");
        let captions = tool_button("view-reveal-symbolic", "Captions (t)");
        let filters = gtk::Button::with_label("Filters");
        filters.set_tooltip_text(Some("Filters (i)"));
        let help = gtk::Button::with_label("?");
        help.set_tooltip_text(Some("Help (h)"));
        let menub = gtk::MenuButton::new();
        menub.set_icon_name("view-more-symbolic");
        menub.set_menu_model(Some(menu));
        for w in [&grid, &prev, &next] {
            pill.append(w);
        }
        let sep = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sep.add_css_class("sep");
        pill.append(&sep);
        for w in [&scale, &rotate, &captions, &filters, &help] {
            pill.append(w);
        }
        pill.append(&menub);
        let revealer = gtk::Revealer::new();
        revealer.set_child(Some(&pill));
        revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
        revealer.set_transition_duration(150);
        revealer.set_halign(gtk::Align::Center);
        revealer.set_valign(gtk::Align::Start);
        revealer.set_margin_top(12);
        revealer.set_reveal_child(true);
        Toolbar {
            revealer,
            grid,
            prev,
            next,
            scale,
            rotate,
            captions,
            filters,
            help,
        }
    }
}

pub struct Captions {
    pub tl: gtk::Label,
    pub tr: gtk::Label,
    pub bl: gtk::Label,
}

impl Captions {
    pub fn new() -> Captions {
        let mk = |h: gtk::Align, v: gtk::Align| {
            let l = gtk::Label::new(None);
            l.add_css_class("mz-overlay");
            l.set_halign(h);
            l.set_valign(v);
            l.set_margin_start(12);
            l.set_margin_end(12);
            l.set_margin_top(12);
            l.set_margin_bottom(12);
            l.set_can_target(false);
            l.set_visible(false);
            l
        };
        Captions {
            tl: mk(gtk::Align::Start, gtk::Align::Start),
            tr: mk(gtk::Align::End, gtk::Align::Start),
            bl: mk(gtk::Align::Start, gtk::Align::End),
        }
    }

    pub fn set(&self, tl: Option<&str>, tr: Option<&str>, bl: Option<&str>) {
        for (l, t) in [(&self.tl, tl), (&self.tr, tr), (&self.bl, bl)] {
            match t {
                Some(s) if !s.is_empty() => {
                    l.set_text(s);
                    l.set_visible(true);
                }
                _ => l.set_visible(false),
            }
        }
    }
}

pub struct MetaPanel {
    pub revealer: gtk::Revealer,
    rows: gtk::Box,
    pub on_row: MetaRowCallback,
}

impl MetaPanel {
    pub fn new() -> MetaPanel {
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.add_css_class("mz-panel");
        outer.set_size_request(300, -1);
        let h = gtk::Label::new(Some("METADATA"));
        h.add_css_class("h");
        h.set_halign(gtk::Align::Start);
        h.set_margin_top(40);
        outer.append(&h);
        let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.append(&rows);
        let hint = gtk::Label::new(Some("click a row to copy · Folder opens the file manager"));
        hint.add_css_class("k");
        hint.set_halign(gtk::Align::Start);
        hint.set_margin_top(12);
        hint.set_wrap(true);
        outer.append(&hint);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_child(Some(&outer));
        let revealer = gtk::Revealer::new();
        revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
        revealer.set_transition_duration(150);
        revealer.set_child(Some(&scroller));
        revealer.set_reveal_child(false);
        MetaPanel {
            revealer,
            rows,
            on_row: Default::default(),
        }
    }

    pub fn set(&self, items: &[(String, String)]) {
        while let Some(c) = self.rows.first_child() {
            self.rows.remove(&c);
        }
        for (k, v) in items {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("row");
            row.set_focusable(true);
            let kl = gtk::Label::new(Some(k));
            kl.add_css_class("k");
            kl.set_xalign(0.0);
            kl.set_width_chars(9);
            kl.set_valign(gtk::Align::Start);
            let vl = gtk::Label::new(Some(v));
            vl.add_css_class("v");
            vl.set_xalign(0.0);
            vl.set_wrap(true);
            vl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            vl.set_hexpand(true);
            vl.set_selectable(false);
            row.append(&kl);
            row.append(&vl);
            let click = gtk::GestureClick::new();
            let (k2, v2, cb) = (k.clone(), v.clone(), self.on_row.clone());
            click.connect_released(move |_, _, _, _| {
                let callback = cb.borrow().clone();
                if let Some(f) = callback {
                    f(&k2, &v2);
                }
            });
            row.add_controller(click);
            let key = gtk::EventControllerKey::new();
            let (k2, v2, cb) = (k.clone(), v.clone(), self.on_row.clone());
            key.connect_key_pressed(move |_, key, _, _| {
                if matches!(
                    key,
                    gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter | gtk::gdk::Key::space
                ) {
                    let callback = cb.borrow().clone();
                    if let Some(f) = callback {
                        f(&k2, &v2);
                    }
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            });
            row.add_controller(key);
            self.rows.append(&row);
        }
    }

    pub fn focus_first(&self) -> bool {
        self.rows.first_child().is_some_and(|row| row.grab_focus())
    }

    pub fn contains(&self, widget: &gtk::Widget) -> bool {
        widget.is_ancestor(&self.revealer)
    }
}
