use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::fs;
use crate::migrate::{normalize_estimate_unit, normalize_tag, MigrationRegistry, SchemaVersion};
use crate::timeutil;
use crate::Result;

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub generated_at: String,
    pub repo_root: String,
    pub layout_version: String,
    pub checks: Vec<DoctorCheck>,
    pub summary: DoctorSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorSummary {
    pub ok: usize,
    pub warnings: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub id: String,
    pub status: DoctorStatus,
    pub scope: String,
    pub message: String,
    pub details: Vec<String>,
    pub hint: Option<String>,
}

impl DoctorReport {
    pub fn new(repo_root: &str, layout_version: &str, checks: Vec<DoctorCheck>) -> Result<Self> {
        let summary = DoctorSummary::from_checks(&checks);
        Ok(Self {
            generated_at: timeutil::now_rfc3339()?,
            repo_root: repo_root.to_string(),
            layout_version: layout_version.to_string(),
            checks,
            summary,
        })
    }
}

impl DoctorSummary {
    pub fn from_checks(checks: &[DoctorCheck]) -> Self {
        let mut ok = 0usize;
        let mut warnings = 0usize;
        let mut errors = 0usize;
        for check in checks {
            match check.status {
                DoctorStatus::Ok => ok += 1,
                DoctorStatus::Warn => warnings += 1,
                DoctorStatus::Error => errors += 1,
            }
        }
        Self {
            ok,
            warnings,
            errors,
        }
    }
}

impl DoctorStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DoctorStatus::Ok => "ok",
            DoctorStatus::Warn => "warn",
            DoctorStatus::Error => "error",
        }
    }
}

impl DoctorCheck {
    pub fn ok(id: &str, scope: &str, message: &str) -> Self {
        Self {
            id: id.to_string(),
            status: DoctorStatus::Ok,
            scope: scope.to_string(),
            message: message.to_string(),
            details: Vec::new(),
            hint: None,
        }
    }

    pub fn warn(
        id: &str,
        scope: &str,
        message: &str,
        details: Vec<String>,
        hint: Option<&str>,
    ) -> Self {
        Self {
            id: id.to_string(),
            status: DoctorStatus::Warn,
            scope: scope.to_string(),
            message: message.to_string(),
            details,
            hint: hint.map(|value| value.to_string()),
        }
    }

    pub fn error(
        id: &str,
        scope: &str,
        message: &str,
        details: Vec<String>,
        hint: Option<&str>,
    ) -> Self {
        Self {
            id: id.to_string(),
            status: DoctorStatus::Error,
            scope: scope.to_string(),
            message: message.to_string(),
            details,
            hint: hint.map(|value| value.to_string()),
        }
    }
}

/// Check if schema version needs migration.
pub fn check_schema_version(data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "schema";
    let registry = MigrationRegistry::new();
    let latest = registry.latest_version();

    let tickets_dir = data_root.join("tickets");
    if !tickets_dir.is_dir() {
        return checks;
    }

    let mut outdated_tickets = Vec::new();
    let mut current_versions: HashSet<String> = HashSet::new();

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

            if let Ok(content) = fs::read_to_string(&ticket_path) {
                if let Ok(ticket) = serde_json::from_str::<Value>(&content) {
                    if let Some(version_str) = ticket.get("schema_version").and_then(|v| v.as_str())
                    {
                        current_versions.insert(version_str.to_string());
                        if let Ok(version) = SchemaVersion::parse(version_str) {
                            if version < latest {
                                let id = ticket
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown");
                                outdated_tickets.push(format!("{} (v{})", id, version_str));
                            }
                        }
                    }
                }
            }
        }
    }

    if outdated_tickets.is_empty() {
        checks.push(DoctorCheck::ok(
            "schema_version_current",
            scope,
            &format!("all tickets at schema version {}", latest),
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "schema_version_outdated",
            scope,
            &format!(
                "{} tickets have outdated schema versions",
                outdated_tickets.len()
            ),
            outdated_tickets,
            Some("run `tik migrate` to update all tickets to the latest schema"),
        ));
    }

    checks
}

/// Check for unnormalized tags.
pub fn check_tag_normalization(data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "data_quality";

    let tickets_dir = data_root.join("tickets");
    if !tickets_dir.is_dir() {
        return checks;
    }

    let mut unnormalized_tags: Vec<String> = Vec::new();

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

            if let Ok(content) = fs::read_to_string(&ticket_path) {
                if let Ok(ticket) = serde_json::from_str::<Value>(&content) {
                    let ticket_id = ticket
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    if let Some(tags) = ticket.get("tags").and_then(|v| v.as_array()) {
                        for tag in tags {
                            if let Some(tag_str) = tag.as_str() {
                                let normalized = normalize_tag(tag_str);
                                if normalized != tag_str {
                                    unnormalized_tags.push(format!(
                                        "{}: '{}' -> '{}'",
                                        ticket_id, tag_str, normalized
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if unnormalized_tags.is_empty() {
        checks.push(DoctorCheck::ok(
            "tags_normalized",
            scope,
            "all tags are properly normalized",
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "tags_unnormalized",
            scope,
            &format!("{} tags need normalization", unnormalized_tags.len()),
            unnormalized_tags,
            Some("run `tik migrate` to normalize all tags"),
        ));
    }

    checks
}

/// Check for freeform estimate units.
pub fn check_estimate_units(data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "data_quality";

    let tickets_dir = data_root.join("tickets");
    if !tickets_dir.is_dir() {
        return checks;
    }

    let mut freeform_units: Vec<String> = Vec::new();
    let valid_units = ["hours", "days", "weeks", "points", "story_points"];

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

            if let Ok(content) = fs::read_to_string(&ticket_path) {
                if let Ok(ticket) = serde_json::from_str::<Value>(&content) {
                    let ticket_id = ticket
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");

                    if let Some(estimate) = ticket.get("estimate") {
                        if let Some(unit) = estimate.get("unit").and_then(|v| v.as_str()) {
                            if !valid_units.contains(&unit) {
                                let normalized = normalize_estimate_unit(unit);
                                freeform_units
                                    .push(format!("{}: '{}' -> '{}'", ticket_id, unit, normalized));
                            }
                        }
                    }
                }
            }
        }
    }

    if freeform_units.is_empty() {
        checks.push(DoctorCheck::ok(
            "estimate_units_valid",
            scope,
            "all estimate units are valid enum values",
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "estimate_units_freeform",
            scope,
            &format!("{} estimates have freeform units", freeform_units.len()),
            freeform_units,
            Some("run `tik migrate` to normalize estimate units"),
        ));
    }

    checks
}

/// Check referential integrity (milestone_id, relation targets).
pub fn check_referential_integrity(data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "integrity";

    let tickets_dir = data_root.join("tickets");
    let milestones_dir = data_root.join("milestones");

    // Collect all ticket IDs
    let mut ticket_ids: HashSet<String> = HashSet::new();
    if tickets_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&tickets_dir) {
            for entry in entries.flatten() {
                let ticket_dir = entry.path();
                if ticket_dir.is_dir() {
                    let ticket_path = ticket_dir.join("ticket.json");
                    if ticket_path.is_file() {
                        if let Ok(content) = fs::read_to_string(&ticket_path) {
                            if let Ok(ticket) = serde_json::from_str::<Value>(&content) {
                                if let Some(id) = ticket.get("id").and_then(|v| v.as_str()) {
                                    ticket_ids.insert(id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Collect all milestone IDs
    let mut milestone_ids: HashSet<String> = HashSet::new();
    if milestones_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&milestones_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(milestone) = serde_json::from_str::<Value>(&content) {
                            if let Some(id) = milestone.get("id").and_then(|v| v.as_str()) {
                                milestone_ids.insert(id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Check tickets for broken references
    let mut broken_milestone_refs: Vec<String> = Vec::new();
    let mut broken_relation_refs: Vec<String> = Vec::new();

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

                if let Ok(content) = fs::read_to_string(&ticket_path) {
                    if let Ok(ticket) = serde_json::from_str::<Value>(&content) {
                        let ticket_id = ticket
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        // Check milestone reference
                        if let Some(milestone_id) =
                            ticket.get("milestone_id").and_then(|v| v.as_str())
                        {
                            if !milestone_ids.contains(milestone_id) {
                                broken_milestone_refs
                                    .push(format!("{} -> {} (not found)", ticket_id, milestone_id));
                            }
                        }

                        // Check relation references
                        if let Some(relations) = ticket.get("relations").and_then(|v| v.as_array())
                        {
                            for rel in relations {
                                if let Some(target_id) = rel.get("id").and_then(|v| v.as_str()) {
                                    if !ticket_ids.contains(target_id) {
                                        let rel_type = rel
                                            .get("type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown");
                                        broken_relation_refs.push(format!(
                                            "{} -{} -> {} (not found)",
                                            ticket_id, rel_type, target_id
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if broken_milestone_refs.is_empty() {
        checks.push(DoctorCheck::ok(
            "milestone_refs_valid",
            scope,
            "all milestone references are valid",
        ));
    } else {
        checks.push(DoctorCheck::error(
            "milestone_refs_broken",
            scope,
            &format!(
                "{} broken milestone references",
                broken_milestone_refs.len()
            ),
            broken_milestone_refs,
            Some("clear the invalid milestone_id or create the missing milestone"),
        ));
    }

    if broken_relation_refs.is_empty() {
        checks.push(DoctorCheck::ok(
            "relation_refs_valid",
            scope,
            "all relation references are valid",
        ));
    } else {
        checks.push(DoctorCheck::error(
            "relation_refs_broken",
            scope,
            &format!("{} broken relation references", broken_relation_refs.len()),
            broken_relation_refs,
            Some("remove the invalid relations or restore the missing tickets"),
        ));
    }

    checks
}

/// Check for stale lock files.
pub fn check_stale_locks(tik_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "locks";

    let locks_dir = tik_root.join("locks");
    if !locks_dir.is_dir() {
        return checks;
    }

    let mut stale_locks: Vec<String> = Vec::new();
    let stale_threshold = Duration::from_secs(3600); // 1 hour

    if let Ok(entries) = std::fs::read_dir(&locks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "lock").unwrap_or(false) {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(age) = SystemTime::now().duration_since(modified) {
                            if age > stale_threshold {
                                let filename = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                let age_mins = age.as_secs() / 60;
                                stale_locks
                                    .push(format!("{} ({} minutes old)", filename, age_mins));
                            }
                        }
                    }
                }
            }
        }
    }

    if stale_locks.is_empty() {
        checks.push(DoctorCheck::ok(
            "locks_fresh",
            scope,
            "no stale lock files detected",
        ));
    } else {
        checks.push(DoctorCheck::warn(
            "locks_stale",
            scope,
            &format!("{} stale lock files detected", stale_locks.len()),
            stale_locks,
            Some("these locks may be from crashed processes; consider removing them if no tik process is running"),
        ));
    }

    checks
}

/// Check event log consistency.
pub fn check_event_log_consistency(data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "events";

    let tickets_dir = data_root.join("tickets");
    if !tickets_dir.is_dir() {
        return checks;
    }

    let mut issues: Vec<String> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&tickets_dir) {
        for entry in entries.flatten() {
            let ticket_dir = entry.path();
            if !ticket_dir.is_dir() {
                continue;
            }

            let events_path = ticket_dir.join("notes.jsonl");
            if !events_path.is_file() {
                continue;
            }

            let ticket_id = ticket_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Ok(content) = fs::read_to_string(&events_path) {
                let mut last_ts: Option<String> = None;
                let mut has_created = false;

                for (line_num, line) in content.lines().enumerate() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<Value>(line) {
                        Ok(event) => {
                            if let Some(event_type) = event.get("type").and_then(|v| v.as_str()) {
                                if event_type == "created" {
                                    has_created = true;
                                }
                            }

                            if let Some(ts) = event.get("ts").and_then(|v| v.as_str()) {
                                if let Some(ref prev_ts) = last_ts {
                                    if ts < prev_ts.as_str() {
                                        issues.push(format!(
                                            "{}: out-of-order timestamps at line {}",
                                            ticket_id,
                                            line_num + 1
                                        ));
                                    }
                                }
                                last_ts = Some(ts.to_string());
                            }
                        }
                        Err(e) => {
                            issues.push(format!(
                                "{}: malformed JSON at line {}: {}",
                                ticket_id,
                                line_num + 1,
                                e
                            ));
                        }
                    }
                }

                if !has_created && !content.trim().is_empty() {
                    issues.push(format!("{}: missing 'created' event", ticket_id));
                }
            }
        }
    }

    if issues.is_empty() {
        checks.push(DoctorCheck::ok(
            "event_logs_consistent",
            scope,
            "all event logs are consistent",
        ));
    } else {
        checks.push(DoctorCheck::error(
            "event_logs_issues",
            scope,
            &format!("{} event log issues detected", issues.len()),
            issues,
            Some("review and fix the event log files manually or restore from backup"),
        ));
    }

    checks
}

/// Check index staleness.
pub fn check_index_staleness(data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();
    let scope = "index";

    let index_dir = data_root.join("index");
    let tickets_dir = data_root.join("tickets");

    let fts_path = index_dir.join("fts.sqlite");
    let jsonl_path = index_dir.join("tickets.jsonl");

    let index_path = if fts_path.is_file() {
        Some(&fts_path)
    } else if jsonl_path.is_file() {
        Some(&jsonl_path)
    } else {
        None
    };

    if index_path.is_none() {
        checks.push(DoctorCheck::warn(
            "index_missing",
            scope,
            "no index found",
            Vec::new(),
            Some("run `tik index rebuild` to create the search index"),
        ));
        return checks;
    }

    let index_path = index_path.unwrap();
    let index_modified = std::fs::metadata(index_path)
        .ok()
        .and_then(|m| m.modified().ok());

    if tickets_dir.is_dir() {
        let mut newer_tickets: Vec<String> = Vec::new();

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

                if let Some(index_time) = index_modified {
                    if let Ok(ticket_meta) = std::fs::metadata(&ticket_path) {
                        if let Ok(ticket_time) = ticket_meta.modified() {
                            if ticket_time > index_time {
                                let id = ticket_dir
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default();
                                newer_tickets.push(id);
                            }
                        }
                    }
                }
            }
        }

        if newer_tickets.is_empty() {
            checks.push(DoctorCheck::ok(
                "index_current",
                scope,
                "search index is up to date",
            ));
        } else {
            checks.push(DoctorCheck::warn(
                "index_stale",
                scope,
                &format!("{} tickets modified since last index", newer_tickets.len()),
                newer_tickets,
                Some("run `tik index rebuild` to update the search index"),
            ));
        }
    }

    checks
}

/// Run all comprehensive doctor checks for a project.
pub fn run_comprehensive_checks(tik_root: &Path, data_root: &Path) -> Vec<DoctorCheck> {
    let mut checks = Vec::new();

    checks.extend(check_schema_version(data_root));
    checks.extend(check_tag_normalization(data_root));
    checks.extend(check_estimate_units(data_root));
    checks.extend(check_referential_integrity(data_root));
    checks.extend(check_stale_locks(tik_root));
    checks.extend(check_event_log_consistency(data_root));
    checks.extend(check_index_staleness(data_root));

    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};
    use serde_json::json;
    use tempfile::tempdir;

    fn write_ticket(
        data_root: &Path,
        id: &str,
        schema_version: &str,
        tags: &[&str],
        estimate_unit: Option<&str>,
        milestone_id: Option<&str>,
        relations: &[(&str, &str)],
    ) {
        let ticket_dir = data_root.join("tickets").join(id);
        std::fs::create_dir_all(&ticket_dir).unwrap();
        let mut ticket = json!({
            "id": id,
            "schema_version": schema_version,
            "tags": tags,
        });
        if let Some(unit) = estimate_unit {
            ticket["estimate"] = json!({"value": 1.0, "unit": unit});
        }
        if let Some(milestone) = milestone_id {
            ticket["milestone_id"] = json!(milestone);
        }
        if !relations.is_empty() {
            let rels: Vec<Value> = relations
                .iter()
                .map(|(kind, target)| json!({"type": kind, "id": target}))
                .collect();
            ticket["relations"] = json!(rels);
        }
        let path = ticket_dir.join("ticket.json");
        std::fs::write(path, serde_json::to_string_pretty(&ticket).unwrap()).unwrap();
    }

    fn write_milestone(data_root: &Path, id: &str) {
        let milestone_dir = data_root.join("milestones");
        std::fs::create_dir_all(&milestone_dir).unwrap();
        let milestone = json!({ "id": id });
        let path = milestone_dir.join(format!("{id}.json"));
        std::fs::write(path, serde_json::to_string_pretty(&milestone).unwrap()).unwrap();
    }

    #[test]
    fn summary_counts_statuses() {
        let checks = vec![
            DoctorCheck::ok("ok", "scope", "ok"),
            DoctorCheck::warn("warn", "scope", "warn", Vec::new(), None),
            DoctorCheck::error("err", "scope", "err", Vec::new(), None),
        ];
        let summary = DoctorSummary::from_checks(&checks);
        assert_eq!(summary.ok, 1);
        assert_eq!(summary.warnings, 1);
        assert_eq!(summary.errors, 1);
        assert_eq!(DoctorStatus::Warn.as_str(), "warn");
    }

    #[test]
    fn schema_version_checks_detect_outdated() {
        let dir = tempdir().unwrap();
        let data_root = dir.path();
        let latest = MigrationRegistry::new().latest_version();
        let outdated = SchemaVersion::new(latest.major.saturating_sub(1), latest.minor);
        write_ticket(
            data_root,
            "T-ONE",
            &outdated.as_string(),
            &[],
            None,
            None,
            &[],
        );
        write_ticket(
            data_root,
            "T-TWO",
            &latest.as_string(),
            &[],
            None,
            None,
            &[],
        );
        let checks = check_schema_version(data_root);
        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, DoctorStatus::Warn));
        assert!(checks[0].message.contains("outdated"));
    }

    #[test]
    fn tag_and_estimate_normalization_checks() {
        let dir = tempdir().unwrap();
        let data_root = dir.path();
        write_ticket(
            data_root,
            "T-ONE",
            "1.0",
            &["Needs Normalize"],
            Some("hrs"),
            None,
            &[],
        );
        let tag_checks = check_tag_normalization(data_root);
        assert_eq!(tag_checks.len(), 1);
        assert!(matches!(tag_checks[0].status, DoctorStatus::Warn));
        let estimate_checks = check_estimate_units(data_root);
        assert_eq!(estimate_checks.len(), 1);
        assert!(matches!(estimate_checks[0].status, DoctorStatus::Warn));
    }

    #[test]
    fn referential_integrity_detects_broken_links() {
        let dir = tempdir().unwrap();
        let data_root = dir.path();
        write_ticket(
            data_root,
            "T-ONE",
            "1.0",
            &[],
            None,
            Some("M-MISSING"),
            &[("blocks", "T-MISSING")],
        );
        write_ticket(data_root, "T-TWO", "1.0", &[], None, None, &[]);
        write_milestone(data_root, "M-REAL");

        let checks = check_referential_integrity(data_root);
        assert_eq!(checks.len(), 2);
        assert!(checks.iter().any(|c| matches!(c.status, DoctorStatus::Error)));
        assert!(checks[0].message.contains("milestone"));
        assert!(checks[1].message.contains("relation"));
    }

    #[test]
    fn stale_locks_are_reported() {
        let dir = tempdir().unwrap();
        let tik_root = dir.path();
        let locks_dir = tik_root.join("locks");
        std::fs::create_dir_all(&locks_dir).unwrap();
        let lock_path = locks_dir.join("repo.lock");
        std::fs::write(&lock_path, "lock").unwrap();
        let old_time = FileTime::from_system_time(SystemTime::now() - Duration::from_secs(7200));
        set_file_mtime(&lock_path, old_time).unwrap();

        let checks = check_stale_locks(tik_root);
        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, DoctorStatus::Warn));
        assert!(checks[0].details[0].contains("minutes old"));
    }

    #[test]
    fn event_log_consistency_flags_issues() {
        let dir = tempdir().unwrap();
        let data_root = dir.path();
        let ticket_dir = data_root.join("tickets").join("T-ONE");
        std::fs::create_dir_all(&ticket_dir).unwrap();
        let events = [
            r#"{"type":"note","ts":"2026-01-02T00:00:00Z"}"#,
            r#"{"type":"created","ts":"2026-01-01T00:00:00Z"}"#,
            r#"{"type":"note","ts":"2026-01-03T00:00:00Z"}"#,
            r#"{bad json"#,
        ]
        .join("\n");
        std::fs::write(ticket_dir.join("notes.jsonl"), events).unwrap();

        let checks = check_event_log_consistency(data_root);
        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, DoctorStatus::Error));
        assert!(checks[0].details.iter().any(|d| d.contains("malformed JSON")));
    }

    #[test]
    fn index_staleness_reports_missing_and_stale() {
        let dir = tempdir().unwrap();
        let data_root = dir.path();
        let missing = check_index_staleness(data_root);
        assert_eq!(missing.len(), 1);
        assert!(matches!(missing[0].status, DoctorStatus::Warn));
        assert!(missing[0].message.contains("no index"));

        let index_dir = data_root.join("index");
        std::fs::create_dir_all(&index_dir).unwrap();
        std::fs::write(index_dir.join("tickets.jsonl"), "[]\n").unwrap();

        write_ticket(data_root, "T-ONE", "1.0", &[], None, None, &[]);
        let checks = check_index_staleness(data_root);
        assert_eq!(checks.len(), 1);
        assert!(matches!(checks[0].status, DoctorStatus::Warn));
        assert!(checks[0].message.contains("modified since last index"));
    }
}
