//! Small, version-tolerant user preferences file.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub grouped: bool,
    pub tile_step: usize,
    pub scale_mode: String,
    pub caption_mode: String,
    pub slideshow_seconds: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            grouped: false,
            tile_step: crate::gallery::DEFAULT_TILE_STEP,
            scale_mode: "fit".into(),
            caption_mode: "brief".into(),
            slideshow_seconds: 4,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let Ok(text) = std::fs::read_to_string(path()) else {
            return Self::default();
        };
        let mut settings: Self = toml::from_str(&text).unwrap_or_default();
        settings.tile_step = settings.tile_step.min(crate::gallery::TILE_STEPS.len() - 1);
        settings.slideshow_seconds = settings.slideshow_seconds.clamp(1, 3_600);
        settings
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        let Some(directory) = path.parent() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "settings path has no parent",
            ));
        };
        std::fs::create_dir_all(directory)?;
        let mut temporary = tempfile::Builder::new()
            .prefix(".config-")
            .tempfile_in(directory)?;
        temporary.write_all(
            toml::to_string_pretty(self)
                .map_err(std::io::Error::other)?
                .as_bytes(),
        )?;
        temporary.as_file_mut().sync_all()?;
        temporary.persist(path).map(|_| ()).map_err(|e| e.error)
    }
}

fn path() -> PathBuf {
    gtk::glib::user_config_dir()
        .join("mzera")
        .join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let settings = Settings::default();
        assert!(settings.tile_step < crate::gallery::TILE_STEPS.len());
        assert!((1..=3_600).contains(&settings.slideshow_seconds));
    }

    #[test]
    fn missing_fields_keep_defaults() {
        let settings: Settings = toml::from_str("grouped = true").expect("partial settings");
        assert!(settings.grouped);
        assert_eq!(settings.tile_step, crate::gallery::DEFAULT_TILE_STEP);
        assert_eq!(settings.scale_mode, "fit");
    }
}
