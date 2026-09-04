mod actions;
mod app;
mod exifinfo;
mod filters;
mod gallery;
mod model;
mod overlays;
mod scan;
mod settings;
mod theme;
mod thumbs;
mod video;
mod viewer;

fn main() -> gtk::glib::ExitCode {
    app::run()
}
