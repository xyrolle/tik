# PLAN: Tiketer (production-grade CLI-first, local-first, AI-first ticketing)

## 1) Vision and Non-negotiables
Build an open-source, CLI-first ticketing system that stores all data locally in `.tik/` and is optimized for both humans and AI agents. It must be production-ready, deterministic, schema-driven, and fully traceable without relying on any online service.

Non-negotiables:
- Local-first by default, optional git sync, no required daemon.
- Agent-friendly schemas and deterministic CLI output modes.
- Human-friendly editing with Markdown alongside structured JSON/JSONL.
- Full traceability with append-only history and explicit actors.
- Cross-platform stability (macOS, Linux, Windows) and safe concurrency.

## 2) Success Criteria (Production Ready)
- 100 percent CLI coverage for core workflows (create, update, search, close, audit).
- All data validated against versioned JSON schemas; migrations supported.
- Atomic writes, file locking, crash recovery, and integrity checks built in.
- Multiple display modes for humans and agents, with stable machine output.
- Search at scale with a rebuildable index and deterministic fallbacks.
- Comprehensive docs, tests, and examples for all commands and formats.

## 3) User-Centered Journeys
### Solo Builder
- `tik init`, `tik new "Feature X"`, `tik note`, `tik close`.
- Fast filtering (`tik list --status open --tag mvp`).

### Agent-Assisted Developer
- `tik export --json` for prompt input.
- `tik ai summarize`, `tik ai plan` with reviewable output.

### Freelancer or Consultant
- Local project tickets without SaaS.
- Optional git sync and export to clients.

## 4) Data Model (Schema-First)
### Core Entities
- Ticket: primary unit of work.
- Milestone: grouped delivery or phase.
- Note/Event: append-only history log.
- Artifact: files, commits, URLs, or references.
- Relation: links between tickets (blocks, depends_on, duplicates, parent/child).
- Actor: human, agent, or system identity.

### Ticket Schema (authoritative JSON)
```json
{
  "schema_version": "1.0",
  "id": "T-01JH1C5QABZ7F72G4H9V8G0K0P",
  "title": "Add CLI init command",
  "status": "open",
  "type": "feature",
  "priority": "medium",
  "severity": "normal",
  "assignees": ["me"],
  "milestone_id": "M-01JH1C5S1F2YDP6P4B9W2J2ZB8",
  "tags": ["cli", "mvp"],
  "created_at": "2026-01-01T10:00:00Z",
  "updated_at": "2026-01-01T10:00:00Z",
  "closed_at": null,
  "summary": "Initialize project scaffolding from CLI",
  "description": "## Context\nWhy we need this\n\n## Requirements\n- Creates .tik/\n- Writes config\n",
  "acceptance": ["Creates .tik/", "Writes config"],
  "estimate": {"value": 3, "unit": "days"},
  "due_at": null,
  "relations": [{"type": "blocks", "id": "T-01JH1C5W8D7YQK9VQH1C0N7T2A"}],
  "artifacts": [{"type": "file", "ref": "artifacts/design.md"}],
  "custom": {}
}
```

### Note/Event Schema (append-only JSONL)
```json
{"event_id":"E-01JH1C5Z3NVQK8YQ6N7B1A1T2B","ts":"2026-01-01T10:05:00Z","actor":"human","type":"note","data":{"text":"Scaffold config format options"}}
{"event_id":"E-01JH1C62QG8J3M1W9J1S2C2D3E","ts":"2026-01-01T10:06:00Z","actor":"agent","type":"analysis","data":{"text":"JSON is easiest to parse; TOML is easier for humans"}}
{"event_id":"E-01JH1C66B6S6A3K4P8R9T0U1V2","ts":"2026-01-01T10:10:00Z","actor":"human","type":"status_change","data":{"from":"open","to":"in_progress","reason":"starting implementation"}}
```

### ID Strategy
- Canonical IDs are ULIDs with prefixes (`T-`, `M-`, `E-`) for uniqueness and sort order.
- CLI accepts short prefixes for convenience (`T-01JH1C5...`).
- File paths are derived from IDs to avoid rename issues.

## 5) Storage Layout (Required)
```
.tik/
  repo.json
  config.json
  schema/
    ticket.schema.json
    milestone.schema.json
    event.schema.json
    config.schema.json
  tickets/
    T-01JH1C5QABZ7F72G4H9V8G0K0P/
      ticket.json
      notes.jsonl
      notes.md
      artifacts/
        design.md
  milestones/
    M-01JH1C5S1F2YDP6P4B9W2J2ZB8.json
  index/
    tickets.jsonl
    fts.sqlite
  locks/
  tmp/
```

Rules:
- `ticket.json` is authoritative for fields.
- `notes.jsonl` is append-only and canonical for history.
- `notes.md` is human narrative, generated or edited with CLI.
- `index/` is derived and rebuildable; never treated as source of truth.
- `locks/` and `tmp/` are internal for safety and atomic writes.

## 6) Versioning and Migration
- CLI versioning follows SemVer.
- Schemas are versioned with `schema_version` in each file.
- `tik migrate` upgrades repo data; reversible where possible.
- `tik doctor` validates repo integrity and suggests fixes.

## 7) Technical Approach (Rust-first)
Rust is the preferred implementation language because it delivers fast, safe, cross-platform CLI binaries with strong filesystem primitives and deterministic performance. It is ideal for a local-first tool that must be reliable under heavy file I/O and large repositories.

Core architectural choices:
- Single binary CLI with a shared core library for storage, indexing, and rendering.
- Schema-first validation using JSON Schema and strict serialization with `serde`.
- Atomic write strategy using temp files + fsync + rename.
- File locking for concurrent CLI runs and external editors.
- Clear separation between source-of-truth files and derived index data.

Crates and components (representative, not exhaustive):
- CLI: `clap` for command parsing, `clap_complete` for shell completions.
- Serialization: `serde`, `serde_json`, `serde_yaml`, `csv`.
- Config: `toml`.
- Time and IDs: `time` or `chrono`, `ulid`.
- Validation: `jsonschema` or equivalent crate with strict modes.
- SQLite FTS: `rusqlite` with FTS5 (bundled or system).
- Tables and rendering: `tabled` or `comfy-table`.
- Errors: `thiserror` + `anyhow` for structured errors and context.
- TTY UX: `dialoguer`, `console`, `is-terminal`.
- Paging: `pager` for long output with `$PAGER` integration.
- Versions and edits: `semver`, `toml_edit`.

AI integration approach:
- Offline-first: AI features are optional and never write without confirmation.
- Backend adapters are explicit and pluggable: local LLM via stdio, or user-defined command hooks.
- Prompt export (`tik ai prompt`) is always available, even without an AI backend.

Packaging and distribution:
- Single static binary per platform where possible.
- Deterministic builds, reproducible release artifacts, and signed checksums.

### 7.1) Module Diagram and Workspace Layout
```
tik-cli (bin)
  └── depends on tik-core (lib)
      ├── domain (Ticket, Milestone, Event, IDs)
      ├── schema (JSON schema loading + validation)
      ├── storage (repo layout, atomic reads/writes)
      ├── index (FTS rebuild + query)
      ├── search (query parsing + filters)
      ├── render (table/json/jsonl/yaml/md/timeline/graph)
      ├── interop (import/export adapters)
      ├── ai (prompt bundles + backend trait)
      ├── locking (repo + ticket locks)
      └── config (repo.json + config.json)
```

### 7.2) Public API Boundaries
Stable public API surface (in `tik-core`):
- `Repo` (open repo, validates structure, owns locks)
- Domain types: `Ticket`, `Milestone`, `Event`, `Artifact`, `Relation`, `Actor`
- ID types: `TicketId`, `MilestoneId`, `EventId`
- Services: `TicketStore`, `EventStore`, `MilestoneStore`
- Traits: `Index`, `Renderer`, `AiBackend`
- Types: `SearchQuery`, `OutputFormat`, `ExportFormat`
- Errors: `TikError` with stable error codes for CLI exit mapping

Internal-only modules:
- Raw filesystem paths, temp files, and low-level locks.
- SQLite internals and SQL strings.
- Schema compilation details (only validators are exposed).

CLI constraints:
- `tik-cli` never accesses `.tik/` directly; it calls `tik-core`.
- Output formatting is always routed through `Renderer` for determinism.
- All writes go through `Repo` to guarantee atomicity and event logging.

### 7.3) Dependency Versions (Pinned)
Pinned versions are set at project start and updated intentionally via release cadence.

```toml
[dependencies]
clap = "4.5.4"
clap_complete = "4.5.2"
serde = "1.0.197"
serde_json = "1.0.115"
serde_yaml = "0.9.34"
toml = "0.8.12"
toml_edit = "0.22.12"
csv = "1.3.0"
time = { version = "0.3.36", features = ["serde", "formatting", "parsing"] }
ulid = { version = "1.1.2", features = ["serde"] }
jsonschema = "0.17.1"
rusqlite = { version = "0.31.0", features = ["bundled-full", "modern_sqlite"] }
thiserror = "1.0.58"
anyhow = "1.0.82"
tabled = "0.15.0"
tempfile = "3.10.1"
fs4 = "0.9.1"
walkdir = "2.5.0"
ignore = "0.4.22"
tracing = "0.1.40"
tracing-subscriber = "0.3.18"
dialoguer = "0.11.0"
console = "0.15.8"
is-terminal = "0.4.12"
pager = "0.16.1"
semver = "1.0.22"
```

### 7.4) FTS5 Strategy (SQLite)
- Use `rusqlite` with `bundled-full` to ensure FTS5 availability across platforms.
- Store `fts.sqlite` under `.tik/index/` and treat it as derived data.
- Provide `tik index rebuild` to regenerate index from canonical files.
- Optional `system-sqlite` feature for packagers who prefer OS SQLite builds.

### 7.5) Windows Locking Details
- Use `fs4::FileExt` to provide shared and exclusive locks.
- Repository-level lock: `.tik/locks/repo.lock` for write operations.
- Ticket-level locks: `.tik/locks/<ticket_id>.lock` for isolated edits.
- Lock ordering is enforced: repo lock first, then ticket lock, to avoid deadlocks.
- Locks are held by keeping the lock file handle open for the full operation.
- On Windows, `fs4` uses `LockFileEx`; no polling loops, no hacks.
- Lock acquisition must support timeouts with clear UX messaging on contention.

### 7.6) UX and Interaction Design (CLI-first)
Interaction philosophy:
- Zero-friction defaults, with an escape hatch for structured detail.
- Predictable behavior across interactive and non-interactive environments.
- Explicit, explainable failures with precise remediation steps.

Interaction modes:
- Non-interactive: strict flags, no prompts, stable exit codes for scripting.
- Interactive: guided prompts, previews, and confirmation gates.
- Editor-first: `$EDITOR` integration for ticket/notes; validation on save.
- Pager-first: long outputs go to a pager in TTY contexts.
- Default entry: `tik` launches the TUI; `--non-interactive` or explicit subcommands for scripts.

Primary UX flows:
- Create: `tik new "Title"` or `tik new --interactive`, then `tik note`.
- Review: `tik show`, `tik timeline`, `tik diff`, `tik history`.
- Plan: `tik relate`, `tik milestone set`, `tik graph`.
- Search: `tik list` with filters, `tik search` with query language.
- Export: `tik export --format md|json|jsonl`, optional redaction.

Display and output rules:
- TTY detection controls color and paging; `--no-color` overrides.
- `--format` is always honored; JSON outputs are deterministic and stable.
- Error output is structured with codes for scripts and human-readable messages.

Accessibility and usability:
- Clear, concise prompts with defaults shown.
- Consistent wording for statuses, priorities, and actions.
- No hidden state; all changes visible via `tik log` and `tik diff`.

### 7.7) Exit Codes (Stable Contract)
Exit codes are stable and documented for automation and CI scripting.

```text
0  success
1  internal error (unexpected)
2  usage error (invalid arguments)
3  schema validation error
4  not found (ticket, milestone, artifact)
5  repo not initialized or invalid
6  lock contention (busy)
7  conflict detected (merge markers or unresolved state)
8  IO error (read/write failure)
9  permission denied
10 config error
11 index error (FTS failure)
12 import/export error
13 external command failed ($EDITOR, pager)
14 AI backend error or unavailable
15 interrupted (SIGINT)
```

### 7.8) Technology Guidelines and Best Practices
The following rules are non-negotiable and guide all implementation decisions.

Rust standards:
- Rust edition is pinned; MSRV is defined and enforced.
- `rustfmt` and `clippy` are required in CI; warnings are denied on release builds.
- `thiserror` for typed errors; `anyhow` only at CLI boundaries.

I/O safety and data integrity:
- All writes are atomic with fsync and rename semantics.
- No writes to symlinks; paths are sanitized and normalized.
- Never mutate derived data without recomputing from source-of-truth files.

Concurrency and locking:
- Lock ordering is deterministic (repo lock before ticket lock).
- Lock acquisition timeouts are required with clear user messaging.
- Shared vs exclusive locks are explicit in APIs.

Performance and scaling:
- Index is preferred for large repos; scan fallback must be deterministic.
- Pagination is required for list and search outputs.
- Avoid full ticket scans in hot paths; cache derived values where safe.

Security and untrusted input:
- Import content is treated as untrusted and strictly validated.
- Reject invalid UTF-8 or normalize safely when reading files.
- Enforce path traversal protection for artifacts and imports.

UX consistency:
- TTY rules are consistent across commands; `--no-color` and `--format` always win.
- Non-interactive mode never prompts; interactive mode never surprises.
- Error messages include actionable remediation.

## 8) CLI Commands (Complete, No Gaps)
### Repo and Config
- `tik init`, `tik status`, `tik config get/set`, `tik doctor`, `tik migrate`, `tik backup`, `tik restore`, `tik index rebuild`, `tik completion`.

### Tickets
- `tik new`, `tik list`, `tik show`, `tik edit`, `tik set`, `tik assign`, `tik tag`, `tik relate`, `tik link`, `tik unlink`, `tik close`, `tik reopen`, `tik archive`, `tik delete` (soft delete only).

### Notes and History
- `tik note`, `tik log`, `tik timeline`, `tik diff`, `tik history`.

### Milestones
- `tik milestone new/list/show/set/close`.

### Search and Reporting
- `tik search`, `tik report`, `tik stats`, `tik graph`.

### Import/Export
- `tik export --format json|jsonl|yaml|csv|md`
- `tik import --format json|csv|md`

### AI
- `tik ai summarize`, `tik ai plan`, `tik ai next`, `tik ai refine`, `tik ai prompt`.

Every command supports:
- `--format` output, `--no-color`, `--json` alias, `--quiet`, and `--dry-run`.
- Deterministic output for agents and stable exit codes for scripting.

## 9) Display Modes (Multiple, Stable)
- `table`: default human view, width-aware, color optional.
- `compact`: minimal single-line view for lists.
- `json`: strict JSON with stable field order.
- `jsonl`: streaming-friendly JSONL for pipelines.
- `yaml`: human-friendly config exports.
- `md`: Markdown for docs or client sharing.
- `timeline`: chronological view from notes.jsonl.
- `graph`: Mermaid graph output for relations.
- `csv`: spreadsheet export for audits.
- `template`: user-defined format string (no hacks, strict schema).

## 10) Search and Indexing
- Full-text search across titles, summaries, tags, and notes.
- Indexed search via rebuildable SQLite FTS in `.tik/index/`.
- Fallback scan mode when index is missing or invalid.
- Query language supports boolean ops, status filters, date ranges, and tags.

## 11) Traceability and Auditability
- All changes generate events in `notes.jsonl`.
- Explicit actor (`human`, `agent`, `system`) for accountability.
- Status, assignment, relation, and metadata changes are recorded as events.
- `tik diff` shows field-level changes between revisions.

## 12) Usability and Safety
- Interactive prompts with escape hatches (`--yes`, `--no`).
- `$EDITOR` integration for ticket and notes editing.
- Clear error messages with actionable remediation.
- Atomic writes with temp files and rename semantics.
- File locks to avoid concurrent write corruption.

## 13) Reliability and Edge Cases (Handled)
- Partial writes: always atomic writes, verified checksum before commit.
- Merge conflicts: detect conflict markers, `tik doctor` offers repair.
- Clock skew: event ordering via ULID and timestamps in UTC.
- Broken references: artifacts and relations validated and reported.
- Massive repos: index rebuilds and pagination to keep performance stable.
- Corrupted schema: strict validation with auto-repair only when safe.
- Accidental deletes: soft delete with recoverable archive state.

## 14) Security and Privacy
- No network usage by default; explicit opt-in for AI backends.
- Optional redaction of sensitive notes before export.
- Clear separation between local files and external references.

## 15) Interoperability
- Stable JSON/JSONL schemas published with version guarantees.
- Import/export adapters for common formats (CSV, Markdown, JSON).
- Optional adapters for GitHub or Jira via explicit user opt-in.

## 16) AI-First Features (Optional, Local-First)
- `tik ai summarize`: produces summary + key decisions + open questions.
- `tik ai plan`: drafts actionable steps in structured JSON for review.
- `tik ai next`: suggests next tickets based on dependencies.
- `tik ai refine`: validates schema, fills missing fields, never writes without confirmation.
- `tik ai prompt`: exports a structured prompt bundle for local LLMs.

## 17) Quality Gates
- Unit tests for schema validation, parsing, and rendering.
- Integration tests for CLI commands and file operations.
- Fixtures for corrupted and edge-case repos.
- Cross-platform CI for filesystem semantics.

## 18) Delivery Plan (All Features, No Compromises)
Phase 1: Core schemas, storage layout, CLI scaffolding, and validation.
Phase 2: Full ticket, milestone, and note workflows with traceability.
Phase 3: Search, indexing, display modes, and export/import.
Phase 4: AI commands, prompt export, and usability refinements.
Phase 5: Hardening, docs, fixtures, and long-term maintenance.

## 19) Decisions Locked In
- Storage root is `.tik/`.
- Canonical IDs are ULIDs with prefixes.
- JSON is authoritative; Markdown is for human-readable narrative.
- Append-only event log is required for traceability.
