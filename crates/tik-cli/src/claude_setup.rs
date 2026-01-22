//! Claude Code integration setup utilities.

use std::fs;
use std::io::Write;
use std::path::Path;
use tik_core::{Result, TikError};

pub const CLAUDE_SKILL_CONTENT: &str = r#"---
name: tiketer
description: Manage tickets, milestones, and project tracking using the local tik CLI. Use this skill when working with tickets, bugs, features, tasks, or project management in this repository.
triggers:
  - create a ticket
  - new ticket
  - add a bug
  - track a task
  - list tickets
  - show ticket
  - close ticket
  - ticket management
  - milestone
  - tik
---

# Tiketer CLI Skill

This skill enables AI-assisted ticket and project management using the local `tik` CLI tool.

## Actor Identification

**Always use `--actor "agent:claude"` for all write operations.** This ensures proper attribution in the audit log.

## Output Format

**Always use `--format json` for all commands.** This provides deterministic, machine-readable output.

## Pre-flight Check

Before performing operations, verify the repository is valid:

```bash
tik doctor --format json
```

If this fails with exit code 5, the repository needs initialization:
```bash
tik init
```

## Command Reference

### Create Ticket

```bash
tik new "Title" \
  --tag bug,mvp \
  --type bug \
  --priority high \
  --severity critical \
  --summary "Short summary" \
  --description "Full description" \
  --actor "agent:claude" \
  --format json
```

**Options:**
- `--tag <tags>` - Comma-separated tags
- `--type <type>` - feature, bug, chore, task, spike
- `--priority <p>` - low, medium, high, critical
- `--severity <s>` - low, normal, high, critical
- `--summary <text>` - Short summary (defaults to title)
- `--description <text>` - Full description

**Returns:** Full ticket JSON with generated ID

### List Tickets

```bash
tik list --status open --format json
tik list --tag mvp --assignee alice --format json
tik list --all --format json  # No pagination
```

**Filters:** `--status`, `--tag`, `--assignee`, `--type`, `--priority`, `--milestone`

### Search Tickets

```bash
tik search "status:open tag:mvp authentication" --format json
```

**Query syntax:**
- `status:open` - Filter by status
- `tag:mvp` - Filter by tag
- `assignee:alice` - Filter by assignee
- `type:bug` - Filter by type
- `priority:high` - Filter by priority
- `created:>=2026-01-01` - Date filters
- Free text searches title/description

### Show Ticket

```bash
tik show T-01ARZ3NDEKTSV4RRFFQ69G5FAV --format json
```

### Add Note

```bash
tik note T-01ARZ3NDEKTSV4RRFFQ69G5FAV "Progress update or comment" \
  --actor "agent:claude" --format json
```

### Close Ticket

```bash
tik close T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --reason "Completed implementation" \
  --actor "agent:claude" --format json
```

### Reopen Ticket

```bash
tik reopen T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --actor "agent:claude" --format json
```

### View Event Log

```bash
tik log T-01ARZ3NDEKTSV4RRFFQ69G5FAV --format json
```

Shows complete audit trail including actor attribution.

### Relations

```bash
# Add relation
tik relate T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --relation blocks \
  --target T-01ANOTHERTICKETID \
  --actor "agent:claude" --format json

# Remove relation
tik unrelate T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --relation blocks \
  --target T-01ANOTHERTICKETID \
  --actor "agent:claude" --format json
```

**Relation types:** blocks, blocked_by, depends_on, duplicate, parent, child

### Artifact Links

```bash
# Link file or URL
tik link T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --type file \
  --ref "src/auth/login.rs" \
  --actor "agent:claude" --format json

# Remove link
tik unlink T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --type file \
  --ref "src/auth/login.rs" \
  --actor "agent:claude" --format json
```

**Artifact types:** file, url, commit, pr

### Tags

```bash
tik tag T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --add mvp,security \
  --actor "agent:claude" --format json

tik tag T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --remove wontfix \
  --actor "agent:claude" --format json
```

### Assignees

```bash
tik assign T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --add alice,bob \
  --actor "agent:claude" --format json

tik assign T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --remove alice \
  --actor "agent:claude" --format json
```

### Milestones

```bash
# Create milestone
tik milestone new "Phase 1" \
  --tag mvp \
  --due-at 2026-02-01T00:00:00Z \
  --actor "agent:claude" --format json

# List milestones
tik milestone list --status open --format json

# Show milestone
tik milestone show M-01ARZ3NDEKTSV4RRFFQ69G5FAV --format json

# Assign ticket to milestone
tik milestone set T-01ARZ3NDEKTSV4RRFFQ69G5FAV M-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --actor "agent:claude" --format json

# Close milestone
tik milestone close M-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --reason "Shipped to production" \
  --actor "agent:claude" --format json
```

## Exit Codes

| Code | Meaning | Action |
|------|---------|--------|
| 0 | Success | Parse JSON output |
| 2 | Usage error | Check command syntax |
| 4 | Not found | Ticket/milestone doesn't exist |
| 5 | Repo invalid | Run `tik init` or check `.tik/` |
| 6 | Lock contention | Retry after brief delay |

## Common Workflows

### Bug Report Workflow

1. Create ticket with bug type and relevant tags
2. Link related code files as artifacts
3. Set appropriate priority/severity
4. Add to milestone if applicable

```bash
tik new "Login fails with special characters" \
  --type bug --priority high --severity high \
  --tag auth,security \
  --summary "Users cannot login when password contains special chars" \
  --actor "agent:claude" --format json
```

### Feature Implementation Workflow

1. Check for existing tickets: `tik search "feature-name" --format json`
2. Create ticket if not exists
3. Add notes as implementation progresses
4. Link PRs/commits as artifacts
5. Close with completion reason

### Linking Code to Tickets

After implementing a fix or feature:

```bash
# Link the implementation file
tik link T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  --type file --ref "src/auth/password.rs" \
  --actor "agent:claude" --format json

# Add implementation note
tik note T-01ARZ3NDEKTSV4RRFFQ69G5FAV \
  "Implemented password validation with special character support" \
  --actor "agent:claude" --format json
```

## Error Handling

- **Exit code 5 (repo invalid):** Check if `.tik/` exists, run `tik init` if needed
- **Exit code 4 (not found):** Verify ticket/milestone ID is correct
- **Exit code 6 (lock contention):** Another process is writing; retry after delay
- **Exit code 2 (usage error):** Check command syntax and required arguments

## Data Locations

- Repository data: `.tik/` directory
- Tickets: `.tik/projects/<project>/tickets/T-<ulid>/`
- Milestones: `.tik/projects/<project>/milestones/`
- Index: `.tik/projects/<project>/index/`

**Never modify these files directly.** Always use CLI commands.
"#;

pub const CLAUDE_MD_CONTENT: &str = r#"# Tiketer Quick Reference

Quick reference for Claude and other AI agents working with tik ticket management.

## Actor Identification

For all write operations, identify yourself as an agent:

```bash
--actor "agent:claude"
```

This records your actions in the audit log with proper attribution.

## Output Format

Always use JSON output for machine-readable responses:

```bash
--format json
```

Available formats: `json`, `jsonl`, `yaml`, `table`, `compact`, `md`, `csv`

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Internal error |
| 2 | Usage error (invalid arguments) |
| 3 | Schema validation error |
| 4 | Not found (ticket, milestone, artifact) |
| 5 | Repository not initialized or invalid |
| 6 | Lock contention (busy) |
| 7 | Conflict detected |
| 8 | IO error |
| 9 | Permission denied |
| 10 | Configuration error |

## Repository Check

Before operations, verify the repository is valid:

```bash
tik doctor --format json
```

Check for `.tik/` directory existence before running commands.

## Common Operations

### Create a ticket
```bash
tik new "Title here" --tag bug --actor "agent:claude" --format json
```

### List tickets
```bash
tik list --status open --format json
tik search "status:open tag:mvp" --format json
```

### Show ticket details
```bash
tik show T-<ulid> --format json
```

### Add a note
```bash
tik note T-<ulid> "Note text" --actor "agent:claude" --format json
```

### Close a ticket
```bash
tik close T-<ulid> --reason "completed" --actor "agent:claude" --format json
```

### Reopen a ticket
```bash
tik reopen T-<ulid> --actor "agent:claude" --format json
```

### View event log
```bash
tik log T-<ulid> --format json
```

### Milestones
```bash
tik milestone list --format json
tik milestone new "Phase 1" --actor "agent:claude" --format json
tik milestone set T-<ulid> M-<ulid> --actor "agent:claude" --format json
```

### Relations and links
```bash
tik relate T-<ulid> --relation blocks --target T-<other> --actor "agent:claude" --format json
tik link T-<ulid> --type file --ref "path/to/file" --actor "agent:claude" --format json
```

### Tags and assignees
```bash
tik tag T-<ulid> --add mvp,priority --actor "agent:claude" --format json
tik assign T-<ulid> --add alice --actor "agent:claude" --format json
```

## Key Constraints

1. **Never touch `.tik/` directly** - always use CLI commands
2. **Use `tik doctor`** to check repository health before bulk operations
3. **JSON output is deterministic** - safe for parsing
4. **File locks are automatic** - retry on exit code 6 (lock contention)
5. **Ticket IDs are ULIDs** - format `T-<26 characters>`
6. **Milestone IDs are ULIDs** - format `M-<26 characters>`

## JSON Output Structure

### Ticket
```json
{
  "schema_version": "1.0",
  "id": "T-01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "title": "Ticket title",
  "status": "open|in_progress|blocked|closed|archived",
  "type": "feature|bug|chore|task|spike",
  "priority": "low|medium|high|critical",
  "severity": "low|normal|high|critical",
  "assignees": ["alice", "bob"],
  "milestone_id": "M-01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "tags": ["mvp", "backend"],
  "created_at": "2026-01-01T00:00:00Z",
  "updated_at": "2026-01-01T00:00:00Z",
  "closed_at": null,
  "summary": "Short summary",
  "description": "Full description",
  "acceptance": ["criterion 1", "criterion 2"],
  "estimate": {"value": 3, "unit": "points"},
  "due_at": "2026-02-01T00:00:00Z",
  "relations": [{"type": "blocks", "id": "T-..."}],
  "artifacts": [{"type": "file", "ref": "path/to/file"}],
  "custom": {}
}
```

### Milestone
```json
{
  "schema_version": "1.0",
  "id": "M-01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "title": "Milestone title",
  "status": "open|closed",
  "description": "Description",
  "tags": ["mvp"],
  "created_at": "2026-01-01T00:00:00Z",
  "updated_at": "2026-01-01T00:00:00Z",
  "due_at": "2026-02-01T00:00:00Z",
  "closed_at": null
}
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `TIK_ACTOR` | Default actor for operations (overridden by `--actor`) |

## Full Documentation

See `.claude/skills/tiketer.md` for the complete skill reference.
"#;

const CLAUDE_MD_APPEND_SECTION: &str = r#"

## Ticket Management (tik CLI)

Use `tik` for local ticket/task tracking with proper agent attribution:

```bash
# Create a ticket
tik new "Bug: login fails" --type bug --tag auth --actor "agent:claude" --format json

# List open tickets
tik list --status open --format json

# Add a note
tik note T-<id> "Progress update" --actor "agent:claude" --format json

# Close when done
tik close T-<id> --reason "Fixed" --actor "agent:claude" --format json
```

Key points:
- Always use `--actor "agent:claude"` for attribution
- Always use `--format json` for machine-readable output
- Run `tik doctor --format json` to verify repository health
- See `.claude/skills/tiketer.md` for full command reference
"#;

/// AGENTS.md content for Codex CLI and other AI agents.
pub const AGENTS_MD_CONTENT: &str = r#"# AGENTS.md

Instructions for AI agents working in this repository.

## Ticket Management (tik CLI)

This repository uses `tik` for local ticket/task tracking. Use JSON output and proper agent attribution.

### Actor Identification

For all write operations, identify yourself as an agent:

```bash
--actor "agent:codex"
```

This records your actions in the audit log with proper attribution.

### Output Format

Always use JSON output for machine-readable responses:

```bash
--format json
```

Available formats: `json`, `jsonl`, `yaml`, `table`, `compact`, `md`, `csv`

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Internal error |
| 2 | Usage error (invalid arguments) |
| 3 | Schema validation error |
| 4 | Not found (ticket, milestone, artifact) |
| 5 | Repository not initialized or invalid |
| 6 | Lock contention (busy) |
| 7 | Conflict detected |
| 8 | IO error |
| 9 | Permission denied |
| 10 | Configuration error |

### Repository Check

Before operations, verify the repository is valid:

```bash
tik doctor --format json
```

If this fails with exit code 5, initialize with `tik init`.

### Common Operations

#### Create a ticket
```bash
tik new "Title here" --tag bug --actor "agent:codex" --format json
```

#### List tickets
```bash
tik list --status open --format json
tik search "status:open tag:mvp" --format json
```

#### Show ticket details
```bash
tik show T-<ulid> --format json
```

#### Add a note
```bash
tik note T-<ulid> "Note text" --actor "agent:codex" --format json
```

#### Close a ticket
```bash
tik close T-<ulid> --reason "completed" --actor "agent:codex" --format json
```

#### Reopen a ticket
```bash
tik reopen T-<ulid> --actor "agent:codex" --format json
```

#### View event log
```bash
tik log T-<ulid> --format json
```

#### Milestones
```bash
tik milestone list --format json
tik milestone new "Phase 1" --actor "agent:codex" --format json
tik milestone set T-<ulid> M-<ulid> --actor "agent:codex" --format json
```

#### Relations and links
```bash
tik relate T-<ulid> --relation blocks --target T-<other> --actor "agent:codex" --format json
tik link T-<ulid> --type file --ref "path/to/file" --actor "agent:codex" --format json
```

#### Tags and assignees
```bash
tik tag T-<ulid> --add mvp,priority --actor "agent:codex" --format json
tik assign T-<ulid> --add alice --actor "agent:codex" --format json
```

### Key Constraints

1. **Never touch `.tik/` directly** - always use CLI commands
2. **Use `tik doctor`** to check repository health before bulk operations
3. **JSON output is deterministic** - safe for parsing
4. **File locks are automatic** - retry on exit code 6 (lock contention)
5. **Ticket IDs are ULIDs** - format `T-<26 characters>`
6. **Milestone IDs are ULIDs** - format `M-<26 characters>`

### JSON Output Structure

#### Ticket
```json
{
  "schema_version": "1.0",
  "id": "T-01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "title": "Ticket title",
  "status": "open|in_progress|blocked|closed|archived",
  "type": "feature|bug|chore|task|spike",
  "priority": "low|medium|high|critical",
  "severity": "low|normal|high|critical",
  "assignees": ["alice", "bob"],
  "milestone_id": "M-01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "tags": ["mvp", "backend"],
  "created_at": "2026-01-01T00:00:00Z",
  "updated_at": "2026-01-01T00:00:00Z"
}
```

#### Milestone
```json
{
  "schema_version": "1.0",
  "id": "M-01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "title": "Milestone title",
  "status": "open|closed",
  "created_at": "2026-01-01T00:00:00Z",
  "due_at": "2026-02-01T00:00:00Z"
}
```
"#;

const AGENTS_MD_APPEND_SECTION: &str = r#"

## Ticket Management (tik CLI)

Use `tik` for local ticket/task tracking with proper agent attribution:

```bash
# Create a ticket
tik new "Bug: login fails" --type bug --tag auth --actor "agent:codex" --format json

# List open tickets
tik list --status open --format json

# Add a note
tik note T-<id> "Progress update" --actor "agent:codex" --format json

# Close when done
tik close T-<id> --reason "Fixed" --actor "agent:codex" --format json
```

Key points:
- Always use `--actor "agent:codex"` for attribution
- Always use `--format json` for machine-readable output
- Run `tik doctor --format json` to verify repository health
"#;

/// Result of AI setup operation.
#[derive(Debug, Default)]
pub struct ClaudeSetupResult {
    pub created_files: Vec<String>,
    pub updated_files: Vec<String>,
}

impl ClaudeSetupResult {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.created_files.is_empty() {
            parts.push(format!("Created: {}", self.created_files.join(", ")));
        }
        if !self.updated_files.is_empty() {
            parts.push(format!("Updated: {}", self.updated_files.join(", ")));
        }
        if parts.is_empty() {
            "AI integration already configured".to_string()
        } else {
            parts.join(". ")
        }
    }
}

/// Set up AI integration files with options.
///
/// - `setup_claude`: Create CLAUDE.md and .claude/skills/tiketer.md
/// - `setup_agents`: Create AGENTS.md (for Codex CLI)
pub fn setup_ai_integration(
    root: &Path,
    setup_claude: bool,
    setup_agents: bool,
) -> Result<ClaudeSetupResult> {
    let mut result = ClaudeSetupResult::default();

    if setup_claude {
        // Create .claude/skills directory
        let skills_dir = root.join(".claude").join("skills");
        fs::create_dir_all(&skills_dir)
            .map_err(|e| TikError::io("failed to create .claude/skills", e))?;

        // Write tiketer.md skill file
        let skill_path = skills_dir.join("tiketer.md");
        if skill_path.exists() {
            fs::write(&skill_path, CLAUDE_SKILL_CONTENT)
                .map_err(|e| TikError::io("failed to write skill file", e))?;
            result.updated_files.push(".claude/skills/tiketer.md".to_string());
        } else {
            fs::write(&skill_path, CLAUDE_SKILL_CONTENT)
                .map_err(|e| TikError::io("failed to write skill file", e))?;
            result.created_files.push(".claude/skills/tiketer.md".to_string());
        }

        // Handle CLAUDE.md
        let claude_md_path = root.join("CLAUDE.md");
        if claude_md_path.exists() {
            let existing = fs::read_to_string(&claude_md_path)
                .map_err(|e| TikError::io("failed to read CLAUDE.md", e))?;

            if !existing.contains("## Tiketer") && !existing.contains("tik new") {
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .open(&claude_md_path)
                    .map_err(|e| TikError::io("failed to open CLAUDE.md", e))?;

                file.write_all(CLAUDE_MD_APPEND_SECTION.as_bytes())
                    .map_err(|e| TikError::io("failed to append to CLAUDE.md", e))?;
                result.updated_files.push("CLAUDE.md".to_string());
            }
        } else {
            fs::write(&claude_md_path, CLAUDE_MD_CONTENT)
                .map_err(|e| TikError::io("failed to write CLAUDE.md", e))?;
            result.created_files.push("CLAUDE.md".to_string());
        }
    }

    if setup_agents {
        // Handle AGENTS.md (for Codex CLI and other agents)
        let agents_md_path = root.join("AGENTS.md");
        if agents_md_path.exists() {
            let existing = fs::read_to_string(&agents_md_path)
                .map_err(|e| TikError::io("failed to read AGENTS.md", e))?;

            if !existing.contains("## Ticket Management (tik CLI)")
                && !existing.contains("tik new")
                && !existing.contains("agent:codex")
            {
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .open(&agents_md_path)
                    .map_err(|e| TikError::io("failed to open AGENTS.md", e))?;

                file.write_all(AGENTS_MD_APPEND_SECTION.as_bytes())
                    .map_err(|e| TikError::io("failed to append to AGENTS.md", e))?;
                result.updated_files.push("AGENTS.md".to_string());
            }
        } else {
            fs::write(&agents_md_path, AGENTS_MD_CONTENT)
                .map_err(|e| TikError::io("failed to write AGENTS.md", e))?;
            result.created_files.push("AGENTS.md".to_string());
        }
    }

    Ok(result)
}

/// Set up all AI integration files (Claude skill, CLAUDE.md, AGENTS.md).
/// Convenience wrapper that enables all integrations.
pub fn setup_claude_integration(root: &Path) -> Result<ClaudeSetupResult> {
    setup_ai_integration(root, true, true)
}
