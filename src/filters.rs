//! Filter popover: format checkboxes with counts, presets, root-only toggle.

use crate::model::{ALL_GROUPS, Filter, Model, preset};
use crate::scan::GROUPS;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type FilterHandler = Option<Rc<dyn Fn(Filter)>>;
type FilterCallback = Rc<RefCell<FilterHandler>>;

fn emit_filter(filter: &RefCell<Filter>, on_change: &RefCell<FilterHandler>) {
    let value = *filter.borrow();
    let callback = on_change.borrow().clone();
    if let Some(callback) = callback {
        callback(value);
    }
}

pub struct FilterPopover {
    pub popover: gtk::Popover,
    checks: Vec<gtk::CheckButton>,
    counts: Vec<gtk::Label>,
    root_only: gtk::CheckButton,
    clear_folder: gtk::Button,
    updating: Rc<Cell<bool>>,
    filter: Rc<RefCell<Filter>>,
    pub on_change: FilterCallback,
}

impl FilterPopover {
    pub fn new() -> FilterPopover {
        let popover = gtk::Popover::new();
        popover.add_css_class("mz-filters");
        popover.set_has_arrow(false);
        popover.set_autohide(true);
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let updating = Rc::new(Cell::new(false));
        let filter = Rc::new(RefCell::new(Filter::default()));
        let on_change: FilterCallback = Default::default();
        let mut checks = Vec::new();
        let mut counts = Vec::new();

        let emit = {
            let filter = filter.clone();
            let on_change = on_change.clone();
            let updating = updating.clone();
            Rc::new(move || {
                if updating.get() {
                    return;
                }
                emit_filter(&filter, &on_change);
            })
        };

        for g in GROUPS {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let cb = gtk::CheckButton::with_label(g.label());
            cb.set_active(true);
            cb.set_hexpand(true);
            let n = gtk::Label::new(Some("0"));
            n.add_css_class("n");
            n.set_halign(gtk::Align::End);
            row.append(&cb);
            row.append(&n);
            vbox.append(&row);
            let (filter2, emit2) = (filter.clone(), emit.clone());
            cb.connect_toggled(move |c| {
                let mut f = filter2.borrow_mut();
                if c.is_active() {
                    f.groups |= g.bit();
                } else {
                    f.groups &= !g.bit();
                }
                drop(f);
                emit2();
            });
            checks.push(cb);
            counts.push(n);
        }
        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_margin_top(4);
        sep.set_margin_bottom(4);
        vbox.append(&sep);
        let root_only = gtk::CheckButton::with_label("Root folder only (no subfolders)");
        {
            let (filter2, emit2) = (filter.clone(), emit.clone());
            root_only.connect_toggled(move |c| {
                filter2.borrow_mut().root_only = c.is_active();
                emit2();
            });
        }
        vbox.append(&root_only);

        let clear_folder = gtk::Button::with_label("Show all folders");
        clear_folder.set_tooltip_text(Some("Remove the active folder filter"));
        clear_folder.set_visible(false);
        {
            let (filter2, emit2) = (filter.clone(), emit.clone());
            clear_folder.connect_clicked(move |_| {
                filter2.borrow_mut().folder_only = None;
                emit2();
            });
        }
        vbox.append(&clear_folder);

        let btns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        btns.set_margin_top(6);
        let mk = |label: &str, groups: u16, filter: Rc<RefCell<Filter>>, emit: Rc<dyn Fn()>| {
            let b = gtk::Button::with_label(label);
            b.set_hexpand(true);
            b.connect_clicked(move |_| {
                filter.borrow_mut().groups = groups;
                emit();
            });
            b
        };
        let only_jpeg = mk(
            "Only JPEG",
            preset(2).unwrap().1,
            filter.clone(),
            emit.clone(),
        );
        let no_webp = mk(
            "No WebP",
            preset(5).unwrap().1,
            filter.clone(),
            emit.clone(),
        );
        let all = mk("All", ALL_GROUPS, filter.clone(), emit.clone());
        let done = gtk::Button::with_label("Done");
        done.add_css_class("acc");
        done.set_hexpand(true);
        {
            let p = popover.clone();
            done.connect_clicked(move |_| p.popdown());
        }
        for b in [&only_jpeg, &no_webp, &all, &done] {
            btns.append(b);
        }
        vbox.append(&btns);
        popover.set_child(Some(&vbox));

        FilterPopover {
            popover,
            checks,
            counts,
            root_only,
            clear_folder,
            updating,
            filter,
            on_change,
        }
    }

    /// Sync checkboxes and counts from the model without emitting.
    pub fn update(&self, model: &Model) {
        self.updating.set(true);
        *self.filter.borrow_mut() = model.filter;
        for (i, g) in GROUPS.iter().enumerate() {
            self.checks[i].set_active(model.filter.groups & g.bit() != 0);
            self.counts[i].set_text(&group_digits(model.counts[i]));
        }
        self.root_only.set_active(model.filter.root_only);
        self.clear_folder
            .set_visible(model.filter.folder_only.is_some());
        self.updating.set(false);
    }
}

pub fn group_digits(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{2009}');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_callback_may_synchronously_update_filter_state() {
        let filter = Rc::new(RefCell::new(Filter::default()));
        let callbacks: FilterCallback = Default::default();
        let target = filter.clone();
        *callbacks.borrow_mut() = Some(Rc::new(move |_| {
            target.borrow_mut().groups = 0;
        }));

        emit_filter(&filter, &callbacks);

        assert_eq!(filter.borrow().groups, 0);
    }
}
