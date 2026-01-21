# tik Usage Guide

Complete guide for building, installing, and running tik.

## Prerequisites

- **Rust**: Version 1.78 or later
- **Cargo**: Comes with Rust installation

To install Rust, visit [rustup.rs](https://rustup.rs/) or run:
```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify your Rust version:
```sh
rustc --version  # Should be 1.78.0 or higher
```

## Building

### Debug Build
```sh
cargo build
```
The binary is created at `./target/debug/tik`.

### Release Build (Recommended for Production)
```sh
cargo build --release
```
The optimized binary is created at `./target/release/tik`.

### Build Specific Crates
```sh
cargo build -p tik        # CLI binary only
cargo build -p tik-core   # Core library only
```

## Installation

### Option 1: Install Locally via Cargo
```sh
cargo install --path crates/tik-cli
```
This installs `tik` to `~/.cargo/bin/` (ensure it's in your PATH).

### Option 2: Manual Installation
```sh
cargo build --release
cp ./target/release/tik /usr/local/bin/  # or another directory in PATH
```

### Verify Installation
```sh
tik --version
tik --help
```

## Running

### Development Mode (via Cargo)
```sh
cargo run -p tik -- <command> [options]
```

Examples:
```sh
cargo run -p tik -- init
cargo run -p tik -- new "My first ticket" --tag feature
cargo run -p tik -- list
```

### Interactive TUI (Default)
Running `tik` with no subcommand launches the TUI in a TTY:
```sh
tik
```
Use `--non-interactive` to require explicit commands (useful for scripts).
Key bindings: `Up/Down` select, `n` new, `a` note, `c` close, `r` reopen, `f` filter, `g` refresh, `q` quit.

### Production Mode (Direct Binary)
After building or installing:
```sh
tik <command> [options]
```

## Command Reference

| Command | Description |
|---------|-------------|
| `tik init` | Initialize a new Tiketer repository |
| `tik status` | Show repository health and statistics |
| `tik config show/get/set` | Read or update repo configuration |
| `tik doctor` | Validate repo integrity and surface issues |
| `tik migrate` | Run schema/layout migrations |
| `tik project init/list/select` | Manage workspace projects |
| `tik milestone new/list/show/set/close` | Manage milestones |
| `tik index rebuild` | Rebuild full-text index |
| `tik index status` | Show index freshness and metadata |
| `tik new <title>` | Create a new ticket |
| `tik list` | List all tickets |
| `tik show <id>` | Display ticket details |
| `tik note <id> <text>` | Add a note to a ticket |
| `tik edit <id>` | Edit ticket JSON in `$EDITOR` |
| `tik edit <id> --notes` | Edit ticket notes in `$EDITOR` |
| `tik log <id>` | Show ticket event history |
| `tik close <id>` | Close a ticket |
| `tik reopen <id>` | Reopen a closed ticket |
| `tik search <query>` | Search tickets with filters |
| `tik stats` | Show aggregate repository stats |
| `tik report` | Generate a report summary |
| `tik graph` | Show ticket dependency graph |
| `tik export` | Export repo data |
| `tik import` | Import repo data |
| `tik relate <id>` | Add ticket relation |
| `tik unrelate <id>` | Remove ticket relation |
| `tik link <id>` | Add artifact link |
| `tik unlink <id>` | Remove artifact link |
| `tik assign <id>` | Add/remove/set assignees |
| `tik tag <id>` | Add/remove/set tags |

### Global Options

| Option | Description |
|--------|-------------|
| `--format <fmt>` | Output format: `table`, `compact`, `json`, `jsonl`, `yaml`, `md`, `csv` |
| `--path <dir>` | Specify repository root path |
| `--no-color` | Disable colored output |
| `--quiet` | Suppress output |
| `--non-interactive` | Disable interactive TUI and require a command |
| `--project <name>` | Select a project in the current workspace |
| `--help` | Show help for any command |

## Workflow Example

```sh
# 1. Initialize a new repository
tik init

# 2. Create tickets
tik new "Implement user authentication" --tag security,mvp
tik new "Add database migrations" --tag backend

# 3. List open tickets
tik list --status open

# 4. View a specific ticket
tik show T-01ARZ3NDEKTSV4RRFFQ69G5FAV

# 5. Add progress notes
tik note T-01ARZ3NDEKTSV4RRFFQ69G5FAV "Started implementation"

# 5b. Edit ticket JSON or notes
tik edit T-01ARZ3NDEKTSV4RRFFQ69G5FAV
tik edit T-01ARZ3NDEKTSV4RRFFQ69G5FAV --notes

# 6. Close when done
tik close T-01ARZ3NDEKTSV4RRFFQ69G5FAV --reason "merged to main"
```

## Milestones

```sh
tik milestone new "Phase 1" --tag mvp --due-at 2026-02-01T00:00:00Z
tik milestone list --status open
tik milestone show M-01ARZ3NDEKTSV4RRFFQ69G5FAV
tik milestone set T-01ARZ3NDEKTSV4RRFFQ69G5FAV M-01ARZ3NDEKTSV4RRFFQ69G5FAV
tik milestone close M-01ARZ3NDEKTSV4RRFFQ69G5FAV --reason "shipped"
```

## Projects

```sh
tik project list
tik project init "client-a" --description "Client A work"
tik project select "client-a"
tik --project client-a list
```

Projects are isolated under `.tik/projects/<name>/`. Use `--project` for monorepos or when you want
to keep tickets separated by context.

## Search and Index

```sh
tik index rebuild
tik index status
tik search "status:open tag:mvp login"
tik search "created:>=2026-01-01 due:2026-02-01..2026-03-01"
```

Query filters:
- `status`, `tag`, `assignee`, `type`, `priority`, `severity`, `milestone`
- `created`, `updated`, `closed`, `due` with `>=`, `<=`, or `start..end`

Sorting:
- `tik list --sort updated`
- `tik search "tag:mvp" --sort priority`
- `tik report --metric summary --sort created`

## Reporting and Graphs

```sh
tik stats
tik stats --status open --tag mvp --since 2026-01-01
tik report --metric summary --limit 5 --sort updated --status open
tik report --metric burndown --group-by week --since 2026-01-01 --until 2026-02-01
tik report --metric throughput --group-by month --since 2026-01-01
tik graph --root T-01ARZ3NDEKTSV4RRFFQ69G5FAV --depth 2 --relation blocks --include-milestones
tik graph --dot > graph.dot
```

Report filters:
- `--status`, `--tag`, `--assignee`, `--milestone`, `--since`, `--until`

Group-by values for burndown/throughput: `day`, `week`, `month`.

## Doctor and Migrate

```sh
tik doctor
tik doctor --all-projects
tik migrate
```

## Pagination

Ticket list and search outputs are paginated by default (50 results).
Use `--limit` and `--offset` to page, or `--all` to return everything.

```sh
tik list --limit 25 --offset 25
tik search "tag:mvp" --all
```

## Import and Export

```sh
tik export --format json > export.json
tik export --format csv --scope tickets > tickets.csv
tik import --input-format json --input export.json
tik import --input-format csv --scope tickets --input tickets.csv
```

Notes:
- CSV import/export requires `--scope tickets` or `--scope milestones`.
- Markdown import expects a YAML frontmatter block exported by `tik export --format md`.

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `TIK_ACTOR` | Override the author name for operations | `$USER` |

### Repository Structure

After running `tik init`, the following structure is created:

```
.tik/
  repo.json              # Repository metadata
  config.json            # User configuration
  workspace.json         # Workspace + project selection
  schema/
    ticket.schema.json   # Ticket JSON schema
    milestone.schema.json
    event.schema.json
    config.schema.json
  projects/
    default/
      project.json       # Project metadata
      config.json        # Project configuration
      tickets/
        T-<ulid>/
          ticket.json    # Ticket data
          notes.jsonl    # Event log (append-only)
          notes.md       # Human-readable notes
      milestones/
        M-<ulid>.json    # Milestone data
        M-<ulid>.jsonl   # Milestone events (append-only)
      index/
        fts.sqlite       # SQLite FTS index (derived)
        tickets.jsonl    # Index snapshot (derived)
      locks/
      tmp/
  locks/
  tmp/
```

## Output Formats

Use `--format` to change output:

```sh
tik list --format json     # Machine-readable JSON
tik list --format yaml     # YAML format
tik list --format csv      # CSV (nested fields as JSON strings)
tik list --format table    # Human-readable table (default)
tik list --format compact  # Compact single-line format
tik list --format md       # Markdown format
tik list --format jsonl    # JSON Lines (one object per line)
```

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

## Testing

Run the test suite:
```sh
cargo test
```

Run tests for a specific crate:
```sh
cargo test -p tik-core
cargo test -p tik
```

Seed data for manual testing lives in `fixtures/seed`:
```sh
cd fixtures/seed && tik
```

## Troubleshooting

### "Repository not initialized"
Run `tik init` in your project directory to create the `.tik/` directory.

### "Ticket not found"
Ensure you're using the full ticket ID (e.g., `T-01ARZ3NDEKTSV4RRFFQ69G5FAV`).

### "Lock contention"
Another process may be accessing the repository. Wait and retry, or check for stale lock files in `.tik/`.

### Build Errors
Ensure Rust 1.78+ is installed:
```sh
rustup update stable
```
