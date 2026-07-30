//! Loading, saving, and migrating settings from `config/settings.json`.
//!
//! On startup the configuration directory and an empty settings file are created
//! with defaults if they do not already exist. Loading is lenient: a corrupt or
//! partially-present file is merged over the defaults so the game never refuses
//! to boot because of a malformed config.

use std::path::PathBuf;

use serde_json::Value;

use crate::settings::Settings;

/// The directory containing `settings.json`, relative to the current working
/// directory.
pub fn config_dir() -> PathBuf {
    PathBuf::from("config")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

/// Load settings, creating a defaulted file if none exists. Any IO or parse
/// failures are swallowed into a defaulted `Settings`; we never crash the game
/// over configuration.
pub fn load_or_create() -> Settings {
    let path = settings_path();
    if !path.exists() {
        if let Err(e) = save_default() {
            // We still proceed with in-memory defaults; the next save attempt
            // will try again.
            eprintln!("warning: could not write default settings: {e}");
        }
        return Settings::default();
    }
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_lenient(&text),
        Err(e) => {
            eprintln!("warning: could not read settings ({e}); using defaults");
            Settings::default()
        }
    }
}

/// Write a fresh default settings.json, creating the config directory.
pub fn save_default() -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let s = Settings::default();
    let json = serde_json::to_string_pretty(&s).map_err(io_from_serde)?;
    std::fs::write(settings_path(), json)
}

/// Persist the current settings to disk.
pub fn save(settings: &Settings) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let json = serde_json::to_string_pretty(settings).map_err(io_from_serde)?;
    std::fs::write(settings_path(), json)
}

/// Parse a JSON config, deep-merging it on top of the defaults so missing
/// fields keep their default values.
fn parse_lenient(text: &str) -> Settings {
    let mut defaults = serde_json::to_value(Settings::default()).unwrap_or(Value::Null);
    match serde_json::from_str::<Value>(text) {
        Ok(user) => {
            merge_over(&mut defaults, user);
        }
        Err(e) => {
            eprintln!("warning: settings.json is not valid JSON ({e}); using defaults");
        }
    }
    serde_json::from_value(defaults).unwrap_or_else(|e| {
        eprintln!("warning: settings did not deserialize ({e}); using defaults");
        Settings::default()
    })
}

/// Recursively merge `rhs` into `lhs` object fields; non-object values in `rhs`
/// replace those in `lhs`.
fn merge_over(lhs: &mut Value, rhs: Value) {
    use Value::{Array, Bool, Null, Number, Object, String};
    match (lhs, rhs) {
        (Object(l), Object(r)) => {
            for (k, v) in r {
                if let Some(existing) = l.get_mut(&k) {
                    merge_over(existing, v);
                } else {
                    l.insert(k, v);
                }
            }
        }
        (lhs_slot, rhs_value) => {
            // Replace scalars/arrays wholesale.
            match rhs_value {
                Null | Bool(_) | Number(_) | String(_) | Array(_) => {
                    *lhs_slot = rhs_value;
                }
                Object(_) => {}
            }
        }
    }
}

fn io_from_serde(e: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}