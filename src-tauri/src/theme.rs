//! UI theme persistence: selected preset + user CSS in
//! `~/.config/sc-rust-client/theme.json`.

use crate::config;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const FILE: &str = "theme.json";
/// Upper bound for the user stylesheet (it lives in a JSON file and the DOM).
const MAX_CSS_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeConfig {
    #[serde(default = "default_preset")]
    pub preset: String,
    #[serde(default)]
    pub custom_css: String,
}

fn default_preset() -> String {
    "soundcloud-dark".into()
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig { preset: default_preset(), custom_css: String::new() }
    }
}

pub fn load() -> ThemeConfig {
    config::load(FILE)
}

pub fn validate(cfg: &ThemeConfig) -> Result<()> {
    let id_ok = !cfg.preset.is_empty()
        && cfg.preset.len() <= 64
        && cfg.preset.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !id_ok {
        bail!("некорректный идентификатор темы");
    }
    if cfg.custom_css.len() > MAX_CSS_BYTES {
        bail!("кастомный CSS слишком большой (максимум {} КБ)", MAX_CSS_BYTES / 1024);
    }
    Ok(())
}

pub fn save(cfg: &ThemeConfig) -> Result<()> {
    validate(cfg)?;
    config::save(FILE, cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let c: ThemeConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c, ThemeConfig::default());
        let c: ThemeConfig = serde_json::from_str(r#"{"preset":"hyprland"}"#).unwrap();
        assert_eq!(c.preset, "hyprland");
        assert!(c.custom_css.is_empty());
    }

    #[test]
    fn rejects_bad_preset_ids_and_huge_css() {
        let mut c = ThemeConfig { preset: "../etc".into(), custom_css: String::new() };
        assert!(validate(&c).is_err());
        c.preset = "cyberpunk".into();
        assert!(validate(&c).is_ok());
        c.custom_css = "a".repeat(MAX_CSS_BYTES + 1);
        assert!(validate(&c).is_err());
    }
}
