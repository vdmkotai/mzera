//! Thin wrapper around gtk::Video for WebM playback.

use gtk::prelude::*;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

type ErrorCallback = Rc<dyn Fn(String)>;

pub struct VideoView {
    pub widget: gtk::Video,
    pub on_error: Rc<RefCell<Option<ErrorCallback>>>,
}

impl VideoView {
    pub fn new() -> VideoView {
        let widget = gtk::Video::new();
        widget.set_autoplay(true);
        widget.set_loop(true);
        widget.set_graphics_offload(gtk::GraphicsOffloadEnabled::Enabled);
        widget.set_hexpand(true);
        widget.set_vexpand(true);
        VideoView {
            widget,
            on_error: Rc::new(RefCell::new(None)),
        }
    }

    pub fn open(&self, path: &Path) {
        let mf = gtk::MediaFile::for_filename(path);
        let callback = self.on_error.clone();
        let display_path = path.display().to_string();
        mf.connect_error_notify(move |stream| {
            let Some(error) = stream.error() else { return };
            let callback = callback.borrow().clone();
            if let Some(callback) = callback {
                callback(format!("Could not play {display_path}: {error}"));
            }
        });
        mf.set_muted(true);
        mf.set_loop(true);
        self.widget.set_media_stream(Some(&mf));
        mf.play();
    }

    pub fn stop(&self) {
        if let Some(s) = self.widget.media_stream() {
            s.pause();
        }
        self.widget.set_media_stream(None::<&gtk::MediaStream>);
    }

    fn stream(&self) -> Option<gtk::MediaStream> {
        self.widget.media_stream()
    }

    pub fn toggle(&self) {
        if let Some(s) = self.stream() {
            if s.is_playing() { s.pause() } else { s.play() }
        }
    }

    pub fn seek_by(&self, secs: f64) {
        if let Some(s) = self.stream() {
            if !s.is_seekable() {
                return;
            }
            let dur = s.duration();
            let t = (s.timestamp() as f64 + secs * 1_000_000.0).max(0.0);
            let t = if dur > 0 { t.min(dur as f64) } else { t };
            s.seek(t as i64);
        }
    }

    pub fn seek_to_start(&self) {
        if let Some(s) = self.stream() {
            s.seek(0);
        }
    }

    pub fn seek_to_end(&self) {
        if let Some(s) = self.stream()
            && s.is_seekable()
            && s.duration() > 0
        {
            s.seek(s.duration());
        }
    }

    pub fn volume_by(&self, d: f64) {
        if let Some(s) = self.stream() {
            let v = (s.volume() + d).clamp(0.0, 1.0);
            s.set_muted(false);
            s.set_volume(v);
        }
    }

    pub fn toggle_mute(&self) {
        if let Some(s) = self.stream() {
            s.set_muted(!s.is_muted());
        }
    }

    pub fn toggle_loop(&self) {
        if let Some(s) = self.stream() {
            s.set_loop(!s.is_loop());
        }
    }
}
