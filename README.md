# tik

A local-first CLI ticketing system for agents & solo work, written in Rust. It stores all data in `.tik/`, uses schema-first JSON/JSONL for deterministic automation, and keeps append-only history for traceability.

## Status
Implemented in this repo:
- `tik init` repo scaffold with schemas and config.
- `tik status` repo health summary.
- `tik new` create tickets.
- `tik list` list tickets with optional status filter.
- `tik show` view a ticket.
- `tik note` append notes (JSONL + Markdown).
- `tik edit` edit `ticket.json` or `notes.md` with `$EDITOR` and schema validation.
- `tik log` read ticket history.
- `tik close` and `tik reopen` with optional reason.
- `tik relate` and `tik unrelate` to manage dependencies.
- `tik link` and `tik unlink` to manage artifacts.
- `tik assign` to add/remove/set assignees.
- `tik tag` to add/remove/set tags.
- `tik config show/get/set` for repo config.
- `tik milestone new/list/show/set/close`.
- `tik index rebuild` and `tik search` with query filters.
- `tik stats`, `tik report`, and `tik graph` for reporting.
- `tik export` and `tik import` for repo data exchange.
- Multi-format output: `table`, `compact`, `json`, `jsonl`, `yaml`, `md`, `csv`.

Planned features are tracked in `PLAN.md`.

## Quickstart
```sh
cargo run -p tik -- init
cargo run -p tik -- new "Initial setup" --tag mvp,cli
cargo run -p tik -- list --status open
```

## Usage
All commands accept `--format` and `--path` (optional root path). Use `--quiet` to suppress output.

### Interactive TUI (default)
Running `tik` with no subcommand launches the TUI in a TTY.
Use `--non-interactive` to require explicit commands for scripting.
Key bindings: `Up/Down` select, `n` new, `a` note, `c` close, `r` reopen, `f` filter, `g` refresh, `q` quit.

### Initialize a repo
```sh
tik init
```

### Repo status
```sh
tik status --format json
```

### Create a ticket
```sh
tik new "Add CLI init command" --tag cli,mvp
```

Optional fields:
- `--summary` for a short summary.
- `--description` for full Markdown description.
- `--actor` to override author (defaults to `TIK_ACTOR`, then `USER`).

### List tickets
```sh
tik list --status open --format table
```

### Show a ticket
```sh
tik show T-01ARZ3NDEKTSV4RRFFQ69G5FAV
```

### Add a note
```sh
tik note T-01ARZ3NDEKTSV4RRFFQ69G5FAV "Investigated schema options"
```

### Read ticket history
```sh
tik log T-01ARZ3NDEKTSV4RRFFQ69G5FAV --format jsonl
```

### Edit ticket fields or notes
```sh
tik edit T-01ARZ3NDEKTSV4RRFFQ69G5FAV
tik edit T-01ARZ3NDEKTSV4RRFFQ69G5FAV --notes
```

### Close or reopen
```sh
tik close T-01ARZ3NDEKTSV4RRFFQ69G5FAV --reason "merged"
tik reopen T-01ARZ3NDEKTSV4RRFFQ69G5FAV --reason "regression found"
```

### Relate tickets
```sh
tik relate T-01ARZ3NDEKTSV4RRFFQ69G5FAV --relation blocks --target T-01ARZ3NDEKTSV4RRFFQ69G5FAA
tik unrelate T-01ARZ3NDEKTSV4RRFFQ69G5FAV --relation blocks --target T-01ARZ3NDEKTSV4RRFFQ69G5FAA
```

### Link artifacts
```sh
tik link T-01ARZ3NDEKTSV4RRFFQ69G5FAV --artifact file --reference docs/design.md
tik unlink T-01ARZ3NDEKTSV4RRFFQ69G5FAV --artifact file --reference docs/design.md
```

File artifacts must be relative paths and must not contain `..`.

### Assign assignees
```sh
tik assign T-01ARZ3NDEKTSV4RRFFQ69G5FAV --add alice,bob
tik assign T-01ARZ3NDEKTSV4RRFFQ69G5FAV --remove alice
tik assign T-01ARZ3NDEKTSV4RRFFQ69G5FAV --set alice,bob
tik assign T-01ARZ3NDEKTSV4RRFFQ69G5FAV --clear
```

### Tag tickets
```sh
tik tag T-01ARZ3NDEKTSV4RRFFQ69G5FAV --add mvp,cli
tik tag T-01ARZ3NDEKTSV4RRFFQ69G5FAV --remove cli
tik tag T-01ARZ3NDEKTSV4RRFFQ69G5FAV --set mvp,cli
tik tag T-01ARZ3NDEKTSV4RRFFQ69G5FAV --clear
```

### Config
```sh
tik config show
tik config get output_format
tik config set output_format json
```

### Milestones
```sh
tik milestone new "Phase 1" --tag mvp --due-at 2026-02-01T00:00:00Z
tik milestone list --status open
tik milestone show M-01ARZ3NDEKTSV4RRFFQ69G5FAV
tik milestone set T-01ARZ3NDEKTSV4RRFFQ69G5FAV M-01ARZ3NDEKTSV4RRFFQ69G5FAV
tik milestone close M-01ARZ3NDEKTSV4RRFFQ69G5FAV --reason "shipped"
```

### Search and index
```sh
tik index rebuild
tik search "status:open tag:mvp login"
tik search "created:>=2026-01-01 due:2026-02-01..2026-03-01"
```

Query filters:
- `status`, `tag`, `assignee`, `type`, `priority`, `severity`, `milestone`
- `created`, `updated`, `closed`, `due` with `>=`, `<=`, or `start..end`

### Reporting and graphs
```sh
tik stats
tik report --limit 5
tik graph --format md
```

### Import and export
```sh
tik export --format json > export.json
tik export --format csv --scope tickets > tickets.csv
tik import --input-format json --input export.json
tik import --input-format csv --scope tickets --input tickets.csv
```

Config keys:
- `output_format`: table, compact, json, jsonl, yaml, md, csv.
- `pager`: auto, always, never.
- `timezone`: UTC, local.

## Output Formats
Supported values for `--format`:
- `table` (default)
- `compact`
- `json`
- `jsonl`
- `yaml`
- `md`
- `csv`

For CSV outputs, nested fields (tags, relations, artifacts, custom) are serialized as JSON strings.

## Repo Layout (Excerpt)
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
    T-.../
      ticket.json
      notes.jsonl
      notes.md
  milestones/
    M-...json
    M-...jsonl
  index/
    fts.sqlite
    tickets.jsonl
```

## Tests
Run after every change:
```sh
cargo test
```

## License
MIT
