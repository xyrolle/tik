use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Map, Value};
use tik_core::{
    ArtifactType, Estimate, Milestone, MilestoneStatus, NewMilestone, NewTicket, Priority,
    RelationType, Repo, Result, Severity, Ticket, TicketStatus, TicketType, TikError,
};

fn main() -> ExitCode {
    let args = SeedArgs::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error (code {}): {err}", err.code().as_u8());
            ExitCode::from(err.code().as_u8())
        }
    }
}

struct SeedArgs {
    path: PathBuf,
}

impl SeedArgs {
    fn parse() -> Self {
        let mut path = None;
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--path" => {
                    let value = args.next().unwrap_or_else(|| {
                        eprintln!("missing value for --path");
                        std::process::exit(2);
                    });
                    path = Some(PathBuf::from(value));
                }
                "--help" | "-h" => {
                    println!("usage: seed_fixture [--path <dir>]");
                    std::process::exit(0);
                }
                other => {
                    eprintln!("unknown argument: {other}");
                    std::process::exit(2);
                }
            }
        }
        Self {
            path: path.unwrap_or_else(|| PathBuf::from("fixtures/seed")),
        }
    }
}

fn run(args: SeedArgs) -> Result<()> {
    if !args.path.exists() {
        std::fs::create_dir_all(&args.path).map_err(|err| TikError::io("create seed path", err))?;
    }

    let repo = if args.path.join(".tik").is_dir() {
        Repo::open(&args.path)?
    } else {
        Repo::init(&args.path, env!("CARGO_PKG_VERSION"))?
    };

    let actor = "seed";

    let mvp = ensure_milestone(
        &repo,
        "MVP",
        "First release slice",
        Some("2026-02-15T00:00:00Z"),
        &["mvp", "seed"],
        actor,
    )?;
    let ga = ensure_milestone(
        &repo,
        "Release 0.1",
        "General availability",
        Some("2026-03-01T00:00:00Z"),
        &["release", "seed"],
        actor,
    )?;
    if ga.status != MilestoneStatus::Closed {
        repo.close_milestone(&ga.id, actor, Some("seed close"))?;
    }

    let bootstrap = ensure_ticket(
        &repo,
        "Bootstrap project",
        "Scaffold the repo layout",
        "Seed fixture for repo scaffolding and UX",
        &["mvp", "cli"],
        actor,
    )?;
    let tui = ensure_ticket(
        &repo,
        "Add TUI polish",
        "Improve layout and status bar",
        "Seed fixture for TUI ergonomics and layout",
        &["ui", "tui"],
        actor,
    )?;
    let locking = ensure_ticket(
        &repo,
        "Handle lock contention",
        "Surface lock wait errors",
        "Seed fixture for lock handling and messaging",
        &["core", "locking"],
        actor,
    )?;
    let docs = ensure_ticket(
        &repo,
        "Document config",
        "Add config examples",
        "Seed fixture for configuration docs",
        &["docs"],
        actor,
    )?;
    let archive = ensure_ticket(
        &repo,
        "Seed: Archive test",
        "Archived sample for UI coverage",
        "Seed fixture for archived status rendering",
        &["seed", "archive"],
        actor,
    )?;
    let parent = ensure_ticket(
        &repo,
        "Seed: Parent",
        "Parent relationship sample",
        "Seed fixture for parent/child relationships",
        &["seed", "rel"],
        actor,
    )?;
    let child = ensure_ticket(
        &repo,
        "Seed: Child",
        "Child relationship sample",
        "Seed fixture for parent/child relationships",
        &["seed", "rel"],
        actor,
    )?;
    let duplicate = ensure_ticket(
        &repo,
        "Seed: Duplicate",
        "Duplicate relationship sample",
        "Seed fixture for duplicate relationships",
        &["seed", "rel"],
        actor,
    )?;

    apply_ticket_patch(&repo, &bootstrap.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Feature;
        ticket.priority = Priority::High;
        ticket.severity = Severity::Normal;
        ticket.acceptance = vec![
            "init works end-to-end".to_string(),
            "tui launches with default repo".to_string(),
        ];
        ticket.estimate = Some(Estimate {
            value: 3.0,
            unit: "days".to_string(),
        });
        ticket.due_at = Some("2026-02-01T00:00:00Z".to_string());
        ticket.custom = seed_custom("cli", true);
    })?;
    apply_ticket_patch(&repo, &tui.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Feature;
        ticket.priority = Priority::Medium;
        ticket.severity = Severity::Low;
        ticket.custom = seed_custom("tui", true);
    })?;
    apply_ticket_patch(&repo, &locking.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Bug;
        ticket.priority = Priority::Critical;
        ticket.severity = Severity::Critical;
        ticket.custom = seed_custom("locks", true);
    })?;
    apply_ticket_patch(&repo, &docs.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Chore;
        ticket.priority = Priority::Low;
        ticket.severity = Severity::Low;
        ticket.custom = seed_custom("docs", true);
    })?;
    apply_ticket_patch(&repo, &archive.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Spike;
        ticket.priority = Priority::Medium;
        ticket.severity = Severity::High;
        ticket.custom = seed_custom("archive", true);
    })?;
    apply_ticket_patch(&repo, &parent.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Task;
        ticket.priority = Priority::Medium;
        ticket.severity = Severity::Normal;
        ticket.custom = seed_custom("rel", true);
    })?;
    apply_ticket_patch(&repo, &child.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Task;
        ticket.priority = Priority::Medium;
        ticket.severity = Severity::Normal;
        ticket.custom = seed_custom("rel", true);
    })?;
    apply_ticket_patch(&repo, &duplicate.id, actor, "seed fields", |ticket| {
        ticket.kind = TicketType::Task;
        ticket.priority = Priority::Medium;
        ticket.severity = Severity::Normal;
        ticket.custom = seed_custom("rel", true);
    })?;

    ensure_status_transitions(
        &repo,
        &bootstrap.id,
        actor,
        &[TicketStatus::Closed, TicketStatus::Open],
    )?;
    ensure_status(&repo, &bootstrap.id, actor, TicketStatus::Open)?;
    ensure_status(&repo, &tui.id, actor, TicketStatus::InProgress)?;
    ensure_status(&repo, &locking.id, actor, TicketStatus::Blocked)?;
    ensure_status(&repo, &docs.id, actor, TicketStatus::Closed)?;
    ensure_status(&repo, &archive.id, actor, TicketStatus::Archived)?;

    repo.add_tags(
        &bootstrap.id,
        vec!["seed".into(), "demo".into()],
        actor,
        Some("seed add"),
    )?;
    repo.remove_tags(
        &bootstrap.id,
        vec!["demo".into()],
        actor,
        Some("seed remove"),
    )?;
    repo.set_tags(
        &bootstrap.id,
        vec!["mvp".into(), "cli".into(), "seed".into()],
        actor,
        Some("seed set"),
    )?;

    repo.add_assignees(
        &tui.id,
        vec!["ada".into(), "sam".into()],
        actor,
        Some("seed add"),
    )?;
    repo.remove_assignees(&tui.id, vec!["sam".into()], actor, Some("seed remove"))?;
    repo.set_assignees(
        &tui.id,
        vec!["ada".into(), "lee".into()],
        actor,
        Some("seed set"),
    )?;

    repo.add_artifact(
        &bootstrap.id,
        ArtifactType::File,
        "docs/seed.md",
        actor,
        Some("seed"),
    )?;
    repo.add_artifact(
        &bootstrap.id,
        ArtifactType::Url,
        "https://example.com/spec",
        actor,
        Some("seed"),
    )?;
    repo.add_artifact(
        &locking.id,
        ArtifactType::Commit,
        "deadbeef",
        actor,
        Some("seed"),
    )?;
    repo.add_artifact(
        &duplicate.id,
        ArtifactType::File,
        "docs/duplicate.md",
        actor,
        Some("seed"),
    )?;
    repo.remove_artifact(
        &duplicate.id,
        ArtifactType::File,
        "docs/duplicate.md",
        actor,
        Some("seed remove"),
    )?;

    repo.add_relation(
        &bootstrap.id,
        RelationType::Blocks,
        &locking.id,
        actor,
        Some("seed"),
    )?;
    repo.add_relation(
        &locking.id,
        RelationType::BlockedBy,
        &bootstrap.id,
        actor,
        Some("seed"),
    )?;
    repo.add_relation(
        &tui.id,
        RelationType::DependsOn,
        &bootstrap.id,
        actor,
        Some("seed"),
    )?;
    repo.add_relation(
        &duplicate.id,
        RelationType::Duplicate,
        &docs.id,
        actor,
        Some("seed"),
    )?;
    repo.add_relation(
        &parent.id,
        RelationType::Parent,
        &child.id,
        actor,
        Some("seed"),
    )?;
    repo.add_relation(
        &child.id,
        RelationType::Child,
        &parent.id,
        actor,
        Some("seed"),
    )?;

    repo.set_ticket_milestone(&bootstrap.id, Some(&mvp.id), actor, Some("seed set"))?;
    repo.set_ticket_milestone(&bootstrap.id, None, actor, Some("seed clear"))?;
    repo.set_ticket_milestone(&parent.id, Some(&ga.id), actor, Some("seed set"))?;

    ensure_note(&repo, &tui.id, "Drafted new layout for ticket list", actor)?;
    ensure_note(&repo, &locking.id, "Added lock timeout messaging", actor)?;
    ensure_note(&repo, &docs.id, "Docs refreshed", actor)?;
    ensure_notes_edited(&repo, &tui.id, actor)?;

    Ok(())
}

fn ensure_milestone(
    repo: &Repo,
    title: &str,
    description: &str,
    due_at: Option<&str>,
    tags: &[&str],
    actor: &str,
) -> Result<Milestone> {
    let milestones = repo.list_milestones(None)?;
    if let Some(milestone) = milestones.into_iter().find(|m| m.title == title) {
        return Ok(milestone);
    }

    repo.create_milestone(
        NewMilestone {
            title: title.to_string(),
            description: Some(description.to_string()),
            due_at: due_at.map(|value| value.to_string()),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
        },
        actor,
    )
}

fn ensure_ticket(
    repo: &Repo,
    title: &str,
    summary: &str,
    description: &str,
    tags: &[&str],
    actor: &str,
) -> Result<Ticket> {
    let tickets = repo.list_tickets(None)?;
    if let Some(ticket) = tickets.into_iter().find(|t| t.title == title) {
        return Ok(ticket);
    }
    repo.create_ticket(
        NewTicket {
            title: title.to_string(),
            summary: Some(summary.to_string()),
            description: Some(description.to_string()),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
        },
        actor,
    )
}

fn apply_ticket_patch(
    repo: &Repo,
    id: &tik_core::TicketId,
    actor: &str,
    reason: &str,
    update: impl FnOnce(&mut Ticket),
) -> Result<Ticket> {
    let mut ticket = repo.load_ticket(id)?;
    let before = serde_json::to_string(&ticket)
        .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
    update(&mut ticket);
    let after = serde_json::to_string_pretty(&ticket)
        .map_err(|err| TikError::Schema(format!("serialize ticket: {err}")))?;
    if before == after {
        return Ok(ticket);
    }
    repo.apply_edit(id, &after, actor, Some(reason))
}

fn ensure_status(
    repo: &Repo,
    id: &tik_core::TicketId,
    actor: &str,
    status: TicketStatus,
) -> Result<()> {
    let ticket = repo.load_ticket(id)?;
    if ticket.status == status {
        return Ok(());
    }
    repo.update_status(id, status, actor, Some("seed status"))?;
    Ok(())
}

fn ensure_status_transitions(
    repo: &Repo,
    id: &tik_core::TicketId,
    actor: &str,
    transitions: &[TicketStatus],
) -> Result<()> {
    let events = repo.read_events(id)?;
    if events.iter().any(|event| event.kind == "status_change") {
        return Ok(());
    }
    for status in transitions {
        repo.update_status(id, status.clone(), actor, Some("seed transition"))?;
    }
    Ok(())
}

fn ensure_note(repo: &Repo, id: &tik_core::TicketId, text: &str, actor: &str) -> Result<()> {
    let notes = repo.read_ticket_notes(id)?;
    if notes.contains(text) {
        return Ok(());
    }
    repo.append_note(id, actor, text)?;
    Ok(())
}

fn ensure_notes_edited(repo: &Repo, id: &tik_core::TicketId, actor: &str) -> Result<()> {
    let mut notes = repo.read_ticket_notes(id)?;
    if notes.contains("[seed-edit]") {
        return Ok(());
    }
    notes.push_str("\n\n[seed-edit] Notes edited for fixture coverage.\n");
    repo.write_ticket_notes(id, &notes)?;
    repo.touch_notes(id, actor, Some("seed edit"))?;
    Ok(())
}

fn seed_custom(component: &str, seed: bool) -> Map<String, Value> {
    let mut custom = Map::new();
    custom.insert(
        "component".to_string(),
        Value::String(component.to_string()),
    );
    custom.insert("seed".to_string(), Value::Bool(seed));
    custom
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use tempfile::tempdir;

    fn ticket_tags(ticket: &Ticket) -> HashSet<&str> {
        ticket.tags.iter().map(|tag| tag.as_str()).collect()
    }

    #[test]
    fn run_creates_seed_repo_and_data() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("seed-repo");
        run(SeedArgs { path: root.clone() }).unwrap();

        let repo = Repo::open(&root).unwrap();
        let tickets = repo.list_tickets(None).unwrap();
        assert!(tickets.len() >= 8);

        let bootstrap = tickets
            .iter()
            .find(|ticket| ticket.title == "Bootstrap project")
            .unwrap();
        assert_eq!(bootstrap.status, TicketStatus::Open);
        assert_eq!(bootstrap.kind, TicketType::Feature);
        assert_eq!(bootstrap.priority, Priority::High);
        assert_eq!(
            bootstrap.custom.get("component").and_then(|v| v.as_str()),
            Some("cli")
        );
        assert_eq!(
            bootstrap.custom.get("seed").and_then(|v| v.as_bool()),
            Some(true)
        );
        let tags = ticket_tags(bootstrap);
        assert!(tags.contains("mvp"));
        assert!(tags.contains("cli"));
        assert!(tags.contains("seed"));
        assert_eq!(bootstrap.due_at.as_deref(), Some("2026-02-01T00:00:00Z"));
        assert!(!bootstrap.acceptance.is_empty());
        assert!(bootstrap.relations.iter().any(|rel| rel.kind == RelationType::Blocks));
        assert!(bootstrap
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == ArtifactType::File));

        let tui = tickets
            .iter()
            .find(|ticket| ticket.title == "Add TUI polish")
            .unwrap();
        assert_eq!(tui.status, TicketStatus::InProgress);
        let notes = repo.read_ticket_notes(&tui.id).unwrap();
        assert!(notes.contains("Drafted new layout for ticket list"));
        assert!(notes.contains("[seed-edit]"));

        let events = repo.read_events(&bootstrap.id).unwrap();
        assert!(events.iter().any(|event| event.kind == "status_change"));

        let parent = tickets
            .iter()
            .find(|ticket| ticket.title == "Seed: Parent")
            .unwrap();
        assert!(parent.milestone_id.is_some());

        let milestones = repo.list_milestones(None).unwrap();
        let ga = milestones
            .iter()
            .find(|milestone| milestone.title == "Release 0.1")
            .unwrap();
        assert_eq!(ga.status, MilestoneStatus::Closed);
    }

    #[test]
    fn run_is_idempotent() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("seed-repo");
        run(SeedArgs { path: root.clone() }).unwrap();

        let repo = Repo::open(&root).unwrap();
        let ticket_count = repo.list_tickets(None).unwrap().len();
        let milestone_count = repo.list_milestones(None).unwrap().len();
        let tui = repo
            .list_tickets(None)
            .unwrap()
            .into_iter()
            .find(|ticket| ticket.title == "Add TUI polish")
            .unwrap();
        let notes = repo.read_ticket_notes(&tui.id).unwrap();
        assert_eq!(notes.matches("[seed-edit]").count(), 1);

        run(SeedArgs { path: root }).unwrap();

        let repo = Repo::open(&dir.path().join("seed-repo")).unwrap();
        assert_eq!(ticket_count, repo.list_tickets(None).unwrap().len());
        assert_eq!(milestone_count, repo.list_milestones(None).unwrap().len());
        let tui = repo
            .list_tickets(None)
            .unwrap()
            .into_iter()
            .find(|ticket| ticket.title == "Add TUI polish")
            .unwrap();
        let notes = repo.read_ticket_notes(&tui.id).unwrap();
        assert_eq!(notes.matches("[seed-edit]").count(), 1);
    }
}
