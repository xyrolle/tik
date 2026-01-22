# CLAUDE.md

Quick reference for Claude and other AI agents working with tiketer-ai.

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

## Further Documentation

- `USAGE.md` - Full command reference
- `AGENTS.md` - Architecture and contributor guidelines
