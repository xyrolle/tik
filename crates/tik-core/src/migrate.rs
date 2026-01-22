use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use crate::fs;
use crate::timeutil;
use crate::{Result, TikError};

/// Represents a single file move during migration.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationMove {
    pub from: String,
    pub to: String,
}

/// Summary of a migration run.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationSummary {
    pub migrated_at: String,
    pub from_layout: String,
    pub to_layout: String,
    pub project: String,
    pub moves: Vec<MigrationMove>,
    pub created: Vec<String>,
    pub warnings: Vec<String>,
}

/// Schema version for tracking data migrations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
}

impl SchemaVersion {
    pub fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }

    pub fn parse(version: &str) -> Result<Self> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 2 {
            return Err(TikError::Schema(format!(
                "invalid schema version format: {version}"
            )));
        }
        let major = parts[0]
            .parse()
            .map_err(|_| TikError::Schema(format!("invalid major version: {}", parts[0])))?;
        let minor = parts[1]
            .parse()
            .map_err(|_| TikError::Schema(format!("invalid minor version: {}", parts[1])))?;
        Ok(Self { major, minor })
    }

    pub fn as_string(&self) -> String {
        format!("{}.{}", self.major, self.minor)
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// Result of a single migration.
#[derive(Debug, Clone, Serialize)]
pub struct MigrationResult {
    pub migration_id: String,
    pub from_version: String,
    pub to_version: String,
    pub tickets_migrated: usize,
    pub milestones_migrated: usize,
    pub events_migrated: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl MigrationResult {
    pub fn new(migration_id: &str, from: &SchemaVersion, to: &SchemaVersion) -> Self {
        Self {
            migration_id: migration_id.to_string(),
            from_version: from.as_string(),
            to_version: to.as_string(),
            tickets_migrated: 0,
            milestones_migrated: 0,
            events_migrated: 0,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Comprehensive migration report.
#[derive(Debug, Clone, Serialize)]
pub struct DataMigrationReport {
    pub started_at: String,
    pub completed_at: Option<String>,
    pub from_version: String,
    pub to_version: String,
    pub migrations: Vec<MigrationResult>,
    pub total_tickets: usize,
    pub total_milestones: usize,
    pub total_events: usize,
    pub all_warnings: Vec<String>,
    pub all_errors: Vec<String>,
    pub success: bool,
}

impl DataMigrationReport {
    pub fn new(from: &SchemaVersion, to: &SchemaVersion) -> Result<Self> {
        Ok(Self {
            started_at: timeutil::now_rfc3339()?,
            completed_at: None,
            from_version: from.as_string(),
            to_version: to.as_string(),
            migrations: Vec::new(),
            total_tickets: 0,
            total_milestones: 0,
            total_events: 0,
            all_warnings: Vec::new(),
            all_errors: Vec::new(),
            success: false,
        })
    }

    pub fn add_migration(&mut self, result: MigrationResult) {
        self.total_tickets += result.tickets_migrated;
        self.total_milestones += result.milestones_migrated;
        self.total_events += result.events_migrated;
        self.all_warnings.extend(result.warnings.clone());
        self.all_errors.extend(result.errors.clone());
        self.migrations.push(result);
    }

    pub fn finalize(&mut self, success: bool) -> Result<()> {
        self.completed_at = Some(timeutil::now_rfc3339()?);
        self.success = success && self.all_errors.is_empty();
        Ok(())
    }
}

/// Trait for data migrations.
pub trait DataMigration: Send + Sync {
    /// Unique identifier for this migration.
    fn id(&self) -> &str;

    /// Source schema version.
    fn source_version(&self) -> SchemaVersion;

    /// Target schema version.
    fn to_version(&self) -> SchemaVersion;

    /// Migrate a ticket JSON value.
    fn migrate_ticket(&self, ticket: &mut Value) -> Result<Vec<String>>;

    /// Migrate a milestone JSON value.
    fn migrate_milestone(&self, milestone: &mut Value) -> Result<Vec<String>> {
        let _ = milestone;
        Ok(Vec::new())
    }

    /// Migrate an event JSON value.
    fn migrate_event(&self, event: &mut Value) -> Result<Vec<String>> {
        let _ = event;
        Ok(Vec::new())
    }
}

/// Migration: v1.0 -> v1.1 - EstimateUnit enum.
pub struct MigrationV1_1;

impl DataMigration for MigrationV1_1 {
    fn id(&self) -> &str {
        "v1.0_to_v1.1_estimate_unit"
    }

    fn source_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 0)
    }

    fn to_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 1)
    }

    fn migrate_ticket(&self, ticket: &mut Value) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Migrate estimate.unit to enum
        if let Some(estimate) = ticket.get_mut("estimate") {
            if let Some(obj) = estimate.as_object_mut() {
                if let Some(unit) = obj.get("unit") {
                    if let Some(unit_str) = unit.as_str() {
                        let normalized = normalize_estimate_unit(unit_str);
                        if normalized != unit_str {
                            warnings.push(format!(
                                "estimate unit '{}' normalized to '{}'",
                                unit_str, normalized
                            ));
                        }
                        obj.insert("unit".to_string(), Value::String(normalized));
                    }
                }
            }
        }

        // Update schema version
        ticket["schema_version"] = Value::String("1.1".to_string());

        Ok(warnings)
    }
}

/// Migration: v1.1 -> v1.2 - Tag normalization.
pub struct MigrationV1_2;

impl DataMigration for MigrationV1_2 {
    fn id(&self) -> &str {
        "v1.1_to_v1.2_tag_normalization"
    }

    fn source_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 1)
    }

    fn to_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 2)
    }

    fn migrate_ticket(&self, ticket: &mut Value) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Normalize tags
        if let Some(tags) = ticket.get_mut("tags") {
            if let Some(tags_arr) = tags.as_array_mut() {
                let mut normalized_tags = Vec::new();
                for tag in tags_arr.iter() {
                    if let Some(tag_str) = tag.as_str() {
                        let normalized = normalize_tag(tag_str);
                        if normalized != tag_str {
                            warnings
                                .push(format!("tag '{}' normalized to '{}'", tag_str, normalized));
                        }
                        if !normalized_tags.contains(&normalized) {
                            normalized_tags.push(normalized);
                        }
                    }
                }
                *tags = Value::Array(normalized_tags.into_iter().map(Value::String).collect());
            }
        }

        // Update schema version
        ticket["schema_version"] = Value::String("1.2".to_string());

        Ok(warnings)
    }
}

/// Migration: v1.2 -> v1.3 - AcceptanceCriterion struct.
pub struct MigrationV1_3;

impl DataMigration for MigrationV1_3 {
    fn id(&self) -> &str {
        "v1.2_to_v1.3_acceptance_criteria"
    }

    fn source_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 2)
    }

    fn to_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 3)
    }

    fn migrate_ticket(&self, ticket: &mut Value) -> Result<Vec<String>> {
        let warnings = Vec::new();

        // Migrate acceptance from Vec<String> to Vec<AcceptanceCriterion>
        if let Some(acceptance) = ticket.get_mut("acceptance") {
            if let Some(arr) = acceptance.as_array() {
                let mut new_acceptance = Vec::new();
                for item in arr {
                    if let Some(text) = item.as_str() {
                        // Old format: just strings
                        new_acceptance.push(serde_json::json!({
                            "text": text,
                            "completed": false,
                            "completed_at": null
                        }));
                    } else if item.is_object() {
                        // Already in new format
                        new_acceptance.push(item.clone());
                    }
                }
                *acceptance = Value::Array(new_acceptance);
            }
        }

        // Update schema version
        ticket["schema_version"] = Value::String("1.3".to_string());

        Ok(warnings)
    }
}

/// Migration: v1.3 -> v2.0 - EventType enum.
pub struct MigrationV2_0;

impl DataMigration for MigrationV2_0 {
    fn id(&self) -> &str {
        "v1.3_to_v2.0_event_types"
    }

    fn source_version(&self) -> SchemaVersion {
        SchemaVersion::new(1, 3)
    }

    fn to_version(&self) -> SchemaVersion {
        SchemaVersion::new(2, 0)
    }

    fn migrate_ticket(&self, ticket: &mut Value) -> Result<Vec<String>> {
        // Update schema version
        ticket["schema_version"] = Value::String("2.0".to_string());
        Ok(Vec::new())
    }

    fn migrate_event(&self, event: &mut Value) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Normalize event type to standard enum values
        if let Some(kind) = event.get("type") {
            if let Some(kind_str) = kind.as_str() {
                let normalized = normalize_event_type(kind_str);
                if normalized != kind_str {
                    warnings.push(format!(
                        "event type '{}' normalized to '{}'",
                        kind_str, normalized
                    ));
                }
                event["type"] = Value::String(normalized);
            }
        }

        // Normalize actor format
        if let Some(actor) = event.get("actor") {
            if let Some(actor_str) = actor.as_str() {
                let normalized = normalize_actor(actor_str);
                if normalized != actor_str {
                    warnings.push(format!(
                        "actor '{}' normalized to '{}'",
                        actor_str, normalized
                    ));
                }
                event["actor"] = Value::String(normalized);
            }
        }

        Ok(warnings)
    }
}

/// Normalize estimate unit to enum value.
pub fn normalize_estimate_unit(unit: &str) -> String {
    let lower = unit.to_lowercase().trim().to_string();
    match lower.as_str() {
        "h" | "hour" | "hours" | "hr" | "hrs" => "hours".to_string(),
        "d" | "day" | "days" => "days".to_string(),
        "w" | "week" | "weeks" | "wk" | "wks" => "weeks".to_string(),
        "p" | "pt" | "pts" | "point" | "points" => "points".to_string(),
        "sp" | "story_point" | "story_points" | "storypoint" | "storypoints" => {
            "story_points".to_string()
        }
        _ => "points".to_string(), // Default unknown to points
    }
}

/// Normalize tag to slug format.
pub fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalize event type to standard enum value.
pub fn normalize_event_type(kind: &str) -> String {
    let lower = kind.to_lowercase().trim().to_string();
    match lower.as_str() {
        "created" | "create" => "created".to_string(),
        "note" | "comment" | "notes" => "note".to_string(),
        "status_change" | "statuschange" | "status" => "status_change".to_string(),
        "ticket_edited" | "ticketedited" | "edited" | "edit" => "ticket_edited".to_string(),
        "notes_edited" | "notesedited" => "notes_edited".to_string(),
        "relation_added" | "relationadded" => "relation_added".to_string(),
        "relation_removed" | "relationremoved" => "relation_removed".to_string(),
        "artifact_added" | "artifactadded" => "artifact_added".to_string(),
        "artifact_removed" | "artifactremoved" => "artifact_removed".to_string(),
        "milestone_set" | "milestoneset" => "milestone_set".to_string(),
        "milestone_cleared" | "milestonecleared" => "milestone_cleared".to_string(),
        "assignees_added" | "assigneesadded" | "assigned" => "assignees_added".to_string(),
        "assignees_removed" | "assigneesremoved" | "unassigned" => "assignees_removed".to_string(),
        "tags_added" | "tagsadded" | "tagged" => "tags_added".to_string(),
        "tags_removed" | "tagsremoved" | "untagged" => "tags_removed".to_string(),
        other => format!("custom:{}", other),
    }
}

/// Normalize actor to standard format (type:identifier).
pub fn normalize_actor(actor: &str) -> String {
    let trimmed = actor.trim();
    if trimmed.contains(':') {
        // Already in correct format
        return trimmed.to_string();
    }
    // Assume human actor if no prefix
    let lower = trimmed.to_lowercase();
    if lower == "system" || lower == "migration" || lower == "auto" {
        format!("system:{}", lower)
    } else if lower.starts_with("agent") || lower.starts_with("bot") || lower.starts_with("ai") {
        format!("agent:{}", trimmed)
    } else {
        format!("human:{}", trimmed)
    }
}

/// Registry of all available migrations.
pub struct MigrationRegistry {
    migrations: Vec<Box<dyn DataMigration>>,
}

impl MigrationRegistry {
    pub fn new() -> Self {
        let migrations: Vec<Box<dyn DataMigration>> = vec![
            Box::new(MigrationV1_1),
            Box::new(MigrationV1_2),
            Box::new(MigrationV1_3),
            Box::new(MigrationV2_0),
        ];
        Self { migrations }
    }

    /// Get migrations needed to go from one version to another.
    pub fn get_migration_path(
        &self,
        from: &SchemaVersion,
        to: &SchemaVersion,
    ) -> Vec<&dyn DataMigration> {
        let mut path = Vec::new();
        let mut current = from.clone();

        while current < *to {
            if let Some(migration) = self
                .migrations
                .iter()
                .find(|m| m.source_version() == current)
            {
                path.push(migration.as_ref());
                current = migration.to_version();
            } else {
                break;
            }
        }

        path
    }

    /// Get the latest schema version.
    pub fn latest_version(&self) -> SchemaVersion {
        self.migrations
            .iter()
            .map(|m| m.to_version())
            .max()
            .unwrap_or(SchemaVersion::new(1, 0))
    }
}

impl Default for MigrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Run migrations on a data directory.
pub fn run_migrations(
    data_root: &Path,
    from_version: &SchemaVersion,
    to_version: &SchemaVersion,
    dry_run: bool,
) -> Result<DataMigrationReport> {
    let registry = MigrationRegistry::new();
    let migrations = registry.get_migration_path(from_version, to_version);

    if migrations.is_empty() {
        return Err(TikError::Usage(format!(
            "no migration path from {} to {}",
            from_version, to_version
        )));
    }

    let mut report = DataMigrationReport::new(from_version, to_version)?;

    let tickets_dir = data_root.join("tickets");
    let milestones_dir = data_root.join("milestones");

    for migration in migrations {
        let mut result = MigrationResult::new(
            migration.id(),
            &migration.source_version(),
            &migration.to_version(),
        );

        // Migrate tickets
        if tickets_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&tickets_dir) {
                for entry in entries.flatten() {
                    let ticket_dir = entry.path();
                    if !ticket_dir.is_dir() {
                        continue;
                    }

                    let ticket_path = ticket_dir.join("ticket.json");
                    if !ticket_path.is_file() {
                        continue;
                    }

                    match migrate_json_file(&ticket_path, |v| migration.migrate_ticket(v), dry_run)
                    {
                        Ok(warnings) => {
                            result.tickets_migrated += 1;
                            for w in warnings {
                                result.add_warning(format!("{}: {}", ticket_path.display(), w));
                            }
                        }
                        Err(e) => {
                            result.add_error(format!("{}: {}", ticket_path.display(), e));
                        }
                    }

                    // Migrate events in notes.jsonl
                    let events_path = ticket_dir.join("notes.jsonl");
                    if events_path.is_file() {
                        match migrate_jsonl_file(
                            &events_path,
                            |v| migration.migrate_event(v),
                            dry_run,
                        ) {
                            Ok((count, warnings)) => {
                                result.events_migrated += count;
                                for w in warnings {
                                    result.add_warning(format!("{}: {}", events_path.display(), w));
                                }
                            }
                            Err(e) => {
                                result.add_error(format!("{}: {}", events_path.display(), e));
                            }
                        }
                    }
                }
            }
        }

        // Migrate milestones
        if milestones_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&milestones_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "json").unwrap_or(false) {
                        match migrate_json_file(&path, |v| migration.migrate_milestone(v), dry_run)
                        {
                            Ok(warnings) => {
                                result.milestones_migrated += 1;
                                for w in warnings {
                                    result.add_warning(format!("{}: {}", path.display(), w));
                                }
                            }
                            Err(e) => {
                                result.add_error(format!("{}: {}", path.display(), e));
                            }
                        }
                    }
                }
            }
        }

        report.add_migration(result);
    }

    report.finalize(true)?;
    Ok(report)
}

/// Migrate a single JSON file.
fn migrate_json_file<F>(path: &Path, migrator: F, dry_run: bool) -> Result<Vec<String>>
where
    F: FnOnce(&mut Value) -> Result<Vec<String>>,
{
    let content = fs::read_to_string(path)?;
    let mut value: Value = serde_json::from_str(&content)
        .map_err(|e| TikError::Schema(format!("parse {}: {}", path.display(), e)))?;

    let warnings = migrator(&mut value)?;

    if !dry_run {
        let output = serde_json::to_string_pretty(&value)
            .map_err(|e| TikError::Schema(format!("serialize {}: {}", path.display(), e)))?;
        fs::write_string_atomic(path, &output)?;
    }

    Ok(warnings)
}

/// Migrate a JSONL file (line-delimited JSON).
fn migrate_jsonl_file<F>(path: &Path, migrator: F, dry_run: bool) -> Result<(usize, Vec<String>)>
where
    F: Fn(&mut Value) -> Result<Vec<String>>,
{
    let content = fs::read_to_string(path)?;
    let mut all_warnings = Vec::new();
    let mut migrated_lines = Vec::new();
    let mut count = 0;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut value: Value = serde_json::from_str(line)
            .map_err(|e| TikError::Schema(format!("parse line in {}: {}", path.display(), e)))?;

        let warnings = migrator(&mut value)?;
        all_warnings.extend(warnings);

        let output = serde_json::to_string(&value).map_err(|e| {
            TikError::Schema(format!("serialize line in {}: {}", path.display(), e))
        })?;
        migrated_lines.push(output);
        count += 1;
    }

    if !dry_run {
        let output = migrated_lines.join("\n") + "\n";
        fs::write_string_atomic(path, &output)?;
    }

    Ok((count, all_warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_parsing() {
        let v = SchemaVersion::parse("1.0").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 0);

        let v2 = SchemaVersion::parse("2.5").unwrap();
        assert_eq!(v2.major, 2);
        assert_eq!(v2.minor, 5);

        assert!(SchemaVersion::parse("invalid").is_err());
    }

    #[test]
    fn test_schema_version_ordering() {
        let v1_0 = SchemaVersion::new(1, 0);
        let v1_1 = SchemaVersion::new(1, 1);
        let v2_0 = SchemaVersion::new(2, 0);

        assert!(v1_0 < v1_1);
        assert!(v1_1 < v2_0);
        assert!(v1_0 < v2_0);
    }

    #[test]
    fn test_normalize_estimate_unit() {
        assert_eq!(normalize_estimate_unit("hours"), "hours");
        assert_eq!(normalize_estimate_unit("h"), "hours");
        assert_eq!(normalize_estimate_unit("HOURS"), "hours");
        assert_eq!(normalize_estimate_unit("days"), "days");
        assert_eq!(normalize_estimate_unit("d"), "days");
        assert_eq!(normalize_estimate_unit("weeks"), "weeks");
        assert_eq!(normalize_estimate_unit("points"), "points");
        assert_eq!(normalize_estimate_unit("story_points"), "story_points");
        assert_eq!(normalize_estimate_unit("unknown"), "points");
    }

    #[test]
    fn test_normalize_tag() {
        assert_eq!(normalize_tag("MVP"), "mvp");
        assert_eq!(normalize_tag("  high priority  "), "high-priority");
        assert_eq!(normalize_tag("bug/fix"), "bug-fix");
        assert_eq!(normalize_tag("feature--request"), "feature-request");
    }

    #[test]
    fn test_normalize_event_type() {
        assert_eq!(normalize_event_type("created"), "created");
        assert_eq!(normalize_event_type("Create"), "created");
        assert_eq!(normalize_event_type("note"), "note");
        assert_eq!(normalize_event_type("comment"), "note");
        assert_eq!(normalize_event_type("status_change"), "status_change");
        assert_eq!(normalize_event_type("unknown_type"), "custom:unknown_type");
    }

    #[test]
    fn test_normalize_actor() {
        assert_eq!(normalize_actor("alice"), "human:alice");
        assert_eq!(normalize_actor("human:bob"), "human:bob");
        assert_eq!(normalize_actor("system"), "system:system");
        assert_eq!(normalize_actor("agent:claude"), "agent:claude");
        assert_eq!(normalize_actor("bot_assistant"), "agent:bot_assistant");
    }

    #[test]
    fn test_migration_registry() {
        let registry = MigrationRegistry::new();
        let v1_0 = SchemaVersion::new(1, 0);
        let v2_0 = SchemaVersion::new(2, 0);

        let path = registry.get_migration_path(&v1_0, &v2_0);
        assert_eq!(path.len(), 4);
        assert_eq!(path[0].id(), "v1.0_to_v1.1_estimate_unit");
        assert_eq!(path[1].id(), "v1.1_to_v1.2_tag_normalization");
        assert_eq!(path[2].id(), "v1.2_to_v1.3_acceptance_criteria");
        assert_eq!(path[3].id(), "v1.3_to_v2.0_event_types");
    }
}
