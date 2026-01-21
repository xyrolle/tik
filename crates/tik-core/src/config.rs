use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::fs;
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: String,
    pub output_format: String,
    pub pager: String,
    pub timezone: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let raw = fs::read_to_string(path)?;
        serde_json::from_str(&raw)
            .map_err(|err| TikError::Config(format!("invalid config json: {err}")))
    }

    pub fn write(path: &Path, config: &Config) -> Result<()> {
        let raw = serde_json::to_string_pretty(config)
            .map_err(|err| TikError::Config(format!("serialize config: {err}")))?;
        fs::write_string_atomic(path, &raw)
    }

    pub fn load_legacy_toml(path: &Path) -> Result<Config> {
        let raw = fs::read_to_string(path)?;
        toml::from_str(&raw)
            .map_err(|err| TikError::Config(format!("invalid legacy config toml: {err}")))
    }

    pub fn get_value(&self, key: &str) -> Result<String> {
        let key = normalize_key(key)?;
        match key.as_str() {
            "schema_version" => Ok(self.schema_version.clone()),
            "output_format" => Ok(self.output_format.clone()),
            "pager" => Ok(self.pager.clone()),
            "timezone" => Ok(self.timezone.clone()),
            _ => Err(TikError::Config(format!("unknown config key: {key}"))),
        }
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        let key = normalize_key(key)?;
        if key == "schema_version" {
            return Err(TikError::Config("schema_version is read-only".to_string()));
        }

        let value = value.trim();
        if value.is_empty() {
            return Err(TikError::Config(format!("value required for {key}")));
        }

        match key.as_str() {
            "output_format" => {
                let value = value.to_lowercase();
                if !OUTPUT_FORMATS.contains(&value.as_str()) {
                    return Err(TikError::Config(format!("invalid output_format: {value}")));
                }
                self.output_format = value;
            }
            "pager" => {
                let value = value.to_lowercase();
                if !PAGER_MODES.contains(&value.as_str()) {
                    return Err(TikError::Config(format!("invalid pager: {value}")));
                }
                self.pager = value;
            }
            "timezone" => {
                let value = value.to_lowercase();
                if !TIMEZONES.contains(&value.as_str()) {
                    return Err(TikError::Config(format!("invalid timezone: {value}")));
                }
                self.timezone = if value == "utc" {
                    "UTC".to_string()
                } else {
                    value
                };
            }
            _ => {
                return Err(TikError::Config(format!("unknown config key: {key}")));
            }
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: "1.0".to_string(),
            output_format: "table".to_string(),
            pager: "auto".to_string(),
            timezone: "UTC".to_string(),
        }
    }
}

const OUTPUT_FORMATS: [&str; 7] = ["table", "compact", "json", "jsonl", "yaml", "md", "csv"];
const PAGER_MODES: [&str; 3] = ["auto", "always", "never"];
const TIMEZONES: [&str; 2] = ["utc", "local"];

fn normalize_key(key: &str) -> Result<String> {
    let key = key.trim();
    if key.is_empty() {
        return Err(TikError::Config("config key cannot be empty".to_string()));
    }
    Ok(key.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = Config::default();
        Config::write(&path, &config).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.schema_version, "1.0");
        assert_eq!(loaded.output_format, "table");
        assert_eq!(loaded.pager, "auto");
        assert_eq!(loaded.timezone, "UTC");
    }

    #[test]
    fn get_set_value() {
        let mut config = Config::default();
        assert_eq!(config.get_value("output_format").unwrap(), "table");
        config.set_value("output_format", "json").unwrap();
        assert_eq!(config.output_format, "json");
        config.set_value("pager", "never").unwrap();
        assert_eq!(config.pager, "never");
        config.set_value("timezone", "local").unwrap();
        assert_eq!(config.timezone, "local");
    }

    #[test]
    fn set_value_rejects_invalid() {
        let mut config = Config::default();
        let err = config.set_value("output_format", "bad").unwrap_err();
        assert!(matches!(err, TikError::Config(_)));
        let err = config.set_value("schema_version", "2.0").unwrap_err();
        assert!(matches!(err, TikError::Config(_)));
    }
}
