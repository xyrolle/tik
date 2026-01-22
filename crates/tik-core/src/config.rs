use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::fs;
use crate::domain::ticket::{
    normalize_assignee, normalize_tag, Priority, Severity, TicketStatus, TicketType,
};
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub schema_version: String,
    pub output_format: String,
    pub pager: String,
    pub timezone: String,
    #[serde(default = "default_ticket_default_type")]
    pub ticket_default_type: String,
    #[serde(default = "default_ticket_default_priority")]
    pub ticket_default_priority: String,
    #[serde(default = "default_ticket_default_severity")]
    pub ticket_default_severity: String,
    #[serde(default = "default_ticket_types")]
    pub ticket_types: Vec<String>,
    #[serde(default = "default_ticket_priorities")]
    pub ticket_priorities: Vec<String>,
    #[serde(default = "default_ticket_severities")]
    pub ticket_severities: Vec<String>,
    #[serde(default = "default_ticket_statuses")]
    pub ticket_statuses: Vec<String>,
    #[serde(default)]
    pub ticket_tags: Vec<String>,
    #[serde(default)]
    pub ticket_assignees: Vec<String>,
    #[serde(default = "default_ticket_estimate_units")]
    pub ticket_estimate_units: Vec<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let raw = fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&raw)
            .map_err(|err| TikError::Config(format!("invalid config json: {err}")))
            ?;
        config.normalize()?;
        Ok(config)
    }

    pub fn write(path: &Path, config: &Config) -> Result<()> {
        let raw = serde_json::to_string_pretty(config)
            .map_err(|err| TikError::Config(format!("serialize config: {err}")))?;
        fs::write_string_atomic(path, &raw)
    }

    pub fn load_legacy_toml(path: &Path) -> Result<Config> {
        let raw = fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&raw)
            .map_err(|err| TikError::Config(format!("invalid legacy config toml: {err}")))?;
        config.normalize()?;
        Ok(config)
    }

    pub fn get_value(&self, key: &str) -> Result<String> {
        let key = normalize_key(key)?;
        match key.as_str() {
            "schema_version" => Ok(self.schema_version.clone()),
            "output_format" => Ok(self.output_format.clone()),
            "pager" => Ok(self.pager.clone()),
            "timezone" => Ok(self.timezone.clone()),
            "ticket_default_type" => Ok(self.ticket_default_type.clone()),
            "ticket_default_priority" => Ok(self.ticket_default_priority.clone()),
            "ticket_default_severity" => Ok(self.ticket_default_severity.clone()),
            "ticket_types" => Ok(join_list(&self.ticket_types)),
            "ticket_priorities" => Ok(join_list(&self.ticket_priorities)),
            "ticket_severities" => Ok(join_list(&self.ticket_severities)),
            "ticket_statuses" => Ok(join_list(&self.ticket_statuses)),
            "ticket_tags" => Ok(join_list(&self.ticket_tags)),
            "ticket_assignees" => Ok(join_list(&self.ticket_assignees)),
            "ticket_estimate_units" => Ok(join_list(&self.ticket_estimate_units)),
            _ => Err(TikError::Config(format!("unknown config key: {key}"))),
        }
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<()> {
        let key = normalize_key(key)?;
        if key == "schema_version" {
            return Err(TikError::Config("schema_version is read-only".to_string()));
        }

        let value = value.trim();

        match key.as_str() {
            "output_format" => {
                if value.is_empty() {
                    return Err(TikError::Config(format!("value required for {key}")));
                }
                let value = value.to_lowercase();
                if !OUTPUT_FORMATS.contains(&value.as_str()) {
                    return Err(TikError::Config(format!("invalid output_format: {value}")));
                }
                self.output_format = value;
            }
            "pager" => {
                if value.is_empty() {
                    return Err(TikError::Config(format!("value required for {key}")));
                }
                let value = value.to_lowercase();
                if !PAGER_MODES.contains(&value.as_str()) {
                    return Err(TikError::Config(format!("invalid pager: {value}")));
                }
                self.pager = value;
            }
            "timezone" => {
                if value.is_empty() {
                    return Err(TikError::Config(format!("value required for {key}")));
                }
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
            "ticket_default_type" => {
                self.ticket_default_type =
                    normalize_enum_value(value, &TICKET_TYPES, "ticket_default_type")?;
            }
            "ticket_default_priority" => {
                self.ticket_default_priority =
                    normalize_enum_value(value, &TICKET_PRIORITIES, "ticket_default_priority")?;
            }
            "ticket_default_severity" => {
                self.ticket_default_severity =
                    normalize_enum_value(value, &TICKET_SEVERITIES, "ticket_default_severity")?;
            }
            "ticket_types" => {
                let values = parse_csv_list(value);
                self.ticket_types = normalize_enum_list(values, &TICKET_TYPES, "ticket_types")?;
            }
            "ticket_priorities" => {
                let values = parse_csv_list(value);
                self.ticket_priorities =
                    normalize_enum_list(values, &TICKET_PRIORITIES, "ticket_priorities")?;
            }
            "ticket_severities" => {
                let values = parse_csv_list(value);
                self.ticket_severities =
                    normalize_enum_list(values, &TICKET_SEVERITIES, "ticket_severities")?;
            }
            "ticket_statuses" => {
                let values = parse_csv_list(value);
                self.ticket_statuses =
                    normalize_enum_list(values, &TICKET_STATUSES, "ticket_statuses")?;
            }
            "ticket_tags" => {
                let values = parse_csv_list(value);
                self.ticket_tags = normalize_freeform_list(values, normalize_tag, "ticket_tags")?;
            }
            "ticket_assignees" => {
                let values = parse_csv_list(value);
                self.ticket_assignees =
                    normalize_freeform_list(values, normalize_assignee, "ticket_assignees")?;
            }
            "ticket_estimate_units" => {
                let values = parse_csv_list(value);
                self.ticket_estimate_units =
                    normalize_enum_list(values, &ESTIMATE_UNITS, "ticket_estimate_units")?;
            }
            _ => {
                return Err(TikError::Config(format!("unknown config key: {key}")));
            }
        }
        self.normalize()?;
        Ok(())
    }

    pub fn normalize(&mut self) -> Result<()> {
        self.ticket_default_type =
            normalize_enum_value(&self.ticket_default_type, &TICKET_TYPES, "ticket_default_type")?;
        self.ticket_default_priority = normalize_enum_value(
            &self.ticket_default_priority,
            &TICKET_PRIORITIES,
            "ticket_default_priority",
        )?;
        self.ticket_default_severity = normalize_enum_value(
            &self.ticket_default_severity,
            &TICKET_SEVERITIES,
            "ticket_default_severity",
        )?;

        self.ticket_types =
            normalize_enum_list(self.ticket_types.clone(), &TICKET_TYPES, "ticket_types")?;
        self.ticket_priorities = normalize_enum_list(
            self.ticket_priorities.clone(),
            &TICKET_PRIORITIES,
            "ticket_priorities",
        )?;
        self.ticket_severities = normalize_enum_list(
            self.ticket_severities.clone(),
            &TICKET_SEVERITIES,
            "ticket_severities",
        )?;
        self.ticket_statuses = normalize_enum_list(
            self.ticket_statuses.clone(),
            &TICKET_STATUSES,
            "ticket_statuses",
        )?;
        self.ticket_estimate_units = normalize_enum_list(
            self.ticket_estimate_units.clone(),
            &ESTIMATE_UNITS,
            "ticket_estimate_units",
        )?;

        self.ticket_tags =
            normalize_freeform_list(self.ticket_tags.clone(), normalize_tag, "ticket_tags")?;
        self.ticket_assignees = normalize_freeform_list(
            self.ticket_assignees.clone(),
            normalize_assignee,
            "ticket_assignees",
        )?;

        ensure_default_allowed(
            &self.ticket_default_type,
            &self.ticket_types,
            "ticket_default_type",
            "ticket_types",
        )?;
        ensure_default_allowed(
            &self.ticket_default_priority,
            &self.ticket_priorities,
            "ticket_default_priority",
            "ticket_priorities",
        )?;
        ensure_default_allowed(
            &self.ticket_default_severity,
            &self.ticket_severities,
            "ticket_default_severity",
            "ticket_severities",
        )?;
        Ok(())
    }

    pub fn validate_ticket(&self, ticket: &crate::domain::ticket::Ticket) -> Result<()> {
        validate_allowed_enum(
            ticket.kind.as_str(),
            &self.ticket_types,
            "ticket type",
        )?;
        validate_allowed_enum(
            ticket.priority.as_str(),
            &self.ticket_priorities,
            "ticket priority",
        )?;
        validate_allowed_enum(
            ticket.severity.as_str(),
            &self.ticket_severities,
            "ticket severity",
        )?;
        validate_allowed_enum(
            ticket.status.as_str(),
            &self.ticket_statuses,
            "ticket status",
        )?;

        let tags: Vec<String> = ticket.tags.iter().map(|tag| normalize_tag(tag)).collect();
        validate_allowed_list(&tags, &self.ticket_tags, "ticket tag")?;

        let assignees: Vec<String> = ticket
            .assignees
            .iter()
            .map(|assignee| normalize_assignee(assignee))
            .collect();
        validate_allowed_list(&assignees, &self.ticket_assignees, "ticket assignee")?;

        if let Some(estimate) = &ticket.estimate {
            validate_allowed_enum(
                estimate.unit.as_str(),
                &self.ticket_estimate_units,
                "estimate unit",
            )?;
        }

        Ok(())
    }

    pub fn normalize_and_validate_tags(&self, tags: Vec<String>) -> Result<Vec<String>> {
        let tags = normalize_input_list(tags, normalize_tag, "ticket_tags")?;
        validate_allowed_list_usage(&tags, &self.ticket_tags, "ticket tag")?;
        Ok(tags)
    }

    pub fn normalize_and_validate_assignees(&self, assignees: Vec<String>) -> Result<Vec<String>> {
        let assignees = normalize_input_list(assignees, normalize_assignee, "ticket_assignees")?;
        validate_allowed_list_usage(&assignees, &self.ticket_assignees, "ticket assignee")?;
        Ok(assignees)
    }

    pub fn normalize_tags(&self, tags: Vec<String>) -> Result<Vec<String>> {
        normalize_input_list(tags, normalize_tag, "ticket_tags")
    }

    pub fn normalize_assignees(&self, assignees: Vec<String>) -> Result<Vec<String>> {
        normalize_input_list(assignees, normalize_assignee, "ticket_assignees")
    }

    pub fn validate_ticket_status(&self, status: &TicketStatus) -> Result<()> {
        validate_allowed_enum(status.as_str(), &self.ticket_statuses, "ticket status")
    }

    pub(crate) fn ticket_defaults(&self) -> Result<crate::domain::ticket::TicketDefaults> {
        Ok(crate::domain::ticket::TicketDefaults {
            kind: TicketType::parse(&self.ticket_default_type)?,
            priority: Priority::parse(&self.ticket_default_priority)?,
            severity: Severity::parse(&self.ticket_default_severity)?,
        })
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: "1.1".to_string(),
            output_format: "table".to_string(),
            pager: "auto".to_string(),
            timezone: "UTC".to_string(),
            ticket_default_type: default_ticket_default_type(),
            ticket_default_priority: default_ticket_default_priority(),
            ticket_default_severity: default_ticket_default_severity(),
            ticket_types: default_ticket_types(),
            ticket_priorities: default_ticket_priorities(),
            ticket_severities: default_ticket_severities(),
            ticket_statuses: default_ticket_statuses(),
            ticket_tags: Vec::new(),
            ticket_assignees: Vec::new(),
            ticket_estimate_units: default_ticket_estimate_units(),
        }
    }
}

const OUTPUT_FORMATS: [&str; 7] = ["table", "compact", "json", "jsonl", "yaml", "md", "csv"];
const PAGER_MODES: [&str; 3] = ["auto", "always", "never"];
const TIMEZONES: [&str; 2] = ["utc", "local"];
const TICKET_TYPES: [&str; 5] = ["feature", "bug", "chore", "task", "spike"];
const TICKET_PRIORITIES: [&str; 4] = ["low", "medium", "high", "critical"];
const TICKET_SEVERITIES: [&str; 4] = ["low", "normal", "high", "critical"];
const TICKET_STATUSES: [&str; 5] = ["open", "in_progress", "blocked", "closed", "archived"];
const ESTIMATE_UNITS: [&str; 5] = ["hours", "days", "weeks", "points", "story_points"];

fn normalize_key(key: &str) -> Result<String> {
    let key = key.trim();
    if key.is_empty() {
        return Err(TikError::Config("config key cannot be empty".to_string()));
    }
    Ok(key.to_lowercase())
}

fn normalize_enum_value(value: &str, allowed: &[&str], label: &str) -> Result<String> {
    let normalized = value.trim().to_lowercase();
    if normalized.is_empty() {
        return Err(TikError::Config(format!("{label} cannot be empty")));
    }
    if !allowed.contains(&normalized.as_str()) {
        return Err(TikError::Config(format!("invalid {label}: {value}")));
    }
    Ok(normalized)
}

fn normalize_enum_list(values: Vec<String>, allowed: &[&str], label: &str) -> Result<Vec<String>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let normalized = normalize_enum_value(&value, allowed, label)?;
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    Ok(out)
}

fn normalize_freeform_list(
    values: Vec<String>,
    normalizer: fn(&str) -> String,
    label: &str,
) -> Result<Vec<String>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let normalized = normalizer(&value);
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(normalized);
        }
    }
    if out.is_empty() {
        return Err(TikError::Config(format!("{label} list cannot be empty")));
    }
    Ok(out)
}

fn validate_allowed_enum(value: &str, allowed: &[String], label: &str) -> Result<()> {
    if allowed.is_empty() {
        return Ok(());
    }
    if !allowed.contains(&value.to_lowercase()) {
        return Err(TikError::Config(format!("invalid {label}: {value}")));
    }
    Ok(())
}

fn validate_allowed_list(values: &[String], allowed: &[String], label: &str) -> Result<()> {
    if allowed.is_empty() {
        return Ok(());
    }
    let allowed_set: HashSet<&str> = allowed.iter().map(|value| value.as_str()).collect();
    for value in values {
        if !allowed_set.contains(value.as_str()) {
            return Err(TikError::Config(format!("invalid {label}: {value}")));
        }
    }
    Ok(())
}

fn validate_allowed_list_usage(
    values: &[String],
    allowed: &[String],
    label: &str,
) -> Result<()> {
    match validate_allowed_list(values, allowed, label) {
        Ok(()) => Ok(()),
        Err(TikError::Config(msg)) => Err(TikError::usage(&msg)),
        Err(err) => Err(err),
    }
}

fn ensure_default_allowed(
    default_value: &str,
    allowed: &[String],
    default_label: &str,
    allowed_label: &str,
) -> Result<()> {
    if allowed.is_empty() {
        return Ok(());
    }
    if !allowed.contains(&default_value.to_string()) {
        return Err(TikError::Config(format!(
            "{default_label} must be listed in {allowed_label}"
        )));
    }
    Ok(())
}

fn parse_csv_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

fn normalize_input_list(
    values: Vec<String>,
    normalizer: fn(&str) -> String,
    label: &str,
) -> Result<Vec<String>> {
    match normalize_freeform_list(values, normalizer, label) {
        Ok(values) => Ok(values),
        Err(TikError::Config(msg)) => Err(TikError::usage(&msg)),
        Err(err) => Err(err),
    }
}

fn join_list(values: &[String]) -> String {
    values.join(",")
}

fn default_ticket_default_type() -> String {
    "task".to_string()
}

fn default_ticket_default_priority() -> String {
    "medium".to_string()
}

fn default_ticket_default_severity() -> String {
    "normal".to_string()
}

fn default_ticket_types() -> Vec<String> {
    TICKET_TYPES.iter().map(|value| (*value).to_string()).collect()
}

fn default_ticket_priorities() -> Vec<String> {
    TICKET_PRIORITIES
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn default_ticket_severities() -> Vec<String> {
    TICKET_SEVERITIES
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn default_ticket_statuses() -> Vec<String> {
    TICKET_STATUSES
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn default_ticket_estimate_units() -> Vec<String> {
    ESTIMATE_UNITS
        .iter()
        .map(|value| (*value).to_string())
        .collect()
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
        assert_eq!(loaded.schema_version, "1.1");
        assert_eq!(loaded.output_format, "table");
        assert_eq!(loaded.pager, "auto");
        assert_eq!(loaded.timezone, "UTC");
        assert_eq!(loaded.ticket_default_type, "task");
        assert_eq!(loaded.ticket_default_priority, "medium");
        assert_eq!(loaded.ticket_default_severity, "normal");
        assert!(loaded.ticket_types.contains(&"feature".to_string()));
        assert!(loaded.ticket_priorities.contains(&"medium".to_string()));
        assert!(loaded.ticket_severities.contains(&"normal".to_string()));
        assert!(loaded.ticket_statuses.contains(&"open".to_string()));
        assert!(loaded.ticket_estimate_units.contains(&"points".to_string()));
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
        config
            .set_value("ticket_default_type", "feature")
            .unwrap();
        assert_eq!(config.ticket_default_type, "feature");
        config
            .set_value("ticket_types", "feature,bug")
            .unwrap();
        assert_eq!(
            config.ticket_types,
            vec!["feature".to_string(), "bug".to_string()]
        );
    }

    #[test]
    fn set_value_rejects_invalid() {
        let mut config = Config::default();
        let err = config.set_value("output_format", "bad").unwrap_err();
        assert!(matches!(err, TikError::Config(_)));
        let err = config.set_value("schema_version", "2.0").unwrap_err();
        assert!(matches!(err, TikError::Config(_)));
        let err = config.set_value("ticket_default_type", "oops").unwrap_err();
        assert!(matches!(err, TikError::Config(_)));
        let err = config.set_value("ticket_types", "oops").unwrap_err();
        assert!(matches!(err, TikError::Config(_)));
    }

    #[test]
    fn normalize_and_validate_tags_enforces_allowed() {
        let mut config = Config::default();
        config.ticket_tags = vec!["mvp".to_string(), "cli".to_string()];
        config.normalize().unwrap();

        let tags = config
            .normalize_and_validate_tags(vec!["mvp".to_string()])
            .unwrap();
        assert_eq!(tags, vec!["mvp".to_string()]);

        let err = config
            .normalize_and_validate_tags(vec!["other".to_string()])
            .unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }
}
