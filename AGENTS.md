# AGENTS.md

This repository builds Tiketer: a production-grade, CLI-first, local-first, AI-first ticketing system. Agents working here must prioritize correctness, determinism, and user trust. No shortcuts.

## Non-negotiables
- Storage lives in `.tik/` with schema-first JSON/JSONL and versioned migrations.
- Append-only history (`notes.jsonl`) is authoritative for auditability.
- Deterministic CLI output modes and stable exit codes.
- Cross-platform reliability (macOS, Linux, Windows).
- No required network access; AI is optional and offline-first.

## Architecture and Boundaries
- `tik-cli` depends on `tik-core` and never touches `.tik/` directly.
- `tik-core` owns all filesystem interactions, validation, and event logging.
- Source-of-truth files are separate from derived index data (`.tik/index/`).
- Public API surface is explicit: `Repo`, domain types, stores, renderers, search, index, AI backend trait.

## Storage Rules
- `ticket.json` is canonical for ticket fields.
- `notes.jsonl` is append-only and always written through `EventStore`.
- `notes.md` is human narrative; it must never be the only source of truth.
- All writes are atomic (temp + fsync + rename). Never write through symlinks.
- Index files are always rebuildable and never edited as a primary source.

## Schema and Versioning
- Each entity carries `schema_version`.
- All reads and writes validate against JSON Schema.
- Schema migrations are explicit, versioned, and reversible where safe.
- SemVer for the CLI; schema compatibility is documented per release.

## CLI UX Contracts
- `--format` always honored; JSON output is deterministic.
- Non-interactive mode never prompts; interactive mode never surprises.
- Long output uses a pager in TTY contexts; `--no-color` always wins.
- Errors are actionable and map to stable exit codes.

## AI Features (Optional)
- AI commands never write without explicit user confirmation.
- Backends are pluggable and local-first; prompt export must always work.
- AI output is structured JSON for agent compatibility.

## Locking and Concurrency
- File locks are mandatory for write operations.
- Lock ordering is deterministic: repo lock, then ticket lock.
- Lock acquisition includes timeouts and clear user messaging.

## Security and Safety
- Treat imports as untrusted; validate and sanitize paths.
- Reject path traversal and avoid following symlinks for writes.
- Explicitly handle invalid UTF-8 or normalize safely.

## Performance Expectations
- Use the index for scale; fallback scans must be deterministic.
- Paginate list and search outputs.
- Avoid O(n) per-ticket operations in hot paths.

## Testing and Quality Gates
- Unit tests for schema validation, parsing, and rendering.
- Integration tests for CLI workflows and file operations.
- Fixtures for corruption and edge cases.
- Cross-platform CI required before release.
- Aim for 100% unit test coverage on implemented features; no new feature ships without tests.
- Run `cargo test` after every feature or bug fix before reporting results.

## Documentation Requirements
- Update `PLAN.md` when product decisions change.
- Keep schema and CLI usage docs aligned with implementation.
- New commands must include examples and output formats.

## Contributor Workflow
### PR Checklist
- Schema changes include version bump, migration plan, and tests.
- CLI changes update help text and examples.
- New commands or flags include output format coverage.
- Edge cases are tested with fixtures and documented.
- `clippy` and `rustfmt` are clean on CI.

### Release Steps
- Update crate versions and `CHANGELOG.md`.
- Validate schema compatibility and run migrations in fixtures.
- Build and verify binaries on macOS, Linux, and Windows.
- Publish checksums and sign release artifacts.

### Module Ownership (Initial)
- `crates/tik-core`: data model, storage, schema, locking.
- `crates/tik-cli`: CLI UX, rendering, command routing.
- `.tik/schema`: schema owners approve all compatibility changes.
