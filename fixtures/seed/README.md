# Seed Fixture

This directory contains a sample Tiketer repo for manual testing.

Usage:
- Run the TUI from the repo root: `cd fixtures/seed && tik`
- Or point commands at it: `tik list --path fixtures/seed`
- Regenerate the fixture: `cargo run -p tik --bin seed_fixture -- --path fixtures/seed`

Notes:
- The data lives in `fixtures/seed/.tik/` and is safe to delete/recreate.
- IDs are pre-generated and not guaranteed to be stable across regenerations.

Coverage:
- Ticket statuses: open, in_progress, blocked, closed, archived.
- Ticket types/priorities/severity: feature/bug/chore/task/spike, low/medium/high/critical.
- Tags, assignees, relations, artifacts, notes (including notes_edited), and milestones (open/closed).
