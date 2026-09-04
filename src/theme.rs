//! Omarchy theme sync: reads ~/.local/state/omarchy/current/theme/colors.toml,
//! generates CSS, installs one CssProvider and reloads it live.

use gtk::{gdk, gio, glib, prelude::*};
use std::cell::RefCell;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Colors {
    pub dark: bool,
    pub bg: gdk::RGBA,
    pub bg_dark: gdk::RGBA,
    pub bg_darker: gdk::RGBA,
    pub bg_light: gdk::RGBA,
    pub fg: gdk::RGBA,
    pub fg_dark: gdk::RGBA,
    pub fg_bright: gdk::RGBA,
    pub accent: gdk::RGBA,
    pub selection: gdk::RGBA,
    pub muted: gdk::RGBA,
    pub red: gdk::RGBA,
    pub green: gdk::RGBA,
    pub yellow: gdk::RGBA,
}

thread_local! {
    static CURRENT: RefCell<Colors> = RefCell::new(Colors::fallback());
}

pub fn colors() -> Colors {
    CURRENT.with(|c| c.borrow().clone())
}

fn rgba(hex: &str) -> Option<gdk::RGBA> {
    gdk::RGBA::parse(hex).ok()
}

pub fn hex(c: &gdk::RGBA) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8
    )
}

pub fn mix(a: &gdk::RGBA, b: &gdk::RGBA, t: f32) -> gdk::RGBA {
    gdk::RGBA::new(
        a.red() * (1.0 - t) + b.red() * t,
        a.green() * (1.0 - t) + b.green() * t,
        a.blue() * (1.0 - t) + b.blue() * t,
        1.0,
    )
}

pub fn alpha(c: &gdk::RGBA, a: f32) -> gdk::RGBA {
    gdk::RGBA::new(c.red(), c.green(), c.blue(), a)
}

fn rgba_css(c: &gdk::RGBA) -> String {
    format!(
        "rgba({},{},{},{:.3})",
        (c.red() * 255.0).round() as u8,
        (c.green() * 255.0).round() as u8,
        (c.blue() * 255.0).round() as u8,
        c.alpha()
    )
}

impl Colors {
    fn fallback() -> Colors {
        Colors {
            dark: true,
            bg: rgba("#1e1e2e").unwrap(),
            bg_dark: rgba("#161622").unwrap(),
            bg_darker: rgba("#101019").unwrap(),
            bg_light: rgba("#313244").unwrap(),
            fg: rgba("#cdd6f4").unwrap(),
            fg_dark: rgba("#6c7086").unwrap(),
            fg_bright: rgba("#cdd6f4").unwrap(),
            accent: rgba("#89b4fa").unwrap(),
            selection: rgba("#45475a").unwrap(),
            muted: rgba("#585b70").unwrap(),
            red: rgba("#f38ba8").unwrap(),
            green: rgba("#a6e3a1").unwrap(),
            yellow: rgba("#f9e2af").unwrap(),
        }
    }

    fn from_toml(text: &str) -> Colors {
        let mut c = Colors::fallback();
        let Ok(table) = text.parse::<toml::Table>() else {
            return c;
        };
        let get = |k: &str| table.get(k).and_then(|v| v.as_str()).and_then(rgba);
        if let Some(m) = table.get("mode").and_then(|v| v.as_str()) {
            c.dark = m != "light";
        }
        if let Some(v) = get("background") {
            c.bg = v;
        }
        if let Some(v) = get("dark_background") {
            c.bg_dark = v;
        }
        if let Some(v) = get("darker_background") {
            c.bg_darker = v;
        }
        if let Some(v) = get("lighter_background") {
            c.bg_light = v;
        }
        if let Some(v) = get("foreground") {
            c.fg = v;
        }
        if let Some(v) = get("dark_foreground") {
            c.fg_dark = v;
        }
        if let Some(v) = get("bright_foreground") {
            c.fg_bright = v;
        }
        if let Some(v) = get("accent") {
            c.accent = v;
        }
        if let Some(v) = get("selection") {
            c.selection = v;
        }
        if let Some(v) = get("muted") {
            c.muted = v;
        }
        if let Some(v) = get("red") {
            c.red = v;
        }
        if let Some(v) = get("green") {
            c.green = v;
        }
        if let Some(v) = get("yellow") {
            c.yellow = v;
        }
        c
    }

    pub fn css(&self) -> String {
        let c = self;
        let chip_on = mix(&c.bg_light, &c.accent, 0.3);
        let pill_bg = alpha(&c.bg, 0.92);
        let overlay_bg = alpha(&gdk::RGBA::BLACK, 0.55);
        let panel_bg = alpha(&c.bg, 0.96);
        format!(
            r#"
window.mz {{ background-color: {bg}; color: {fg}; }}
.mz-gallery-bg {{ background-color: {bg_dark}; }}
.mz-pill {{ background-color: {pill}; border: 1px solid {bg_light}; border-radius: 999px; padding: 4px 8px; box-shadow: 0 8px 24px rgba(0,0,0,0.35); }}
.mz-pill button {{ min-height: 24px; min-width: 24px; padding: 2px 10px; border-radius: 999px; background: transparent; border: 1px solid transparent; color: {fg_light}; box-shadow: none; }}
.mz-pill button:hover {{ background-color: {bg_light}; }}
.mz-pill button.on, .mz-pill button:checked {{ background-color: {chip_on}; border-color: {accent}; color: {fg_bright}; }}
.mz-pill .sep {{ min-width: 1px; background-color: {bg_light}; margin: 4px 4px; }}
.mz-status {{ background-color: {status_bg}; border-top: 1px solid {bg_light}; color: {fg_dark}; padding: 3px 10px; font-size: 12px; }}
.mz-status label.b, .mz-status button.b {{ color: {fg_light}; }}
.mz-status button {{ background: transparent; border: none; box-shadow: none; padding: 0 4px; min-height: 18px; color: {fg_dark}; font-size: 12px; }}
.mz-status button:hover {{ color: {fg_bright}; }}
.mz-overlay {{ font-family: monospace; font-size: 12px; background-color: {overlay}; color: {fg_bright}; padding: 5px 8px; border-radius: 6px; }}
.mz-viewer-bg {{ background-color: black; }}
.mz-edge {{ min-width: 42px; min-height: 72px; margin: 8px; border-radius: 999px; background-color: {overlay}; color: white; font-size: 30px; }}
.mz-edge:hover {{ background-color: {pill}; }}
.mz-panel {{ background-color: {panel}; border-left: 1px solid {bg_light}; padding: 14px; }}
.mz-panel .h {{ color: {fg_dark}; font-size: 11px; letter-spacing: 1px; margin-bottom: 8px; }}
.mz-panel .k {{ color: {fg_dark}; font-size: 12px; }}
.mz-panel .v {{ color: {fg_light}; font-size: 12px; }}
.mz-panel .row {{ border-bottom: 1px solid {bg_light}; padding: 3px 0; }}
.mz-help {{ background-color: {panel}; border: 1px solid {bg_light}; border-radius: 14px; padding: 18px 22px; box-shadow: 0 30px 60px rgba(0,0,0,0.5); }}
.mz-help .g {{ color: {accent}; font-size: 11px; letter-spacing: 1px; margin-top: 10px; }}
.mz-help .t {{ color: {fg_bright}; font-size: 15px; font-weight: bold; }}
.mz-help .a {{ color: {fg_light}; font-size: 12px; }}
.mz-help .k {{ color: {fg_bright}; font-family: monospace; font-size: 11px; background-color: {bg_light}; border-radius: 5px; padding: 1px 6px; margin: 1px; }}
.mz-help .m {{ color: {fg_dark}; font-size: 11px; }}
.mz-dim {{ background-color: rgba(0,0,0,0.45); }}
.mz-jump {{ background-color: {panel}; border-left: 1px solid {bg_light}; }}
.mz-jump row {{ padding: 3px 8px; border-radius: 6px; color: {fg_light}; font-size: 12px; }}
.mz-jump row:selected {{ background-color: {chip_on}; color: {fg_bright}; }}
.mz-jump .h {{ color: {fg_dark}; font-size: 11px; letter-spacing: 1px; padding: 8px 10px 4px; }}
.mz-jump .n {{ color: {fg_dark}; }}
popover.mz-filters contents {{ background-color: {bg}; border: 1px solid {bg_light}; border-radius: 12px; padding: 8px; }}
popover.mz-filters checkbutton {{ color: {fg_light}; padding: 3px 4px; }}
popover.mz-filters .n {{ color: {fg_dark}; font-family: monospace; font-size: 11px; }}
popover.mz-filters button {{ border-radius: 6px; }}
popover.mz-filters button.acc {{ background-color: {accent}; color: {bg}; font-weight: bold; }}
.mz-toast {{ font-size: 12px; }}
"#,
            bg = hex(&c.bg),
            bg_dark = hex(&c.bg_dark),
            bg_light = hex(&c.bg_light),
            fg = hex(&c.fg),
            fg_light = hex(&mix(&c.fg, &c.fg_bright, 0.5)),
            fg_bright = hex(&c.fg_bright),
            fg_dark = hex(&c.fg_dark),
            accent = hex(&c.accent),
            chip_on = hex(&chip_on),
            pill = rgba_css(&pill_bg),
            status_bg = rgba_css(&alpha(&c.bg_dark, 0.85)),
            overlay = rgba_css(&overlay_bg),
            panel = rgba_css(&panel_bg),
        )
    }
}

fn theme_dir() -> PathBuf {
    let home = glib::home_dir();
    home.join(".local/state/omarchy/current/theme")
}

fn override_css_path() -> PathBuf {
    theme_dir().join("mzera.css")
}

fn load_colors() -> Colors {
    match std::fs::read_to_string(theme_dir().join("colors.toml")) {
        Ok(t) => Colors::from_toml(&t),
        Err(_) => Colors::fallback(),
    }
}

pub struct Theme {
    provider: gtk::CssProvider,
    monitors: RefCell<Vec<gio::FileMonitor>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl Theme {
    pub fn install() -> std::rc::Rc<Theme> {
        let provider = gtk::CssProvider::new();
        let display = gdk::Display::default().expect("display");
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let theme = std::rc::Rc::new(Theme {
            provider,
            monitors: RefCell::new(Vec::new()),
            listeners: RefCell::new(Vec::new()),
        });
        theme.apply();

        // Watch both directories: Omarchy may replace the theme directory itself,
        // and an override CSS file may be created after mzera starts.
        let mut monitors = Vec::new();
        let directory = theme_dir();
        let parent = directory
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| directory.clone());
        for p in [directory, parent] {
            let f = gio::File::for_path(&p);
            if let Ok(m) =
                f.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
            {
                let t = std::rc::Rc::downgrade(&theme);
                m.connect_changed(move |_, _, _, _| {
                    if let Some(t) = t.upgrade() {
                        // debounce: several events per theme switch
                        let t2 = t.clone();
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(120),
                            move || t2.apply(),
                        );
                    }
                });
                monitors.push(m);
            }
        }
        *theme.monitors.borrow_mut() = monitors;
        theme
    }

    pub fn on_change(&self, f: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    fn apply(&self) {
        let colors = load_colors();
        let css = match std::fs::read_to_string(override_css_path()) {
            Ok(user) if !user.trim().is_empty() => user,
            _ => colors.css(),
        };
        self.provider.load_from_string(&css);
        let sm = adw::StyleManager::default();
        sm.set_color_scheme(if colors.dark {
            adw::ColorScheme::ForceDark
        } else {
            adw::ColorScheme::ForceLight
        });
        CURRENT.with(|c| *c.borrow_mut() = colors);
        for l in self.listeners.borrow().iter() {
            l();
        }
    }
}
