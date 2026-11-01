//! Saved settings, stored as one JSON file under XDG_CONFIG_HOME.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::color::{parse_color, to_hex};
use crate::error::{Error, Result};
use crate::protocol::{find_mode, DEFAULT_BRIGHTNESS, MODES};

pub const DEFAULT_MODE: &str = "rainbow";
const THEMES: [&str; 3] = ["system", "light", "dark"];
const LANGUAGES: [&str; 6] = ["auto", "en", "zh", "pt", "es", "fr"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeSettings {
    pub brightness: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(flatten)]
    pub params: BTreeMap<String, i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    pub mode: String,
    pub led: bool,
    pub bypass: bool,
    pub keep_running: bool,
    pub theme: String,
    pub language: String,
    pub modes: BTreeMap<String, ModeSettings>,
    pub profiles: BTreeMap<String, Profile>,
}

/// A named snapshot of one effect and its settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub mode: String,
    #[serde(flatten)]
    pub settings: ModeSettings,
}

const MAX_PROFILE_NAME: usize = 32;

impl Config {
    pub fn settings(&self) -> &ModeSettings {
        &self.modes[&self.mode]
    }

    pub fn settings_mut(&mut self) -> &mut ModeSettings {
        self.modes
            .get_mut(&self.mode)
            .expect("mode is always validated")
    }

    /// The profile that matches what is showing now, if any.
    pub fn active_profile(&self) -> Option<&str> {
        if !self.led || self.bypass {
            return None;
        }
        self.profiles
            .iter()
            .find(|(_, profile)| profile.mode == self.mode && profile.settings == *self.settings())
            .map(|(name, _)| name.as_str())
    }

    /// Selects a profile's effect and shows it: lighting on, and the card rather
    /// than the motherboard in control, or the profile would have no visible effect.
    pub fn apply_profile(&mut self, name: &str) -> Result<()> {
        let profile = self
            .profiles
            .get(name)
            .ok_or_else(|| Error::Invalid(format!("no profile named {name:?}")))?
            .clone();
        self.mode = profile.mode;
        *self.settings_mut() = profile.settings;
        self.led = true;
        self.bypass = false;
        Ok(())
    }

    /// Saves the current effect under `name`, replacing any profile called that.
    pub fn save_profile(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_PROFILE_NAME {
            return Err(Error::Invalid(format!(
                "a profile name must be 1-{MAX_PROFILE_NAME} characters"
            )));
        }
        let profile = Profile {
            mode: self.mode.clone(),
            settings: self.settings().clone(),
        };
        self.profiles.insert(name.to_string(), profile);
        Ok(())
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<()> {
        self.profiles
            .remove(name)
            .map(drop)
            .ok_or_else(|| Error::Invalid(format!("no profile named {name:?}")))
    }

    /// Back to default effects and settings, leaving app preferences and
    /// profiles alone.
    pub fn reset_lighting(&mut self) {
        let defaults = Config::default();
        self.modes = defaults.modes;
        self.mode = defaults.mode;
        self.led = true;
        self.bypass = false;
    }
}

pub fn default_mode_settings(key: &str) -> ModeSettings {
    let mode = find_mode(key).expect("known mode");
    ModeSettings {
        brightness: DEFAULT_BRIGHTNESS,
        direction: mode.has_direction.then_some(0),
        color: mode.color_regs.map(|_| "#FFFFFF".to_string()),
        params: mode
            .params
            .iter()
            .map(|p| (p.key.to_string(), p.default))
            .collect(),
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: DEFAULT_MODE.to_string(),
            led: true,
            bypass: false,
            keep_running: false,
            theme: "system".to_string(),
            language: "auto".to_string(),
            modes: MODES
                .iter()
                .map(|mode| (mode.key.to_string(), default_mode_settings(mode.key)))
                .collect(),
            profiles: starter_profiles(),
        }
    }
}

/// Offered on first run. They are ordinary profiles, so they can be replaced or
/// deleted, and deleted ones stay deleted.
fn starter_profiles() -> BTreeMap<String, Profile> {
    let mut gaming = default_mode_settings("rainbow");
    gaming.brightness = 255;
    // Response, not Speed, sets how fast Rainbow moves.
    gaming.params.insert("response".to_string(), 8);
    let mut night = default_mode_settings("custom");
    night.brightness = 24;
    night.color = Some("#FF6A20".to_string());
    BTreeMap::from([
        (
            "Gaming".to_string(),
            Profile {
                mode: "rainbow".to_string(),
                settings: gaming,
            },
        ),
        (
            "Night".to_string(),
            Profile {
                mode: "custom".to_string(),
                settings: night,
            },
        ),
    ])
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("vagalume-rgb").join("config.json")
}

fn overlay_bool(target: &mut bool, saved: &Map<String, Value>, key: &str) {
    if let Some(value) = saved.get(key).and_then(Value::as_bool) {
        *target = value;
    }
}

fn overlay_choice(target: &mut String, saved: &Map<String, Value>, key: &str, allowed: &[&str]) {
    if let Some(value) = saved.get(key).and_then(Value::as_str) {
        if allowed.contains(&value) {
            *target = value.to_string();
        }
    }
}

fn overlay_mode(settings: &mut ModeSettings, key: &str, stored: &Map<String, Value>) {
    let mode = find_mode(key).expect("known mode");
    if let Some(value) = stored.get("brightness").and_then(Value::as_u64) {
        if let Ok(value) = u8::try_from(value) {
            settings.brightness = value;
        }
    }
    if mode.has_direction {
        if let Some(value @ (0 | 1)) = stored.get("direction").and_then(Value::as_u64) {
            settings.direction = Some(value as u8);
        }
    }
    if mode.color_regs.is_some() {
        if let Some(Ok(color)) = stored.get("color").and_then(Value::as_str).map(parse_color) {
            settings.color = Some(to_hex(color));
        }
    }
    for param in mode.params {
        if let Some(value) = stored.get(param.key).and_then(Value::as_i64) {
            if (param.min as i64..=param.max as i64).contains(&value) {
                settings.params.insert(param.key.to_string(), value as i32);
            }
        }
    }
}

fn load_profile((name, stored): (&String, &Value)) -> Option<(String, Profile)> {
    let stored = stored.as_object()?;
    let key = stored.get("mode")?.as_str()?;
    find_mode(key)?;
    let mut settings = default_mode_settings(key);
    overlay_mode(&mut settings, key, stored);
    Some((
        name.clone(),
        Profile {
            mode: key.to_string(),
            settings,
        },
    ))
}

/// Loads settings. Anything missing or malformed falls back to its default, so
/// a damaged file can never produce an out-of-range value.
pub fn load_config(path: &Path) -> Config {
    let mut config = Config::default();
    let saved: Option<Value> = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    let Some(Value::Object(saved)) = saved else {
        return config;
    };
    overlay_bool(&mut config.led, &saved, "led");
    overlay_bool(&mut config.bypass, &saved, "bypass");
    overlay_bool(&mut config.keep_running, &saved, "keep_running");
    overlay_choice(&mut config.theme, &saved, "theme", &THEMES);
    overlay_choice(&mut config.language, &saved, "language", &LANGUAGES);
    if let Some(key) = saved.get("mode").and_then(Value::as_str) {
        if find_mode(key).is_some() {
            config.mode = key.to_string();
        }
    }
    if let Some(Value::Object(modes)) = saved.get("modes") {
        for (key, settings) in &mut config.modes {
            if let Some(Value::Object(stored)) = modes.get(key) {
                overlay_mode(settings, key, stored);
            }
        }
    }
    if let Some(Value::Object(profiles)) = saved.get("profiles") {
        config.profiles = profiles.iter().filter_map(load_profile).collect();
    }
    config
}

pub fn save_config(config: &Config, path: &Path) -> Result<()> {
    let failed = |e: std::io::Error| Error::Device(format!("Cannot save {}: {e}", path.display()));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    let mut text = serde_json::to_string_pretty(config).expect("config always serializes");
    text.push('\n');
    let temp = path.with_extension("tmp");
    fs::write(&temp, text).map_err(failed)?;
    fs::rename(&temp, path).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.json");
        (dir, path)
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn round_trip() {
        let (_dir, path) = path();
        let mut config = Config {
            mode: "breathing".into(),
            ..Config::default()
        };
        config.settings_mut().color = Some("#123456".into());
        save_config(&config, &path).unwrap();
        assert_eq!(load_config(&path), config);
    }

    #[test]
    fn bad_values_fall_back_to_defaults() {
        let (_dir, path) = path();
        write(
            &path,
            r#"{"mode":"nope","led":"yes","theme":"pink",
                "modes":{"custom":{"color":5},"breathing":{"color":"zzz","tempo":99},
                         "chase":{"brightness":999,"direction":7}}}"#,
        );
        assert_eq!(load_config(&path), Config::default());
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let (_dir, path) = path();
        write(&path, "{nope");
        assert_eq!(load_config(&path), Config::default());
    }

    #[test]
    fn deleted_starter_profiles_stay_deleted() {
        let (_dir, path) = path();
        let mut config = Config::default();
        config.delete_profile("Gaming").unwrap();
        config.delete_profile("Night").unwrap();
        save_config(&config, &path).unwrap();
        assert!(load_config(&path).profiles.is_empty());
    }

    #[test]
    fn config_from_before_profiles_gets_the_starters() {
        let (_dir, path) = path();
        write(&path, r#"{"mode":"custom"}"#);
        assert_eq!(load_config(&path).profiles.len(), 2);
    }

    #[test]
    fn bad_profiles_are_dropped_and_bad_values_defaulted() {
        let (_dir, path) = path();
        write(
            &path,
            r#"{"profiles":{"a":{"mode":"nope"},"b":5,"c":{"mode":"stacking","speed":99}}}"#,
        );
        let profiles = load_config(&path).profiles;
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles["c"].settings, default_mode_settings("stacking"));
    }

    #[test]
    fn saved_profile_applies_after_other_changes() {
        let mut config = Config {
            mode: "custom".into(),
            ..Config::default()
        };
        config.settings_mut().color = Some("#123456".into());
        config.save_profile(" Mine ").unwrap();
        assert_eq!(config.active_profile(), Some("Mine"));

        config.settings_mut().color = Some("#000000".into());
        config.mode = "chase".into();
        assert_eq!(config.active_profile(), None);

        config.led = false;
        config.bypass = true;
        config.apply_profile("Mine").unwrap();
        assert_eq!(config.mode, "custom");
        assert_eq!(config.settings().color.as_deref(), Some("#123456"));
        assert!(config.led && !config.bypass);
        assert_eq!(config.active_profile(), Some("Mine"));
    }

    #[test]
    fn profile_is_not_active_while_it_cannot_be_seen() {
        let mut config = Config::default();
        config.apply_profile("Night").unwrap();
        config.led = false;
        assert_eq!(config.active_profile(), None);
        config.led = true;
        config.bypass = true;
        assert_eq!(config.active_profile(), None);
    }

    #[test]
    fn rejects_blank_and_long_profile_names() {
        let mut config = Config::default();
        assert!(config.save_profile("   ").is_err());
        assert!(config.save_profile(&"x".repeat(33)).is_err());
        assert!(config.save_profile(&"é".repeat(32)).is_ok());
    }

    #[test]
    fn unknown_profiles_are_errors() {
        let mut config = Config::default();
        assert!(config.apply_profile("Nope").is_err());
        assert!(config.delete_profile("Nope").is_err());
    }

    #[test]
    fn reset_keeps_profiles() {
        let mut config = Config::default();
        config.save_profile("Mine").unwrap();
        config.reset_lighting();
        assert!(config.profiles.contains_key("Mine"));
    }

    #[test]
    fn colourless_modes_stay_colourless() {
        let (_dir, path) = path();
        write(&path, r##"{"modes":{"chase":{"color":"#FF0000"}}}"##);
        assert_eq!(load_config(&path).modes["chase"].color, None);
    }
}
