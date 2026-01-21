use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell};
use is_terminal::IsTerminal;
use tik_core::{
    sort_tickets, ArtifactType, BackupSummary, BurndownReport, DataMigrationReport, DoctorReport,
    Event, ExportBundle, ExportMilestone, ExportTicket, Graph, GraphOptions, ImportSummary,
    IndexStatus, IndexSummary, MigrationSummary, Milestone, MilestoneId, MilestoneStatus,
    MilestoneSummary, NewMilestone, NewTicket, ProjectMeta, RelationType, Repo, RepoStatus, Report,
    ReportFilters, ReportGroupBy, RestoreSummary, Result, SearchQuery, Stats, ThroughputReport,
    Ticket, TicketId, TicketSort, TicketStatus, TicketSummary, TikError,
};

mod tui;

const DEFAULT_PAGE_SIZE: usize = 50;

#[derive(Parser)]
#[command(name = "tik", version, about = "Tiketer CLI")]
struct Cli {
    #[arg(long, value_enum, global = true)]
    format: Option<OutputFormat>,
    #[arg(long, global = true)]
    no_color: bool,
    #[arg(long, global = true)]
    quiet: bool,
    #[arg(
        long,
        global = true,
        help = "Disable interactive mode and require a command"
    )]
    non_interactive: bool,
    #[arg(
        long,
        global = true,
        help = "Select a project in the current workspace"
    )]
    project: Option<String>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Status {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Doctor {
        #[arg(long)]
        all_projects: bool,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Migrate {
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Milestone {
        #[command(subcommand)]
        command: MilestoneCommand,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Index {
        #[command(subcommand)]
        command: IndexCommand,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Search {
        #[arg(required = true)]
        query: Vec<String>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum)]
        sort: Option<TicketSortArg>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Stats {
        #[arg(long, value_enum, value_delimiter = ',')]
        status: Vec<TicketStatusArg>,
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        assignee: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        milestone: Vec<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Report {
        #[arg(long, default_value_t = 10)]
        limit: usize,
        #[arg(long, value_enum)]
        sort: Option<TicketSortArg>,
        #[arg(long, value_enum, default_value = "summary")]
        metric: ReportMetricArg,
        #[arg(long, value_enum, default_value = "day")]
        group_by: ReportGroupByArg,
        #[arg(long, value_enum, value_delimiter = ',')]
        status: Vec<TicketStatusArg>,
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        assignee: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        milestone: Vec<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Graph {
        #[arg(long, value_delimiter = ',')]
        root: Vec<String>,
        #[arg(long)]
        depth: Option<usize>,
        #[arg(long, value_enum, value_delimiter = ',')]
        relation: Vec<RelationTypeArg>,
        #[arg(long)]
        include_milestones: bool,
        #[arg(long)]
        dot: bool,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Export {
        #[arg(long, value_enum, default_value = "all")]
        scope: ExportScopeArg,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Import {
        #[arg(long, value_enum)]
        input_format: ImportFormatArg,
        #[arg(long)]
        input: PathBuf,
        #[arg(long, value_enum, default_value = "all")]
        scope: ExportScopeArg,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    New {
        title: String,
        #[arg(long)]
        summary: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Show {
        id: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    List {
        #[arg(long, value_enum)]
        status: Option<TicketStatusArg>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum)]
        sort: Option<TicketSortArg>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Note {
        id: String,
        text: String,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Edit {
        id: String,
        #[arg(long)]
        notes: bool,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Assign {
        id: String,
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        remove: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        set: Vec<String>,
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Tag {
        id: String,
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        remove: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        set: Vec<String>,
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Relate {
        id: String,
        #[arg(long, value_enum)]
        relation: RelationTypeArg,
        #[arg(long)]
        target: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Unrelate {
        id: String,
        #[arg(long, value_enum)]
        relation: RelationTypeArg,
        #[arg(long)]
        target: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Link {
        id: String,
        #[arg(long, value_enum)]
        artifact: ArtifactTypeArg,
        #[arg(long)]
        reference: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Unlink {
        id: String,
        #[arg(long, value_enum)]
        artifact: ArtifactTypeArg,
        #[arg(long)]
        reference: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Close {
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Reopen {
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Log {
        id: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    #[command(about = "Generate shell completion scripts")]
    Completion {
        #[arg(value_enum, help = "Shell to generate completions for")]
        shell: ShellArg,
    },
    #[command(about = "Create a backup of the repository")]
    Backup {
        #[arg(long, help = "Output file path")]
        output: PathBuf,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    #[command(about = "Restore a repository from backup")]
    Restore {
        #[arg(long, help = "Input backup file path")]
        input: PathBuf,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    #[command(about = "Run data migrations to update schema versions")]
    DataMigrate {
        #[arg(long, help = "Perform a dry run without making changes")]
        dry_run: bool,
        #[arg(long, help = "Target schema version (default: latest)")]
        to_version: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    #[command(about = "Toggle acceptance criterion completion")]
    Accept {
        id: String,
        #[arg(help = "Index of the acceptance criterion (1-based)")]
        index: usize,
        #[arg(long)]
        actor: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    Show,
    Get { key: String },
    Set { key: String, value: String },
}

#[derive(Subcommand)]
enum ProjectCommand {
    Init {
        name: String,
        #[arg(long)]
        description: Option<String>,
    },
    List,
    Select {
        name: String,
    },
}

#[derive(Subcommand)]
enum IndexCommand {
    Rebuild,
    Status,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ExportScopeArg {
    #[value(name = "all")]
    All,
    #[value(name = "tickets")]
    Tickets,
    #[value(name = "milestones")]
    Milestones,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ImportFormatArg {
    #[value(name = "json")]
    Json,
    #[value(name = "yaml")]
    Yaml,
    #[value(name = "csv")]
    Csv,
    #[value(name = "md")]
    Md,
}

#[derive(Subcommand)]
enum MilestoneCommand {
    New {
        title: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        due_at: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tag: Vec<String>,
        #[arg(long)]
        actor: Option<String>,
    },
    List {
        #[arg(long, value_enum)]
        status: Option<MilestoneStatusArg>,
    },
    Show {
        id: String,
    },
    Close {
        id: String,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
    },
    Set {
        ticket_id: String,
        milestone_id: Option<String>,
        #[arg(long)]
        clear: bool,
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        actor: Option<String>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum TicketSortArg {
    Id,
    Updated,
    Created,
    Priority,
    Status,
    Title,
}

impl From<TicketSortArg> for TicketSort {
    fn from(value: TicketSortArg) -> Self {
        match value {
            TicketSortArg::Id => TicketSort::Id,
            TicketSortArg::Updated => TicketSort::Updated,
            TicketSortArg::Created => TicketSort::Created,
            TicketSortArg::Priority => TicketSort::Priority,
            TicketSortArg::Status => TicketSort::Status,
            TicketSortArg::Title => TicketSort::Title,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ReportMetricArg {
    #[value(name = "summary")]
    Summary,
    #[value(name = "burndown")]
    Burndown,
    #[value(name = "throughput")]
    Throughput,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ReportGroupByArg {
    #[value(name = "day")]
    Day,
    #[value(name = "week")]
    Week,
    #[value(name = "month")]
    Month,
}

impl From<ReportGroupByArg> for ReportGroupBy {
    fn from(value: ReportGroupByArg) -> Self {
        match value {
            ReportGroupByArg::Day => ReportGroupBy::Day,
            ReportGroupByArg::Week => ReportGroupBy::Week,
            ReportGroupByArg::Month => ReportGroupBy::Month,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum OutputFormat {
    Table,
    Compact,
    Json,
    Jsonl,
    Yaml,
    Md,
    Csv,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PagerMode {
    Auto,
    Always,
    Never,
}

impl TryFrom<&str> for OutputFormat {
    type Error = TikError;

    fn try_from(value: &str) -> Result<Self> {
        let value = value.trim().to_lowercase();
        match value.as_str() {
            "table" => Ok(OutputFormat::Table),
            "compact" => Ok(OutputFormat::Compact),
            "json" => Ok(OutputFormat::Json),
            "jsonl" => Ok(OutputFormat::Jsonl),
            "yaml" => Ok(OutputFormat::Yaml),
            "md" => Ok(OutputFormat::Md),
            "csv" => Ok(OutputFormat::Csv),
            _ => Err(TikError::Config(format!("invalid output_format: {value}"))),
        }
    }
}

impl TryFrom<&str> for PagerMode {
    type Error = TikError;

    fn try_from(value: &str) -> Result<Self> {
        let value = value.trim().to_lowercase();
        match value.as_str() {
            "auto" => Ok(PagerMode::Auto),
            "always" => Ok(PagerMode::Always),
            "never" => Ok(PagerMode::Never),
            _ => Err(TikError::Config(format!("invalid pager mode: {value}"))),
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum TicketStatusArg {
    #[value(name = "open")]
    Open,
    #[value(name = "in_progress", alias = "in-progress")]
    InProgress,
    #[value(name = "blocked")]
    Blocked,
    #[value(name = "closed")]
    Closed,
    #[value(name = "archived")]
    Archived,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum MilestoneStatusArg {
    #[value(name = "open")]
    Open,
    #[value(name = "closed")]
    Closed,
    #[value(name = "archived")]
    Archived,
}

#[derive(Debug)]
enum ListOperation {
    Add(Vec<String>),
    Remove(Vec<String>),
    Set(Vec<String>),
    Clear,
}

fn resolve_list_operation(
    add: Vec<String>,
    remove: Vec<String>,
    set: Vec<String>,
    clear: bool,
    label: &str,
) -> Result<ListOperation> {
    let mut count = 0;
    if !add.is_empty() {
        count += 1;
    }
    if !remove.is_empty() {
        count += 1;
    }
    if !set.is_empty() {
        count += 1;
    }
    if clear {
        count += 1;
    }

    if count == 0 {
        return Err(TikError::usage(&format!(
            "{label} requires --add, --remove, --set, or --clear"
        )));
    }
    if count > 1 {
        return Err(TikError::usage(&format!(
            "{label} requires exactly one operation"
        )));
    }

    if clear {
        return Ok(ListOperation::Clear);
    }
    if !add.is_empty() {
        return Ok(ListOperation::Add(add));
    }
    if !remove.is_empty() {
        return Ok(ListOperation::Remove(remove));
    }
    Ok(ListOperation::Set(set))
}

fn resolve_milestone_target(
    milestone_id: Option<String>,
    clear: bool,
) -> Result<Option<MilestoneId>> {
    match (milestone_id, clear) {
        (Some(_), true) => Err(TikError::usage(
            "milestone id cannot be combined with --clear",
        )),
        (None, false) => Err(TikError::usage(
            "milestone id is required unless --clear is set",
        )),
        (None, true) => Ok(None),
        (Some(id), false) => Ok(Some(MilestoneId::parse(&id)?)),
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ShellArg {
    #[value(name = "bash")]
    Bash,
    #[value(name = "zsh")]
    Zsh,
    #[value(name = "fish")]
    Fish,
    #[value(name = "powershell")]
    PowerShell,
    #[value(name = "elvish")]
    Elvish,
}

impl From<ShellArg> for Shell {
    fn from(value: ShellArg) -> Self {
        match value {
            ShellArg::Bash => Shell::Bash,
            ShellArg::Zsh => Shell::Zsh,
            ShellArg::Fish => Shell::Fish,
            ShellArg::PowerShell => Shell::PowerShell,
            ShellArg::Elvish => Shell::Elvish,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum RelationTypeArg {
    #[value(name = "blocks")]
    Blocks,
    #[value(name = "blocked_by", alias = "blocked-by")]
    BlockedBy,
    #[value(name = "depends_on", alias = "depends-on")]
    DependsOn,
    #[value(name = "duplicate")]
    Duplicate,
    #[value(name = "parent")]
    Parent,
    #[value(name = "child")]
    Child,
}

impl From<RelationTypeArg> for RelationType {
    fn from(value: RelationTypeArg) -> Self {
        match value {
            RelationTypeArg::Blocks => RelationType::Blocks,
            RelationTypeArg::BlockedBy => RelationType::BlockedBy,
            RelationTypeArg::DependsOn => RelationType::DependsOn,
            RelationTypeArg::Duplicate => RelationType::Duplicate,
            RelationTypeArg::Parent => RelationType::Parent,
            RelationTypeArg::Child => RelationType::Child,
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ArtifactTypeArg {
    #[value(name = "file")]
    File,
    #[value(name = "url")]
    Url,
    #[value(name = "commit")]
    Commit,
}

impl From<ArtifactTypeArg> for ArtifactType {
    fn from(value: ArtifactTypeArg) -> Self {
        match value {
            ArtifactTypeArg::File => ArtifactType::File,
            ArtifactTypeArg::Url => ArtifactType::Url,
            ArtifactTypeArg::Commit => ArtifactType::Commit,
        }
    }
}

impl From<TicketStatusArg> for TicketStatus {
    fn from(value: TicketStatusArg) -> Self {
        match value {
            TicketStatusArg::Open => TicketStatus::Open,
            TicketStatusArg::InProgress => TicketStatus::InProgress,
            TicketStatusArg::Blocked => TicketStatus::Blocked,
            TicketStatusArg::Closed => TicketStatus::Closed,
            TicketStatusArg::Archived => TicketStatus::Archived,
        }
    }
}

impl From<MilestoneStatusArg> for MilestoneStatus {
    fn from(value: MilestoneStatusArg) -> Self {
        match value {
            MilestoneStatusArg::Open => MilestoneStatus::Open,
            MilestoneStatusArg::Closed => MilestoneStatus::Closed,
            MilestoneStatusArg::Archived => MilestoneStatus::Archived,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error (code {}): {err}", err.code().as_u8());
            ExitCode::from(err.code().as_u8())
        }
    }
}

fn run(mut cli: Cli) -> Result<()> {
    if let Some(command) = cli.command.take() {
        return run_command(&cli, command);
    }
    if cli.non_interactive {
        return Err(TikError::usage(
            "command required in non-interactive mode; run `tik --help`",
        ));
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(TikError::usage(
            "interactive mode requires a TTY; pass --non-interactive or a command",
        ));
    }
    tui::run_tui(cli.no_color, cli.project.as_deref())
}

fn run_command(cli: &Cli, command: Commands) -> Result<()> {
    match command {
        Commands::Init { path } => {
            let root = resolve_root(path)?;
            let repo = Repo::init(&root, env!("CARGO_PKG_VERSION"))?;
            let status = repo.status()?;
            if !cli.quiet {
                let config = tik_core::Config::default();
                let format = resolve_output_format(cli.format, Some(&config))?;
                let pager = resolve_pager_mode(Some(&config))?;
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_status(format, &status)?,
                )?;
            }
        }
        Commands::Status { path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let status = repo.status()?;
            if !cli.quiet {
                let config = repo.config_show()?;
                let format = resolve_output_format(cli.format, Some(&config))?;
                let pager = resolve_pager_mode(Some(&config))?;
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_status(format, &status)?,
                )?;
            }
        }
        Commands::Config { command, path } => {
            let mut repo = resolve_repo(path, cli.project.as_deref())?;
            match command {
                ConfigCommand::Show => {
                    let config = repo.config_show()?;
                    if !cli.quiet {
                        let format = resolve_output_format(cli.format, Some(&config))?;
                        let pager = resolve_pager_mode(Some(&config))?;
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_config(format, &config)?,
                        )?;
                    }
                }
                ConfigCommand::Get { key } => {
                    let value = repo.config_get(&key)?;
                    if !cli.quiet {
                        let config = repo.config_show()?;
                        let format = resolve_output_format(cli.format, Some(&config))?;
                        let pager = resolve_pager_mode(Some(&config))?;
                        let output = render_config_value(format, &key, &value)?;
                        print_output(format, cli.no_color, pager, &output)?;
                    }
                }
                ConfigCommand::Set { key, value } => {
                    let config = repo.config_set(&key, &value)?;
                    if !cli.quiet {
                        let format = resolve_output_format(cli.format, Some(&config))?;
                        let pager = resolve_pager_mode(Some(&config))?;
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_config(format, &config)?,
                        )?;
                    }
                }
            }
        }
        Commands::Project { command, path } => {
            let mut repo = resolve_repo(path, None)?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            match command {
                ProjectCommand::Init { name, description } => {
                    let project = repo.project_init(&name, description.as_deref())?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_project(format, &project)?,
                        )?;
                    }
                }
                ProjectCommand::List => {
                    let projects = repo.project_list()?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_project_list(format, &projects)?,
                        )?;
                    }
                }
                ProjectCommand::Select { name } => {
                    let project = repo.project_select(&name)?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_project(format, &project)?,
                        )?;
                    }
                }
            }
        }
        Commands::Doctor { all_projects, path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let report = repo.doctor(all_projects)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_doctor(format, &report)?,
                )?;
            }
        }
        Commands::Migrate { path } => {
            let mut repo = resolve_repo(path, None)?;
            let summary = repo.migrate()?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_migration_summary(format, &summary)?,
                )?;
            }
        }
        Commands::Milestone { command, path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            match command {
                MilestoneCommand::New {
                    title,
                    description,
                    due_at,
                    tag,
                    actor,
                } => {
                    let actor = resolve_actor(actor);
                    let milestone = repo.create_milestone(
                        NewMilestone {
                            title,
                            description,
                            due_at,
                            tags: tag,
                        },
                        &actor,
                    )?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_milestone(format, &milestone)?,
                        )?;
                    }
                }
                MilestoneCommand::List { status } => {
                    let filter = status.map(MilestoneStatus::from);
                    let milestones = repo.list_milestones(filter)?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_milestone_list(format, &milestones)?,
                        )?;
                    }
                }
                MilestoneCommand::Show { id } => {
                    let milestone_id = MilestoneId::parse(&id)?;
                    let milestone = repo.load_milestone(&milestone_id)?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_milestone(format, &milestone)?,
                        )?;
                    }
                }
                MilestoneCommand::Close { id, reason, actor } => {
                    let actor = resolve_actor(actor);
                    let milestone_id = MilestoneId::parse(&id)?;
                    let milestone =
                        repo.close_milestone(&milestone_id, &actor, reason.as_deref())?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_milestone(format, &milestone)?,
                        )?;
                    }
                }
                MilestoneCommand::Set {
                    ticket_id,
                    milestone_id,
                    clear,
                    reason,
                    actor,
                } => {
                    let actor = resolve_actor(actor);
                    let ticket_id = TicketId::parse(&ticket_id)?;
                    let target = resolve_milestone_target(milestone_id, clear)?;
                    let ticket = repo.set_ticket_milestone(
                        &ticket_id,
                        target.as_ref(),
                        &actor,
                        reason.as_deref(),
                    )?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_ticket(format, &ticket)?,
                        )?;
                    }
                }
            }
        }
        Commands::Index { command, path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            match command {
                IndexCommand::Rebuild => {
                    let summary = repo.rebuild_index()?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_index_summary(format, &summary)?,
                        )?;
                    }
                }
                IndexCommand::Status => {
                    let status = repo.index_status()?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_index_status(format, &status)?,
                        )?;
                    }
                }
            }
        }
        Commands::Search {
            query,
            limit,
            offset,
            all,
            sort,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let raw_query = query.join(" ");
            let query = SearchQuery::parse(&raw_query)?;
            let (offset, limit) = resolve_pagination(all, limit, offset, DEFAULT_PAGE_SIZE)?;
            let tickets = if let Some(sort) = sort {
                let mut tickets = repo.search(&query)?;
                sort_tickets(&mut tickets, TicketSort::from(sort));
                apply_pagination(tickets, offset, limit)
            } else {
                repo.search_page(&query, offset, limit)?
            };
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket_list(format, &tickets)?,
                )?;
            }
        }
        Commands::Stats {
            status,
            tag,
            assignee,
            milestone,
            since,
            until,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let filters = build_report_filters(status, tag, assignee, milestone, since, until)?;
            let stats = repo.stats_filtered(&filters)?;
            if !cli.quiet {
                print_output(format, cli.no_color, pager, &render_stats(format, &stats)?)?;
            }
        }
        Commands::Report {
            limit,
            sort,
            metric,
            group_by,
            status,
            tag,
            assignee,
            milestone,
            since,
            until,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let filters = build_report_filters(status, tag, assignee, milestone, since, until)?;
            match metric {
                ReportMetricArg::Summary => {
                    let sort = sort.map(TicketSort::from).unwrap_or(TicketSort::Updated);
                    let report = repo.report_filtered_sorted(limit, sort, &filters)?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_report(format, &report)?,
                        )?;
                    }
                }
                ReportMetricArg::Burndown => {
                    if sort.is_some() {
                        return Err(TikError::usage(
                            "--sort is only supported with --metric summary",
                        ));
                    }
                    let report = repo.burndown(&filters, ReportGroupBy::from(group_by))?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_burndown(format, &report)?,
                        )?;
                    }
                }
                ReportMetricArg::Throughput => {
                    if sort.is_some() {
                        return Err(TikError::usage(
                            "--sort is only supported with --metric summary",
                        ));
                    }
                    let report = repo.throughput(&filters, ReportGroupBy::from(group_by))?;
                    if !cli.quiet {
                        print_output(
                            format,
                            cli.no_color,
                            pager,
                            &render_throughput(format, &report)?,
                        )?;
                    }
                }
            }
        }
        Commands::Graph {
            root,
            depth,
            relation,
            include_milestones,
            dot,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            if dot && cli.format.is_some() {
                return Err(TikError::usage("--dot cannot be combined with --format"));
            }
            let options =
                build_graph_options(root, depth, relation, include_milestones)?;
            let graph = repo.graph_with_options(&options)?;
            if !cli.quiet {
                if dot {
                    print_output(format, cli.no_color, pager, &render_graph_dot(&graph))?;
                } else {
                    print_output(format, cli.no_color, pager, &render_graph(format, &graph)?)?;
                }
            }
        }
        Commands::Export {
            scope,
            output,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let bundle = repo.export_bundle()?;
            let bundle = filter_export_bundle(bundle, scope);
            let content = render_export(format, &bundle, scope)?;
            if let Some(output) = output {
                write_output_file(&output, &content)?;
            }
            if !cli.quiet {
                print_output(format, cli.no_color, pager, &content)?;
            }
        }
        Commands::Import {
            input_format,
            input,
            scope,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let raw = std::fs::read_to_string(&input)
                .map_err(|err| TikError::io("read import file", err))?;
            let bundle = parse_import_bundle(input_format, &raw, scope)?;
            let summary = repo.import_bundle(bundle, &actor)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_import_summary(format, &summary)?,
                )?;
            }
        }
        Commands::New {
            title,
            summary,
            description,
            tag,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket = repo.create_ticket(
                NewTicket {
                    title,
                    summary,
                    description,
                    tags: tag,
                },
                &actor,
            )?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Show { id, path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let ticket_id = TicketId::parse(&id)?;
            let ticket = repo.load_ticket(&ticket_id)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::List {
            status,
            limit,
            offset,
            all,
            sort,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let filter = status.map(TicketStatus::from);
            let (offset, limit) = resolve_pagination(all, limit, offset, DEFAULT_PAGE_SIZE)?;
            let tickets = if let Some(sort) = sort {
                let mut tickets = repo.list_tickets(filter.clone())?;
                sort_tickets(&mut tickets, TicketSort::from(sort));
                apply_pagination(tickets, offset, limit)
            } else {
                repo.list_tickets_page(filter, offset, limit)?
            };
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket_list(format, &tickets)?,
                )?;
            }
        }
        Commands::Note {
            id,
            text,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let event = repo.append_note(&ticket_id, &actor, &text)?;
            if !cli.quiet {
                print_output(format, cli.no_color, pager, &render_event(format, &event)?)?;
            }
        }
        Commands::Edit {
            id,
            notes,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            if notes {
                let original = repo.read_ticket_notes(&ticket_id)?;
                let edited = edit_text_with_editor(&original)?;
                repo.write_ticket_notes(&ticket_id, &edited)?;
                let event = repo.touch_notes(&ticket_id, &actor, reason.as_deref())?;
                if !cli.quiet {
                    print_output(format, cli.no_color, pager, &render_event(format, &event)?)?;
                }
            } else {
                let raw = repo.read_ticket_raw(&ticket_id)?;
                let edited = edit_text_with_editor(&raw)?;
                let ticket = repo.apply_edit(&ticket_id, &edited, &actor, reason.as_deref())?;
                if !cli.quiet {
                    print_output(
                        format,
                        cli.no_color,
                        pager,
                        &render_ticket(format, &ticket)?,
                    )?;
                }
            }
        }
        Commands::Assign {
            id,
            add,
            remove,
            set,
            clear,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let op = resolve_list_operation(add, remove, set, clear, "assign")?;
            let ticket = match op {
                ListOperation::Add(values) => {
                    repo.add_assignees(&ticket_id, values, &actor, reason.as_deref())?
                }
                ListOperation::Remove(values) => {
                    repo.remove_assignees(&ticket_id, values, &actor, reason.as_deref())?
                }
                ListOperation::Set(values) => {
                    repo.set_assignees(&ticket_id, values, &actor, reason.as_deref())?
                }
                ListOperation::Clear => {
                    repo.set_assignees(&ticket_id, Vec::new(), &actor, reason.as_deref())?
                }
            };
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Tag {
            id,
            add,
            remove,
            set,
            clear,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let op = resolve_list_operation(add, remove, set, clear, "tag")?;
            let ticket = match op {
                ListOperation::Add(values) => {
                    repo.add_tags(&ticket_id, values, &actor, reason.as_deref())?
                }
                ListOperation::Remove(values) => {
                    repo.remove_tags(&ticket_id, values, &actor, reason.as_deref())?
                }
                ListOperation::Set(values) => {
                    repo.set_tags(&ticket_id, values, &actor, reason.as_deref())?
                }
                ListOperation::Clear => {
                    repo.set_tags(&ticket_id, Vec::new(), &actor, reason.as_deref())?
                }
            };
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Relate {
            id,
            relation,
            target,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let target_id = TicketId::parse(&target)?;
            let ticket = repo.add_relation(
                &ticket_id,
                RelationType::from(relation),
                &target_id,
                &actor,
                reason.as_deref(),
            )?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Unrelate {
            id,
            relation,
            target,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let target_id = TicketId::parse(&target)?;
            let ticket = repo.remove_relation(
                &ticket_id,
                RelationType::from(relation),
                &target_id,
                &actor,
                reason.as_deref(),
            )?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Link {
            id,
            artifact,
            reference,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let ticket = repo.add_artifact(
                &ticket_id,
                ArtifactType::from(artifact),
                &reference,
                &actor,
                reason.as_deref(),
            )?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Unlink {
            id,
            artifact,
            reference,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let ticket = repo.remove_artifact(
                &ticket_id,
                ArtifactType::from(artifact),
                &reference,
                &actor,
                reason.as_deref(),
            )?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Close {
            id,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let ticket =
                repo.update_status(&ticket_id, TicketStatus::Closed, &actor, reason.as_deref())?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Reopen {
            id,
            reason,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let ticket =
                repo.update_status(&ticket_id, TicketStatus::Open, &actor, reason.as_deref())?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
        Commands::Log { id, path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let ticket_id = TicketId::parse(&id)?;
            let events = repo.read_events(&ticket_id)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_event_list(format, &events)?,
                )?;
            }
        }
        Commands::Completion { shell } => {
            let mut cmd = Cli::command();
            generate(Shell::from(shell), &mut cmd, "tik", &mut std::io::stdout());
        }
        Commands::Backup { output, path } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let summary = repo.backup(&output)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_backup_summary(format, &summary)?,
                )?;
            }
        }
        Commands::Restore { input, path } => {
            let root = resolve_root(path)?;
            let summary = tik_core::Repo::restore(&root, &input)?;
            if !cli.quiet {
                let config = tik_core::Config::default();
                let format = resolve_output_format(cli.format, Some(&config))?;
                let pager = resolve_pager_mode(Some(&config))?;
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_restore_summary(format, &summary)?,
                )?;
            }
        }
        Commands::DataMigrate {
            dry_run,
            to_version,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let report = repo.run_data_migrations(to_version.as_deref(), dry_run)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_data_migration_report(format, &report, dry_run)?,
                )?;
            }
        }
        Commands::Accept {
            id,
            index,
            actor,
            path,
        } => {
            let repo = resolve_repo(path, cli.project.as_deref())?;
            let config = repo.config_show()?;
            let format = resolve_output_format(cli.format, Some(&config))?;
            let pager = resolve_pager_mode(Some(&config))?;
            let actor = resolve_actor(actor);
            let ticket_id = TicketId::parse(&id)?;
            let ticket = repo.toggle_acceptance(&ticket_id, index, &actor)?;
            if !cli.quiet {
                print_output(
                    format,
                    cli.no_color,
                    pager,
                    &render_ticket(format, &ticket)?,
                )?;
            }
        }
    }

    Ok(())
}

fn resolve_root(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        Some(path) => Ok(path),
        None => std::env::current_dir().map_err(|err| TikError::io("get current dir", err)),
    }
}

fn resolve_repo(path: Option<PathBuf>, project: Option<&str>) -> Result<Repo> {
    let root = resolve_root(path)?;
    Repo::discover_with_project(&root, project)
}

fn resolve_output_format(
    cli_format: Option<OutputFormat>,
    config: Option<&tik_core::Config>,
) -> Result<OutputFormat> {
    if let Some(format) = cli_format {
        return Ok(format);
    }
    if let Some(config) = config {
        return OutputFormat::try_from(config.output_format.as_str());
    }
    Ok(OutputFormat::Table)
}

fn resolve_pager_mode(config: Option<&tik_core::Config>) -> Result<PagerMode> {
    if let Some(config) = config {
        return PagerMode::try_from(config.pager.as_str());
    }
    Ok(PagerMode::Auto)
}

fn resolve_pagination(
    all: bool,
    limit: Option<usize>,
    offset: usize,
    default_limit: usize,
) -> Result<(usize, Option<usize>)> {
    if let Some(limit) = limit {
        if limit == 0 {
            return Err(TikError::usage("limit must be greater than zero"));
        }
    }
    let limit = if all {
        None
    } else {
        Some(limit.unwrap_or(default_limit))
    };
    Ok((offset, limit))
}

fn apply_pagination<T>(items: Vec<T>, offset: usize, limit: Option<usize>) -> Vec<T> {
    let iter = items.into_iter().skip(offset);
    match limit {
        Some(limit) => iter.take(limit).collect(),
        None => iter.collect(),
    }
}

fn build_report_filters(
    status: Vec<TicketStatusArg>,
    tag: Vec<String>,
    assignee: Vec<String>,
    milestone: Vec<String>,
    since: Option<String>,
    until: Option<String>,
) -> Result<ReportFilters> {
    let status_values = status
        .into_iter()
        .map(|value| TicketStatus::from(value).as_str().to_string())
        .collect::<Vec<_>>();
    let mut milestone_values = Vec::new();
    for value in milestone {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(TikError::usage("milestone filter cannot be empty"));
        }
        let id = MilestoneId::parse(trimmed)
            .map_err(|_| TikError::usage(&format!("invalid milestone id: {trimmed}")))?;
        milestone_values.push(id.as_str().to_string());
    }

    ReportFilters::parse(
        status_values,
        tag,
        assignee,
        milestone_values,
        since.as_deref(),
        until.as_deref(),
    )
}

fn build_graph_options(
    root: Vec<String>,
    depth: Option<usize>,
    relation: Vec<RelationTypeArg>,
    include_milestones: bool,
) -> Result<GraphOptions> {
    let mut roots = Vec::new();
    for value in root {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(TikError::usage("root id cannot be empty"));
        }
        let id = TicketId::parse(trimmed)
            .map_err(|_| TikError::usage(&format!("invalid root ticket id: {trimmed}")))?;
        roots.push(id);
    }
    let relations = relation.into_iter().map(RelationType::from).collect();
    Ok(GraphOptions {
        roots,
        depth,
        relations,
        include_milestones,
    })
}

fn resolve_actor(actor: Option<String>) -> String {
    if let Some(actor) = actor {
        return actor;
    }
    if let Ok(actor) = std::env::var("TIK_ACTOR") {
        if !actor.trim().is_empty() {
            return actor;
        }
    }
    if let Ok(actor) = std::env::var("USER") {
        if !actor.trim().is_empty() {
            return actor;
        }
    }
    "unknown".to_string()
}

fn filter_export_bundle(mut bundle: ExportBundle, scope: ExportScopeArg) -> ExportBundle {
    match scope {
        ExportScopeArg::All => bundle,
        ExportScopeArg::Tickets => {
            bundle.milestones.clear();
            bundle
        }
        ExportScopeArg::Milestones => {
            bundle.tickets.clear();
            bundle
        }
    }
}

fn render_export(
    format: OutputFormat,
    bundle: &ExportBundle,
    scope: ExportScopeArg,
) -> Result<String> {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(bundle)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_export_jsonl(bundle),
        OutputFormat::Yaml => serde_yaml::to_string(bundle)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => render_export_md(bundle),
        OutputFormat::Csv => render_export_csv(bundle, scope),
        OutputFormat::Table | OutputFormat::Compact => Err(TikError::usage(
            "export supports json, jsonl, yaml, md, or csv formats",
        )),
    }
}

fn render_export_jsonl(bundle: &ExportBundle) -> Result<String> {
    let mut lines = Vec::new();
    for ticket in &bundle.tickets {
        let value = serde_json::json!({
            "type": "ticket",
            "ticket": ticket.ticket,
            "events": ticket.events,
            "notes_md": ticket.notes_md,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    for milestone in &bundle.milestones {
        let value = serde_json::json!({
            "type": "milestone",
            "milestone": milestone.milestone,
            "events": milestone.events,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn render_export_md(bundle: &ExportBundle) -> Result<String> {
    let yaml = serde_yaml::to_string(bundle)
        .map_err(|err| TikError::internal(&format!("yaml render: {err}")))?;
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&yaml);
    out.push_str("---\n\n");
    out.push_str("# tik export\n\n");
    out.push_str(&format!("- tickets: {}\n", bundle.tickets.len()));
    out.push_str(&format!("- milestones: {}\n", bundle.milestones.len()));
    Ok(out)
}

fn render_export_csv(bundle: &ExportBundle, scope: ExportScopeArg) -> Result<String> {
    match scope {
        ExportScopeArg::All => Err(TikError::usage(
            "csv export requires --scope tickets or --scope milestones",
        )),
        ExportScopeArg::Tickets => {
            let tickets: Vec<Ticket> = bundle.tickets.iter().map(|t| t.ticket.clone()).collect();
            render_ticket_list_csv(&tickets)
        }
        ExportScopeArg::Milestones => {
            let milestones: Vec<Milestone> = bundle
                .milestones
                .iter()
                .map(|m| m.milestone.clone())
                .collect();
            render_milestone_list_csv(&milestones)
        }
    }
}

fn parse_import_bundle(
    format: ImportFormatArg,
    raw: &str,
    scope: ExportScopeArg,
) -> Result<ExportBundle> {
    let bundle = match format {
        ImportFormatArg::Json => serde_json::from_str::<ExportBundle>(raw)
            .map_err(|err| TikError::ImportExport(format!("invalid json: {err}")))?,
        ImportFormatArg::Yaml => serde_yaml::from_str::<ExportBundle>(raw)
            .map_err(|err| TikError::ImportExport(format!("invalid yaml: {err}")))?,
        ImportFormatArg::Md => parse_export_bundle_from_md(raw)?,
        ImportFormatArg::Csv => {
            return parse_import_bundle_csv(raw, scope);
        }
    };

    Ok(filter_export_bundle(bundle, scope))
}

fn parse_export_bundle_from_md(raw: &str) -> Result<ExportBundle> {
    let mut lines = raw.lines();
    let Some(first) = lines.next() else {
        return Err(TikError::ImportExport("empty markdown".to_string()));
    };
    if first.trim() != "---" {
        return Err(TikError::ImportExport(
            "missing markdown frontmatter".to_string(),
        ));
    }
    let mut yaml_lines = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        yaml_lines.push(line);
    }
    let yaml = yaml_lines.join("\n");
    serde_yaml::from_str(&yaml)
        .map_err(|err| TikError::ImportExport(format!("invalid markdown frontmatter: {err}")))
}

fn parse_import_bundle_csv(raw: &str, scope: ExportScopeArg) -> Result<ExportBundle> {
    match scope {
        ExportScopeArg::All => Err(TikError::usage(
            "csv import requires --scope tickets or --scope milestones",
        )),
        ExportScopeArg::Tickets => {
            let tickets = parse_tickets_csv(raw)?;
            let export_tickets = tickets
                .into_iter()
                .map(|ticket| ExportTicket {
                    ticket,
                    events: Vec::new(),
                    notes_md: String::new(),
                })
                .collect();
            Ok(ExportBundle {
                schema_version: "1.0".to_string(),
                exported_at: "import".to_string(),
                tickets: export_tickets,
                milestones: Vec::new(),
            })
        }
        ExportScopeArg::Milestones => {
            let milestones = parse_milestones_csv(raw)?;
            let export_milestones = milestones
                .into_iter()
                .map(|milestone| ExportMilestone {
                    milestone,
                    events: Vec::new(),
                })
                .collect();
            Ok(ExportBundle {
                schema_version: "1.0".to_string(),
                exported_at: "import".to_string(),
                tickets: Vec::new(),
                milestones: export_milestones,
            })
        }
    }
}

fn write_output_file(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| TikError::usage("output path has no parent directory"))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|err| TikError::io("create temp", err))?;
    use std::io::Write;
    temp.write_all(contents.as_bytes())
        .map_err(|err| TikError::io("write temp", err))?;
    temp.flush()
        .map_err(|err| TikError::io("flush temp", err))?;
    temp.as_file()
        .sync_all()
        .map_err(|err| TikError::io("sync temp", err))?;
    temp.persist(path)
        .map_err(|err| TikError::io("persist output", err.error))?;
    Ok(())
}

fn print_output(
    _format: OutputFormat,
    no_color: bool,
    pager: PagerMode,
    output: &str,
) -> Result<()> {
    if no_color {
        console::set_colors_enabled(false);
    }
    if should_page(pager, output) {
        pager::Pager::new().setup();
    }
    println!("{output}");
    Ok(())
}

fn should_page(pager: PagerMode, output: &str) -> bool {
    if !std::io::stdout().is_terminal() {
        return false;
    }
    match pager {
        PagerMode::Never => false,
        PagerMode::Always => true,
        PagerMode::Auto => output.lines().count() > 20,
    }
}

fn open_in_editor(path: &Path) -> Result<()> {
    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .map_err(|_| TikError::ExternalCommand("EDITOR not set".to_string()))?;
    let parts = shell_words::split(&editor)
        .map_err(|err| TikError::ExternalCommand(format!("invalid EDITOR: {err}")))?;
    if parts.is_empty() {
        return Err(TikError::ExternalCommand("EDITOR is empty".to_string()));
    }
    let (cmd, args) = parts
        .split_first()
        .ok_or_else(|| TikError::ExternalCommand("EDITOR command missing".to_string()))?;
    let status = std::process::Command::new(cmd)
        .args(args)
        .arg(path)
        .status()
        .map_err(|err| TikError::ExternalCommand(format!("failed to run editor: {err}")))?;
    if !status.success() {
        return Err(TikError::ExternalCommand(format!(
            "editor exited with status {status}"
        )));
    }
    Ok(())
}

fn edit_text_with_editor(raw: &str) -> Result<String> {
    let mut temp =
        tempfile::NamedTempFile::new().map_err(|err| TikError::io("create temp file", err))?;
    use std::io::Write;
    temp.write_all(raw.as_bytes())
        .map_err(|err| TikError::io("write temp file", err))?;
    temp.flush()
        .map_err(|err| TikError::io("flush temp file", err))?;

    open_in_editor(temp.path())?;

    let edited = std::fs::read_to_string(temp.path())
        .map_err(|err| TikError::io("read edited text", err))?;
    Ok(edited)
}

fn render_status(format: OutputFormat, status: &RepoStatus) -> Result<String> {
    match format {
        OutputFormat::Table => render_status_table(status),
        OutputFormat::Compact => Ok(render_status_compact(status)),
        OutputFormat::Json => serde_json::to_string_pretty(status)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(status)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(status)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_status_md(status)),
        OutputFormat::Csv => render_status_csv(status),
    }
}

fn render_doctor(format: OutputFormat, report: &DoctorReport) -> Result<String> {
    match format {
        OutputFormat::Table => render_doctor_table(report),
        OutputFormat::Compact => Ok(render_doctor_compact(report)),
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(report)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(report)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_doctor_md(report)),
        OutputFormat::Csv => render_doctor_csv(report),
    }
}

fn render_migration_summary(format: OutputFormat, summary: &MigrationSummary) -> Result<String> {
    match format {
        OutputFormat::Table => render_migration_summary_table(summary),
        OutputFormat::Compact => Ok(render_migration_summary_compact(summary)),
        OutputFormat::Json => serde_json::to_string_pretty(summary)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(summary)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(summary)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_migration_summary_md(summary)),
        OutputFormat::Csv => render_migration_summary_csv(summary),
    }
}

fn render_index_status(format: OutputFormat, status: &IndexStatus) -> Result<String> {
    match format {
        OutputFormat::Table => render_index_status_table(status),
        OutputFormat::Compact => Ok(render_index_status_compact(status)),
        OutputFormat::Json => serde_json::to_string_pretty(status)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(status)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(status)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_index_status_md(status)),
        OutputFormat::Csv => render_index_status_csv(status),
    }
}

fn render_backup_summary(format: OutputFormat, summary: &BackupSummary) -> Result<String> {
    match format {
        OutputFormat::Table | OutputFormat::Compact => {
            Ok(format!(
                "Backup completed: {} tickets, {} milestones to {}\nStarted: {}\nCompleted: {}",
                summary.tickets_count,
                summary.milestones_count,
                summary.output_path,
                summary.started_at,
                summary.completed_at
            ))
        }
        OutputFormat::Json => serde_json::to_string_pretty(summary)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(summary)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(summary)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(format!(
            "# Backup Summary\n\n- Output: {}\n- Tickets: {}\n- Milestones: {}\n- Started: {}\n- Completed: {}",
            summary.output_path,
            summary.tickets_count,
            summary.milestones_count,
            summary.started_at,
            summary.completed_at
        )),
        OutputFormat::Csv => {
            csv_write(
                &["output_path", "tickets_count", "milestones_count", "started_at", "completed_at"],
                vec![vec![
                    summary.output_path.clone(),
                    summary.tickets_count.to_string(),
                    summary.milestones_count.to_string(),
                    summary.started_at.clone(),
                    summary.completed_at.clone(),
                ]],
            )
        }
    }
}

fn render_restore_summary(format: OutputFormat, summary: &RestoreSummary) -> Result<String> {
    match format {
        OutputFormat::Table | OutputFormat::Compact => {
            Ok(format!(
                "Restore completed: {} tickets, {} milestones from {}\nStarted: {}\nCompleted: {}",
                summary.tickets_count,
                summary.milestones_count,
                summary.input_path,
                summary.started_at,
                summary.completed_at
            ))
        }
        OutputFormat::Json => serde_json::to_string_pretty(summary)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(summary)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(summary)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(format!(
            "# Restore Summary\n\n- Input: {}\n- Tickets: {}\n- Milestones: {}\n- Started: {}\n- Completed: {}",
            summary.input_path,
            summary.tickets_count,
            summary.milestones_count,
            summary.started_at,
            summary.completed_at
        )),
        OutputFormat::Csv => {
            csv_write(
                &["input_path", "tickets_count", "milestones_count", "started_at", "completed_at"],
                vec![vec![
                    summary.input_path.clone(),
                    summary.tickets_count.to_string(),
                    summary.milestones_count.to_string(),
                    summary.started_at.clone(),
                    summary.completed_at.clone(),
                ]],
            )
        }
    }
}

fn render_data_migration_report(
    format: OutputFormat,
    report: &DataMigrationReport,
    dry_run: bool,
) -> Result<String> {
    match format {
        OutputFormat::Table | OutputFormat::Compact => {
            let status = if dry_run { " (dry run)" } else { "" };
            let mut out = format!(
                "Data migration{}: {} -> {}\nStarted: {}\nCompleted: {}\nSuccess: {}\n",
                status,
                report.from_version,
                report.to_version,
                report.started_at,
                report.completed_at.as_deref().unwrap_or("-"),
                report.success
            );
            out.push_str(&format!(
                "Total: {} tickets, {} milestones, {} events\n",
                report.total_tickets, report.total_milestones, report.total_events
            ));
            if !report.all_warnings.is_empty() {
                out.push_str(&format!("Warnings: {}\n", report.all_warnings.len()));
                for w in &report.all_warnings {
                    out.push_str(&format!("  - {w}\n"));
                }
            }
            if !report.all_errors.is_empty() {
                out.push_str(&format!("Errors: {}\n", report.all_errors.len()));
                for e in &report.all_errors {
                    out.push_str(&format!("  - {e}\n"));
                }
            }
            Ok(out)
        }
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(report)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(report)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => {
            let status = if dry_run { " (dry run)" } else { "" };
            let mut out = format!(
                "# Data Migration Report{}\n\n- From: {}\n- To: {}\n- Success: {}\n\n",
                status, report.from_version, report.to_version, report.success
            );
            out.push_str(&format!(
                "## Summary\n\n- Tickets: {}\n- Milestones: {}\n- Events: {}\n\n",
                report.total_tickets, report.total_milestones, report.total_events
            ));
            if !report.all_warnings.is_empty() {
                out.push_str("## Warnings\n\n");
                for w in &report.all_warnings {
                    out.push_str(&format!("- {w}\n"));
                }
                out.push('\n');
            }
            if !report.all_errors.is_empty() {
                out.push_str("## Errors\n\n");
                for e in &report.all_errors {
                    out.push_str(&format!("- {e}\n"));
                }
            }
            Ok(out)
        }
        OutputFormat::Csv => {
            let warnings = serde_json::to_string(&report.all_warnings)
                .map_err(|err| TikError::internal(&format!("csv warnings json: {err}")))?;
            let errors = serde_json::to_string(&report.all_errors)
                .map_err(|err| TikError::internal(&format!("csv errors json: {err}")))?;
            csv_write(
                &[
                    "from_version",
                    "to_version",
                    "success",
                    "total_tickets",
                    "total_milestones",
                    "total_events",
                    "warnings_json",
                    "errors_json",
                ],
                vec![vec![
                    report.from_version.clone(),
                    report.to_version.clone(),
                    report.success.to_string(),
                    report.total_tickets.to_string(),
                    report.total_milestones.to_string(),
                    report.total_events.to_string(),
                    warnings,
                    errors,
                ]],
            )
        }
    }
}

fn render_index_summary(format: OutputFormat, summary: &IndexSummary) -> Result<String> {
    match format {
        OutputFormat::Table => render_index_summary_table(summary),
        OutputFormat::Compact => Ok(render_index_summary_compact(summary)),
        OutputFormat::Json => serde_json::to_string_pretty(summary)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(summary)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(summary)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_index_summary_md(summary)),
        OutputFormat::Csv => render_index_summary_csv(summary),
    }
}

fn render_import_summary(format: OutputFormat, summary: &ImportSummary) -> Result<String> {
    match format {
        OutputFormat::Table => render_import_summary_table(summary),
        OutputFormat::Compact => Ok(render_import_summary_compact(summary)),
        OutputFormat::Json => serde_json::to_string_pretty(summary)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(summary)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(summary)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_import_summary_md(summary)),
        OutputFormat::Csv => render_import_summary_csv(summary),
    }
}

fn render_stats(format: OutputFormat, stats: &Stats) -> Result<String> {
    match format {
        OutputFormat::Table => render_stats_table(stats),
        OutputFormat::Compact => Ok(render_stats_compact(stats)),
        OutputFormat::Json => serde_json::to_string_pretty(stats)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_stats_jsonl(stats),
        OutputFormat::Yaml => serde_yaml::to_string(stats)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_stats_md(stats)),
        OutputFormat::Csv => render_stats_csv(stats),
    }
}

fn render_report(format: OutputFormat, report: &Report) -> Result<String> {
    match format {
        OutputFormat::Table => render_report_table(report),
        OutputFormat::Compact => Ok(render_report_compact(report)),
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_report_jsonl(report),
        OutputFormat::Yaml => serde_yaml::to_string(report)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_report_md(report)),
        OutputFormat::Csv => render_report_csv(report),
    }
}

fn render_burndown(format: OutputFormat, report: &BurndownReport) -> Result<String> {
    match format {
        OutputFormat::Table => render_burndown_table(report),
        OutputFormat::Compact => Ok(render_burndown_compact(report)),
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_burndown_jsonl(report),
        OutputFormat::Yaml => serde_yaml::to_string(report)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_burndown_md(report)),
        OutputFormat::Csv => render_burndown_csv(report),
    }
}

fn render_throughput(format: OutputFormat, report: &ThroughputReport) -> Result<String> {
    match format {
        OutputFormat::Table => render_throughput_table(report),
        OutputFormat::Compact => Ok(render_throughput_compact(report)),
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_throughput_jsonl(report),
        OutputFormat::Yaml => serde_yaml::to_string(report)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_throughput_md(report)),
        OutputFormat::Csv => render_throughput_csv(report),
    }
}

fn render_graph(format: OutputFormat, graph: &Graph) -> Result<String> {
    match format {
        OutputFormat::Table => render_graph_table(graph),
        OutputFormat::Compact => Ok(render_graph_compact(graph)),
        OutputFormat::Json => serde_json::to_string_pretty(graph)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_graph_jsonl(graph),
        OutputFormat::Yaml => serde_yaml::to_string(graph)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_graph_md(graph)),
        OutputFormat::Csv => render_graph_csv(graph),
    }
}

fn render_config(format: OutputFormat, config: &tik_core::Config) -> Result<String> {
    match format {
        OutputFormat::Table => render_config_table(config),
        OutputFormat::Compact => Ok(render_config_compact(config)),
        OutputFormat::Json => serde_json::to_string_pretty(config)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(config)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(config)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_config_md(config)),
        OutputFormat::Csv => render_config_csv(config),
    }
}

fn render_config_value(format: OutputFormat, key: &str, value: &str) -> Result<String> {
    #[derive(serde::Serialize)]
    struct Row<'a> {
        key: &'a str,
        value: &'a str,
    }

    let row = Row { key, value };
    match format {
        OutputFormat::Table => render_config_value_table(key, value),
        OutputFormat::Compact => Ok(format!("{key}={value}")),
        OutputFormat::Json => serde_json::to_string_pretty(&row)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(&row)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(&row)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(format!(
            "| Key | Value |\\n| --- | --- |\\n| {key} | {value} |\\n"
        )),
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(vec![]);
            writer
                .write_record(["key", "value"])
                .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
            writer
                .write_record([key, value])
                .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
            let data = writer
                .into_inner()
                .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
            String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
        }
    }
}

fn render_project(format: OutputFormat, project: &ProjectMeta) -> Result<String> {
    match format {
        OutputFormat::Table => render_project_table(project),
        OutputFormat::Compact => Ok(render_project_compact(project)),
        OutputFormat::Json => serde_json::to_string_pretty(project)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(project)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(project)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_project_md(project)),
        OutputFormat::Csv => render_project_csv(project),
    }
}

fn render_project_list(format: OutputFormat, projects: &[ProjectMeta]) -> Result<String> {
    match format {
        OutputFormat::Table => render_project_list_table(projects),
        OutputFormat::Compact => Ok(render_project_list_compact(projects)),
        OutputFormat::Json => serde_json::to_string_pretty(projects)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_jsonl(projects),
        OutputFormat::Yaml => serde_yaml::to_string(projects)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_project_list_md(projects)),
        OutputFormat::Csv => render_project_list_csv(projects),
    }
}

fn render_milestone(format: OutputFormat, milestone: &Milestone) -> Result<String> {
    match format {
        OutputFormat::Table => render_milestone_table(milestone),
        OutputFormat::Compact => Ok(render_milestone_compact(milestone)),
        OutputFormat::Json => serde_json::to_string_pretty(milestone)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(milestone)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(milestone)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_milestone_md(milestone)),
        OutputFormat::Csv => render_milestone_csv(milestone),
    }
}

fn render_milestone_list(format: OutputFormat, milestones: &[Milestone]) -> Result<String> {
    match format {
        OutputFormat::Table => render_milestone_list_table(milestones),
        OutputFormat::Compact => Ok(render_milestone_list_compact(milestones)),
        OutputFormat::Json => serde_json::to_string_pretty(milestones)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_jsonl(milestones),
        OutputFormat::Yaml => serde_yaml::to_string(milestones)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_milestone_list_md(milestones)),
        OutputFormat::Csv => render_milestone_list_csv(milestones),
    }
}

fn render_ticket(format: OutputFormat, ticket: &Ticket) -> Result<String> {
    match format {
        OutputFormat::Table => render_ticket_table(ticket),
        OutputFormat::Compact => Ok(render_ticket_compact(ticket)),
        OutputFormat::Json => serde_json::to_string_pretty(ticket)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(ticket)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(ticket)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_ticket_md(ticket)),
        OutputFormat::Csv => render_ticket_csv(ticket),
    }
}

fn render_ticket_list(format: OutputFormat, tickets: &[Ticket]) -> Result<String> {
    match format {
        OutputFormat::Table => render_ticket_list_table(tickets),
        OutputFormat::Compact => Ok(render_ticket_list_compact(tickets)),
        OutputFormat::Json => serde_json::to_string_pretty(tickets)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_jsonl(tickets),
        OutputFormat::Yaml => serde_yaml::to_string(tickets)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_ticket_list_md(tickets)),
        OutputFormat::Csv => render_ticket_list_csv(tickets),
    }
}

fn render_event(format: OutputFormat, event: &Event) -> Result<String> {
    match format {
        OutputFormat::Table => render_event_table(event),
        OutputFormat::Compact => Ok(render_event_compact(event)),
        OutputFormat::Json => serde_json::to_string_pretty(event)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => serde_json::to_string(event)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}"))),
        OutputFormat::Yaml => serde_yaml::to_string(event)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_event_md(event)),
        OutputFormat::Csv => render_event_csv(event),
    }
}

fn render_event_list(format: OutputFormat, events: &[Event]) -> Result<String> {
    match format {
        OutputFormat::Table => render_event_list_table(events),
        OutputFormat::Compact => Ok(render_event_list_compact(events)),
        OutputFormat::Json => serde_json::to_string_pretty(events)
            .map_err(|err| TikError::internal(&format!("json render: {err}"))),
        OutputFormat::Jsonl => render_jsonl(events),
        OutputFormat::Yaml => serde_yaml::to_string(events)
            .map_err(|err| TikError::internal(&format!("yaml render: {err}"))),
        OutputFormat::Md => Ok(render_event_list_md(events)),
        OutputFormat::Csv => render_event_list_csv(events),
    }
}

fn render_status_table(status: &RepoStatus) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "repo_root",
            value: status.repo_root.clone(),
        },
        Row {
            key: "tik_root",
            value: status.tik_root.clone(),
        },
        Row {
            key: "project",
            value: status.project.clone(),
        },
        Row {
            key: "project_root",
            value: status.project_root.clone(),
        },
        Row {
            key: "schema_version",
            value: status.schema_version.clone(),
        },
        Row {
            key: "config_version",
            value: status.config_version.clone(),
        },
        Row {
            key: "layout_version",
            value: status.layout_version.clone(),
        },
        Row {
            key: "index_present",
            value: status.index_present.to_string(),
        },
        Row {
            key: "ticket_count",
            value: status.ticket_count.to_string(),
        },
        Row {
            key: "milestone_count",
            value: status.milestone_count.to_string(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_doctor_table(report: &DoctorReport) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        status: String,
        scope: String,
        id: String,
        message: String,
        details: String,
        hint: String,
    }

    let rows: Vec<Row> = report
        .checks
        .iter()
        .map(|check| Row {
            status: check.status.as_str().to_string(),
            scope: check.scope.clone(),
            id: check.id.clone(),
            message: check.message.clone(),
            details: check.details.join("; "),
            hint: check.hint.clone().unwrap_or_default(),
        })
        .collect();

    let mut out = String::new();
    out.push_str(&format!("Generated at: {}\n", report.generated_at));
    out.push_str(&format!("Repo root: {}\n", report.repo_root));
    out.push_str(&format!("Layout version: {}\n", report.layout_version));
    out.push_str(&format!(
        "Summary: ok={} warnings={} errors={}\n\n",
        report.summary.ok, report.summary.warnings, report.summary.errors
    ));
    out.push_str(&Table::new(rows).to_string());
    Ok(out)
}

fn render_migration_summary_table(summary: &MigrationSummary) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "migrated_at",
            value: summary.migrated_at.clone(),
        },
        Row {
            key: "from_layout",
            value: summary.from_layout.clone(),
        },
        Row {
            key: "to_layout",
            value: summary.to_layout.clone(),
        },
        Row {
            key: "project",
            value: summary.project.clone(),
        },
        Row {
            key: "moves",
            value: summary.moves.len().to_string(),
        },
        Row {
            key: "created",
            value: summary.created.len().to_string(),
        },
        Row {
            key: "warnings",
            value: summary.warnings.len().to_string(),
        },
    ];

    let mut out = String::new();
    out.push_str(&Table::new(rows).to_string());
    if !summary.moves.is_empty() {
        out.push_str("\n\nMoves\n");
        #[derive(Tabled)]
        struct MoveRow {
            from: String,
            to: String,
        }
        let move_rows: Vec<MoveRow> = summary
            .moves
            .iter()
            .map(|item| MoveRow {
                from: item.from.clone(),
                to: item.to.clone(),
            })
            .collect();
        out.push_str(&Table::new(move_rows).to_string());
    }
    if !summary.created.is_empty() {
        out.push_str("\n\nCreated\n");
        for path in &summary.created {
            out.push_str(&format!("- {path}\n"));
        }
    }
    if !summary.warnings.is_empty() {
        out.push_str("\n\nWarnings\n");
        for warning in &summary.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
    }
    Ok(out)
}

fn render_index_status_table(status: &IndexStatus) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "index_present",
            value: status.index_present.to_string(),
        },
        Row {
            key: "stale",
            value: status.stale.to_string(),
        },
        Row {
            key: "indexed_at",
            value: status.indexed_at.clone().unwrap_or_default(),
        },
        Row {
            key: "ticket_count",
            value: status
                .ticket_count
                .map(|count| count.to_string())
                .unwrap_or_default(),
        },
        Row {
            key: "last_ticket_write",
            value: status.last_ticket_write.clone().unwrap_or_default(),
        },
        Row {
            key: "last_milestone_write",
            value: status.last_milestone_write.clone().unwrap_or_default(),
        },
        Row {
            key: "fts_path",
            value: status.fts_path.clone(),
        },
        Row {
            key: "tickets_jsonl_path",
            value: status.tickets_jsonl_path.clone(),
        },
        Row {
            key: "metadata_path",
            value: status.metadata_path.clone(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_index_summary_table(summary: &IndexSummary) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "indexed_at",
            value: summary.indexed_at.clone(),
        },
        Row {
            key: "ticket_count",
            value: summary.ticket_count.to_string(),
        },
        Row {
            key: "index_path",
            value: summary.index_path.clone(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_import_summary_table(summary: &ImportSummary) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "tickets_imported",
            value: summary.tickets_imported.to_string(),
        },
        Row {
            key: "milestones_imported",
            value: summary.milestones_imported.to_string(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_stats_table(stats: &Stats) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        group: String,
        key: String,
        count: usize,
    }

    let rows: Vec<Row> = stats_rows(stats)
        .into_iter()
        .map(|row| Row {
            group: row.group,
            key: row.key,
            count: row.count,
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_report_table(report: &Report) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("Generated at: {}\n\n", report.generated_at));
    out.push_str("Stats\n");
    out.push_str(&render_stats_table(&report.stats)?);
    out.push_str("\n\nRecent Tickets\n");
    out.push_str(&render_recent_tickets_table(&report.recent_tickets)?);
    out.push_str("\n\nMilestones\n");
    out.push_str(&render_milestone_summary_table(&report.milestones)?);
    Ok(out)
}

fn render_burndown_table(report: &BurndownReport) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("Generated at: {}\n", report.generated_at));
    out.push_str(&format!(
        "Range: {} -> {}\n",
        report.range.since, report.range.until
    ));
    out.push_str(&format!(
        "Group by: {}\n\n",
        report_group_by_label(report.group_by)
    ));
    out.push_str("Filters\n");
    out.push_str(&render_report_filters_table(&report.filters)?);
    out.push_str("\n\nBurndown\n");
    out.push_str(&render_burndown_points_table(&report.points)?);
    Ok(out)
}

fn render_throughput_table(report: &ThroughputReport) -> Result<String> {
    let mut out = String::new();
    out.push_str(&format!("Generated at: {}\n", report.generated_at));
    out.push_str(&format!(
        "Range: {} -> {}\n",
        report.range.since, report.range.until
    ));
    out.push_str(&format!(
        "Group by: {}\n\n",
        report_group_by_label(report.group_by)
    ));
    out.push_str("Filters\n");
    out.push_str(&render_report_filters_table(&report.filters)?);
    out.push_str("\n\nThroughput\n");
    out.push_str(&render_throughput_points_table(&report.points)?);
    Ok(out)
}

fn render_graph_table(graph: &Graph) -> Result<String> {
    let mut out = String::new();
    out.push_str("Nodes\n");
    out.push_str(&render_graph_nodes_table(graph)?);
    out.push_str("\n\nEdges\n");
    out.push_str(&render_graph_edges_table(graph)?);
    Ok(out)
}

fn render_config_table(config: &tik_core::Config) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "schema_version",
            value: config.schema_version.clone(),
        },
        Row {
            key: "output_format",
            value: config.output_format.clone(),
        },
        Row {
            key: "pager",
            value: config.pager.clone(),
        },
        Row {
            key: "timezone",
            value: config.timezone.clone(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_config_compact(config: &tik_core::Config) -> String {
    format!(
        "schema_version={} output_format={} pager={} timezone={}",
        config.schema_version, config.output_format, config.pager, config.timezone
    )
}

fn render_config_md(config: &tik_core::Config) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| schema_version | {} |\n", config.schema_version));
    out.push_str(&format!("| output_format | {} |\n", config.output_format));
    out.push_str(&format!("| pager | {} |\n", config.pager));
    out.push_str(&format!("| timezone | {} |\n", config.timezone));
    out
}

fn render_config_csv(config: &tik_core::Config) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record(["schema_version", "output_format", "pager", "timezone"])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    writer
        .write_record([
            config.schema_version.as_str(),
            config.output_format.as_str(),
            config.pager.as_str(),
            config.timezone.as_str(),
        ])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_config_value_table(key: &str, value: &str) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row<'a> {
        key: &'a str,
        value: &'a str,
    }

    let rows = vec![Row { key, value }];
    Ok(Table::new(rows).to_string())
}

fn render_project_table(project: &ProjectMeta) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "name",
            value: project.name.clone(),
        },
        Row {
            key: "description",
            value: project.description.clone(),
        },
        Row {
            key: "created_at",
            value: project.created_at.clone(),
        },
        Row {
            key: "updated_at",
            value: project.updated_at.clone(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_project_list_table(projects: &[ProjectMeta]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        name: String,
        description: String,
        updated_at: String,
    }

    let rows: Vec<Row> = projects
        .iter()
        .map(|project| Row {
            name: project.name.clone(),
            description: project.description.clone(),
            updated_at: project.updated_at.clone(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_milestone_table(milestone: &Milestone) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "id",
            value: milestone.id.to_string(),
        },
        Row {
            key: "title",
            value: milestone.title.clone(),
        },
        Row {
            key: "status",
            value: milestone.status.as_str().to_string(),
        },
        Row {
            key: "updated_at",
            value: milestone.updated_at.clone(),
        },
        Row {
            key: "due_at",
            value: milestone.due_at.clone().unwrap_or_default(),
        },
        Row {
            key: "tags",
            value: milestone.tags.join(","),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_milestone_list_table(milestones: &[Milestone]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        id: String,
        title: String,
        status: String,
        due_at: String,
        updated_at: String,
    }

    let rows: Vec<Row> = milestones
        .iter()
        .map(|milestone| Row {
            id: milestone.id.to_string(),
            title: milestone.title.clone(),
            status: milestone.status.as_str().to_string(),
            due_at: milestone.due_at.clone().unwrap_or_default(),
            updated_at: milestone.updated_at.clone(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_ticket_table(ticket: &Ticket) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "id",
            value: ticket.id.to_string(),
        },
        Row {
            key: "title",
            value: ticket.title.clone(),
        },
        Row {
            key: "status",
            value: ticket.status.as_str().to_string(),
        },
        Row {
            key: "priority",
            value: ticket.priority.as_str().to_string(),
        },
        Row {
            key: "severity",
            value: ticket.severity.as_str().to_string(),
        },
        Row {
            key: "updated_at",
            value: ticket.updated_at.clone(),
        },
        Row {
            key: "summary",
            value: ticket.summary.clone(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_ticket_list_table(tickets: &[Ticket]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        id: String,
        title: String,
        status: String,
        priority: String,
        updated_at: String,
    }

    let rows: Vec<Row> = tickets
        .iter()
        .map(|ticket| Row {
            id: ticket.id.to_string(),
            title: ticket.title.clone(),
            status: ticket.status.as_str().to_string(),
            priority: ticket.priority.as_str().to_string(),
            updated_at: ticket.updated_at.clone(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_event_table(event: &Event) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: &'static str,
        value: String,
    }

    let rows = vec![
        Row {
            key: "event_id",
            value: event.event_id.to_string(),
        },
        Row {
            key: "ts",
            value: event.ts.clone(),
        },
        Row {
            key: "actor",
            value: event.actor.clone(),
        },
        Row {
            key: "type",
            value: event.kind.clone(),
        },
        Row {
            key: "data",
            value: event.data.to_string(),
        },
    ];

    Ok(Table::new(rows).to_string())
}

fn render_event_list_table(events: &[Event]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        ts: String,
        actor: String,
        kind: String,
        data: String,
    }

    let rows: Vec<Row> = events
        .iter()
        .map(|event| Row {
            ts: event.ts.clone(),
            actor: event.actor.clone(),
            kind: event.kind.clone(),
            data: event.data.to_string(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_recent_tickets_table(tickets: &[TicketSummary]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        id: String,
        title: String,
        status: String,
        priority: String,
        created_at: String,
        updated_at: String,
        milestone_id: String,
    }

    let rows: Vec<Row> = tickets
        .iter()
        .map(|ticket| Row {
            id: ticket.id.clone(),
            title: ticket.title.clone(),
            status: ticket.status.clone(),
            priority: ticket.priority.clone(),
            created_at: ticket.created_at.clone(),
            updated_at: ticket.updated_at.clone(),
            milestone_id: ticket.milestone_id.clone().unwrap_or_default(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_burndown_points_table(points: &[tik_core::BurndownPoint]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        period_start: String,
        period_end: String,
        open_tickets: usize,
    }

    let rows: Vec<Row> = points
        .iter()
        .map(|point| Row {
            period_start: point.period_start.clone(),
            period_end: point.period_end.clone(),
            open_tickets: point.open_tickets,
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_throughput_points_table(points: &[tik_core::ThroughputPoint]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        period_start: String,
        period_end: String,
        closed_tickets: usize,
    }

    let rows: Vec<Row> = points
        .iter()
        .map(|point| Row {
            period_start: point.period_start.clone(),
            period_end: point.period_end.clone(),
            closed_tickets: point.closed_tickets,
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_milestone_summary_table(milestones: &[MilestoneSummary]) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        id: String,
        title: String,
        status: String,
        due_at: String,
        total_tickets: usize,
        open_tickets: usize,
        closed_tickets: usize,
    }

    let rows: Vec<Row> = milestones
        .iter()
        .map(|milestone| Row {
            id: milestone.id.clone(),
            title: milestone.title.clone(),
            status: milestone.status.clone(),
            due_at: milestone.due_at.clone().unwrap_or_default(),
            total_tickets: milestone.total_tickets,
            open_tickets: milestone.open_tickets,
            closed_tickets: milestone.closed_tickets,
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_report_filters_table(filters: &ReportFilters) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        key: String,
        value: String,
    }

    let rows: Vec<Row> = report_filter_rows(filters)
        .into_iter()
        .map(|(key, value)| Row { key, value })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_graph_nodes_table(graph: &Graph) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        id: String,
        title: String,
        status: String,
    }

    let rows: Vec<Row> = graph
        .nodes
        .iter()
        .map(|node| Row {
            id: node.id.clone(),
            title: node.title.clone(),
            status: node.status.clone(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

fn render_graph_edges_table(graph: &Graph) -> Result<String> {
    use tabled::Table;
    use tabled::Tabled;

    #[derive(Tabled)]
    struct Row {
        from: String,
        relation: String,
        to: String,
    }

    let rows: Vec<Row> = graph
        .edges
        .iter()
        .map(|edge| Row {
            from: edge.from.clone(),
            relation: edge.relation.clone(),
            to: edge.to.clone(),
        })
        .collect();

    Ok(Table::new(rows).to_string())
}

#[derive(Debug, Clone)]
struct StatsRow {
    group: String,
    key: String,
    count: usize,
}

fn stats_rows(stats: &Stats) -> Vec<StatsRow> {
    let mut rows = Vec::new();
    rows.push(StatsRow {
        group: "totals".to_string(),
        key: "tickets_total".to_string(),
        count: stats.tickets_total,
    });
    rows.push(StatsRow {
        group: "totals".to_string(),
        key: "milestones_total".to_string(),
        count: stats.milestones_total,
    });
    push_stats_map(&mut rows, "tickets_by_status", &stats.tickets_by_status);
    push_stats_map(&mut rows, "tickets_by_type", &stats.tickets_by_type);
    push_stats_map(&mut rows, "tickets_by_priority", &stats.tickets_by_priority);
    push_stats_map(&mut rows, "tickets_by_severity", &stats.tickets_by_severity);
    push_stats_map(&mut rows, "tickets_by_assignee", &stats.tickets_by_assignee);
    push_stats_map(&mut rows, "tickets_by_tag", &stats.tickets_by_tag);
    push_stats_map(
        &mut rows,
        "tickets_by_milestone",
        &stats.tickets_by_milestone,
    );
    push_stats_map(
        &mut rows,
        "milestones_by_status",
        &stats.milestones_by_status,
    );
    rows
}

fn push_stats_map(rows: &mut Vec<StatsRow>, group: &str, map: &BTreeMap<String, usize>) {
    for (key, count) in map {
        rows.push(StatsRow {
            group: group.to_string(),
            key: key.clone(),
            count: *count,
        });
    }
}

fn report_group_by_label(group_by: ReportGroupBy) -> &'static str {
    match group_by {
        ReportGroupBy::Day => "day",
        ReportGroupBy::Week => "week",
        ReportGroupBy::Month => "month",
    }
}

fn report_filter_rows(filters: &ReportFilters) -> Vec<(String, String)> {
    vec![
        ("status".to_string(), format_filter_list(&filters.status)),
        ("tag".to_string(), format_filter_list(&filters.tags)),
        (
            "assignee".to_string(),
            format_filter_list(&filters.assignees),
        ),
        (
            "milestone".to_string(),
            format_filter_list(&filters.milestone_ids),
        ),
        (
            "since".to_string(),
            filters.since.clone().unwrap_or_else(|| "any".to_string()),
        ),
        (
            "until".to_string(),
            filters.until.clone().unwrap_or_else(|| "any".to_string()),
        ),
    ]
}

fn format_filter_list(values: &[String]) -> String {
    if values.is_empty() {
        "any".to_string()
    } else {
        values.join(",")
    }
}

fn graph_node_shape(id: &str) -> &'static str {
    if id.starts_with("M-") {
        "box"
    } else {
        "ellipse"
    }
}

fn escape_dot_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\"', "\\\"")
}

fn format_compact_map(label: &str, map: &BTreeMap<String, usize>) -> String {
    let values = map
        .iter()
        .map(|(key, count)| format!("{key}={count}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{label}=[{values}]")
}

fn render_status_compact(status: &RepoStatus) -> String {
    format!(
        "repo_root={} tik_root={} project={} project_root={} schema_version={} config_version={} layout_version={} index_present={} ticket_count={} milestone_count={}",
        status.repo_root,
        status.tik_root,
        status.project,
        status.project_root,
        status.schema_version,
        status.config_version,
        status.layout_version,
        status.index_present,
        status.ticket_count,
        status.milestone_count
    )
}

fn render_doctor_compact(report: &DoctorReport) -> String {
    let mut out = format!(
        "ok={} warnings={} errors={}",
        report.summary.ok, report.summary.warnings, report.summary.errors
    );
    for check in &report.checks {
        let mut line = format!(
            "{} {} {}",
            check.status.as_str(),
            check.scope,
            check.message
        );
        if !check.details.is_empty() {
            line.push_str(&format!(" [{}]", check.details.join("; ")));
        }
        if let Some(hint) = &check.hint {
            line.push_str(&format!(" hint={hint}"));
        }
        out.push('\n');
        out.push_str(&line);
    }
    out
}

fn render_migration_summary_compact(summary: &MigrationSummary) -> String {
    let mut out = format!(
        "migrated_at={} from_layout={} to_layout={} project={} moves={} created={} warnings={}",
        summary.migrated_at,
        summary.from_layout,
        summary.to_layout,
        summary.project,
        summary.moves.len(),
        summary.created.len(),
        summary.warnings.len()
    );
    for item in &summary.moves {
        out.push_str(&format!("\nmove {} -> {}", item.from, item.to));
    }
    for path in &summary.created {
        out.push_str(&format!("\ncreated {path}"));
    }
    for warning in &summary.warnings {
        out.push_str(&format!("\nwarning {warning}"));
    }
    out
}

fn render_index_status_compact(status: &IndexStatus) -> String {
    format!(
        "index_present={} stale={} indexed_at={} ticket_count={} last_ticket_write={} last_milestone_write={}",
        status.index_present,
        status.stale,
        status.indexed_at.clone().unwrap_or_default(),
        status
            .ticket_count
            .map(|count| count.to_string())
            .unwrap_or_default(),
        status.last_ticket_write.clone().unwrap_or_default(),
        status.last_milestone_write.clone().unwrap_or_default()
    )
}

fn render_project_compact(project: &ProjectMeta) -> String {
    if project.description.is_empty() {
        format!("{} {}", project.name, project.updated_at)
    } else {
        format!(
            "{} {} {}",
            project.name, project.updated_at, project.description
        )
    }
}

fn render_project_list_compact(projects: &[ProjectMeta]) -> String {
    projects
        .iter()
        .map(render_project_compact)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_index_summary_compact(summary: &IndexSummary) -> String {
    format!(
        "indexed_at={} ticket_count={} index_path={}",
        summary.indexed_at, summary.ticket_count, summary.index_path
    )
}

fn render_import_summary_compact(summary: &ImportSummary) -> String {
    format!(
        "tickets_imported={} milestones_imported={}",
        summary.tickets_imported, summary.milestones_imported
    )
}

fn render_stats_compact(stats: &Stats) -> String {
    let mut parts = vec![
        format!("tickets_total={}", stats.tickets_total),
        format!("milestones_total={}", stats.milestones_total),
    ];
    parts.push(format_compact_map(
        "tickets_by_status",
        &stats.tickets_by_status,
    ));
    parts.push(format_compact_map(
        "tickets_by_type",
        &stats.tickets_by_type,
    ));
    parts.push(format_compact_map(
        "tickets_by_priority",
        &stats.tickets_by_priority,
    ));
    parts.push(format_compact_map(
        "tickets_by_severity",
        &stats.tickets_by_severity,
    ));
    parts.push(format_compact_map(
        "tickets_by_assignee",
        &stats.tickets_by_assignee,
    ));
    parts.push(format_compact_map("tickets_by_tag", &stats.tickets_by_tag));
    parts.push(format_compact_map(
        "tickets_by_milestone",
        &stats.tickets_by_milestone,
    ));
    parts.push(format_compact_map(
        "milestones_by_status",
        &stats.milestones_by_status,
    ));
    parts.join(" ")
}

fn render_report_compact(report: &Report) -> String {
    format!(
        "generated_at={} {} recent_tickets={} milestones={}",
        report.generated_at,
        render_stats_compact(&report.stats),
        report.recent_tickets.len(),
        report.milestones.len()
    )
}

fn render_burndown_compact(report: &BurndownReport) -> String {
    let mut out = format!(
        "generated_at={} group_by={} range={}..{} {} points={}",
        report.generated_at,
        report_group_by_label(report.group_by),
        report.range.since,
        report.range.until,
        render_report_filters_compact(&report.filters),
        report.points.len()
    );
    if !report.points.is_empty() {
        let lines = report
            .points
            .iter()
            .map(|point| {
                format!(
                    "{}..{} open={}",
                    point.period_start, point.period_end, point.open_tickets
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out.push_str(&lines);
    }
    out
}

fn render_throughput_compact(report: &ThroughputReport) -> String {
    let mut out = format!(
        "generated_at={} group_by={} range={}..{} {} points={}",
        report.generated_at,
        report_group_by_label(report.group_by),
        report.range.since,
        report.range.until,
        render_report_filters_compact(&report.filters),
        report.points.len()
    );
    if !report.points.is_empty() {
        let lines = report
            .points
            .iter()
            .map(|point| {
                format!(
                    "{}..{} closed={}",
                    point.period_start, point.period_end, point.closed_tickets
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out.push_str(&lines);
    }
    out
}

fn render_graph_compact(graph: &Graph) -> String {
    let mut out = format!("nodes={} edges={}", graph.nodes.len(), graph.edges.len());
    if !graph.edges.is_empty() {
        let edges = graph
            .edges
            .iter()
            .map(|edge| format!("{} {} {}", edge.from, edge.relation, edge.to))
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out.push_str(&edges);
    }
    out
}

fn render_milestone_compact(milestone: &Milestone) -> String {
    format!(
        "{} [{}] {}",
        milestone.id,
        milestone.status.as_str(),
        milestone.title
    )
}

fn render_milestone_list_compact(milestones: &[Milestone]) -> String {
    milestones
        .iter()
        .map(render_milestone_compact)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_report_filters_compact(filters: &ReportFilters) -> String {
    report_filter_rows(filters)
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_ticket_compact(ticket: &Ticket) -> String {
    format!(
        "{} [{}] {}",
        ticket.id,
        ticket.status.as_str(),
        ticket.title
    )
}

fn render_ticket_list_compact(tickets: &[Ticket]) -> String {
    tickets
        .iter()
        .map(render_ticket_compact)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_event_compact(event: &Event) -> String {
    format!("{} {} {} {}", event.ts, event.actor, event.kind, event.data)
}

fn render_event_list_compact(events: &[Event]) -> String {
    events
        .iter()
        .map(render_event_compact)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_status_md(status: &RepoStatus) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| repo_root | {} |\n", status.repo_root));
    out.push_str(&format!("| tik_root | {} |\n", status.tik_root));
    out.push_str(&format!("| project | {} |\n", status.project));
    out.push_str(&format!("| project_root | {} |\n", status.project_root));
    out.push_str(&format!("| schema_version | {} |\n", status.schema_version));
    out.push_str(&format!("| config_version | {} |\n", status.config_version));
    out.push_str(&format!("| layout_version | {} |\n", status.layout_version));
    out.push_str(&format!("| index_present | {} |\n", status.index_present));
    out.push_str(&format!("| ticket_count | {} |\n", status.ticket_count));
    out.push_str(&format!(
        "| milestone_count | {} |\n",
        status.milestone_count
    ));
    out
}

fn render_doctor_md(report: &DoctorReport) -> String {
    let mut out = String::new();
    out.push_str("| Metric | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| generated_at | {} |\n", report.generated_at));
    out.push_str(&format!("| repo_root | {} |\n", report.repo_root));
    out.push_str(&format!("| layout_version | {} |\n", report.layout_version));
    out.push_str(&format!("| ok | {} |\n", report.summary.ok));
    out.push_str(&format!("| warnings | {} |\n", report.summary.warnings));
    out.push_str(&format!("| errors | {} |\n", report.summary.errors));
    out.push_str("\n| Status | Scope | Id | Message | Details | Hint |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for check in &report.checks {
        let details = check.details.join("; ");
        let hint = check.hint.clone().unwrap_or_default();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            check.status.as_str(),
            check.scope,
            check.id,
            check.message,
            details,
            hint
        ));
    }
    out
}

fn render_migration_summary_md(summary: &MigrationSummary) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| migrated_at | {} |\n", summary.migrated_at));
    out.push_str(&format!("| from_layout | {} |\n", summary.from_layout));
    out.push_str(&format!("| to_layout | {} |\n", summary.to_layout));
    out.push_str(&format!("| project | {} |\n", summary.project));
    out.push_str(&format!("| moves | {} |\n", summary.moves.len()));
    out.push_str(&format!("| created | {} |\n", summary.created.len()));
    out.push_str(&format!("| warnings | {} |\n", summary.warnings.len()));
    if !summary.moves.is_empty() {
        out.push_str("\n| From | To |\n");
        out.push_str("| --- | --- |\n");
        for item in &summary.moves {
            out.push_str(&format!("| {} | {} |\n", item.from, item.to));
        }
    }
    if !summary.created.is_empty() {
        out.push_str("\nCreated\n");
        for path in &summary.created {
            out.push_str(&format!("- {path}\n"));
        }
    }
    if !summary.warnings.is_empty() {
        out.push_str("\nWarnings\n");
        for warning in &summary.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
    }
    out
}

fn render_index_status_md(status: &IndexStatus) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| index_present | {} |\n", status.index_present));
    out.push_str(&format!("| stale | {} |\n", status.stale));
    out.push_str(&format!(
        "| indexed_at | {} |\n",
        status.indexed_at.clone().unwrap_or_default()
    ));
    out.push_str(&format!(
        "| ticket_count | {} |\n",
        status
            .ticket_count
            .map(|count| count.to_string())
            .unwrap_or_default()
    ));
    out.push_str(&format!(
        "| last_ticket_write | {} |\n",
        status.last_ticket_write.clone().unwrap_or_default()
    ));
    out.push_str(&format!(
        "| last_milestone_write | {} |\n",
        status.last_milestone_write.clone().unwrap_or_default()
    ));
    out.push_str(&format!("| fts_path | {} |\n", status.fts_path));
    out.push_str(&format!(
        "| tickets_jsonl_path | {} |\n",
        status.tickets_jsonl_path
    ));
    out.push_str(&format!("| metadata_path | {} |\n", status.metadata_path));
    out
}

fn render_project_md(project: &ProjectMeta) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| name | {} |\n", project.name));
    out.push_str(&format!("| description | {} |\n", project.description));
    out.push_str(&format!("| created_at | {} |\n", project.created_at));
    out.push_str(&format!("| updated_at | {} |\n", project.updated_at));
    out
}

fn render_project_list_md(projects: &[ProjectMeta]) -> String {
    let mut out = String::new();
    out.push_str("| Name | Description | Updated At |\n");
    out.push_str("| --- | --- | --- |\n");
    for project in projects {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            project.name, project.description, project.updated_at
        ));
    }
    out
}

fn render_index_summary_md(summary: &IndexSummary) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!("| indexed_at | {} |\n", summary.indexed_at));
    out.push_str(&format!("| ticket_count | {} |\n", summary.ticket_count));
    out.push_str(&format!("| index_path | {} |\n", summary.index_path));
    out
}

fn render_import_summary_md(summary: &ImportSummary) -> String {
    let mut out = String::new();
    out.push_str("| Key | Value |\n");
    out.push_str("| --- | --- |\n");
    out.push_str(&format!(
        "| tickets_imported | {} |\n",
        summary.tickets_imported
    ));
    out.push_str(&format!(
        "| milestones_imported | {} |\n",
        summary.milestones_imported
    ));
    out
}

fn render_stats_md(stats: &Stats) -> String {
    let mut out = String::new();
    out.push_str("| Group | Key | Count |\n");
    out.push_str("| --- | --- | --- |\n");
    for row in stats_rows(stats) {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            row.group, row.key, row.count
        ));
    }
    out
}

fn render_report_md(report: &Report) -> String {
    let mut out = String::new();
    out.push_str("# Report\n\n");
    out.push_str(&format!("- generated_at: {}\n\n", report.generated_at));
    out.push_str("## Stats\n\n");
    out.push_str(&render_stats_md(&report.stats));
    out.push_str("\n\n## Recent Tickets\n\n");
    out.push_str(&render_recent_tickets_md(&report.recent_tickets));
    out.push_str("\n\n## Milestones\n\n");
    out.push_str(&render_milestone_summary_md(&report.milestones));
    out
}

fn render_burndown_md(report: &BurndownReport) -> String {
    let mut out = String::new();
    out.push_str("# Burndown\n\n");
    out.push_str(&format!("- generated_at: {}\n", report.generated_at));
    out.push_str(&format!(
        "- range: {} -> {}\n",
        report.range.since, report.range.until
    ));
    out.push_str(&format!(
        "- group_by: {}\n\n",
        report_group_by_label(report.group_by)
    ));
    out.push_str("## Filters\n\n");
    out.push_str(&render_report_filters_md(&report.filters));
    out.push_str("\n\n## Points\n\n");
    out.push_str("| Period Start | Period End | Open Tickets |\n");
    out.push_str("| --- | --- | --- |\n");
    for point in &report.points {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            point.period_start, point.period_end, point.open_tickets
        ));
    }
    out
}

fn render_throughput_md(report: &ThroughputReport) -> String {
    let mut out = String::new();
    out.push_str("# Throughput\n\n");
    out.push_str(&format!("- generated_at: {}\n", report.generated_at));
    out.push_str(&format!(
        "- range: {} -> {}\n",
        report.range.since, report.range.until
    ));
    out.push_str(&format!(
        "- group_by: {}\n\n",
        report_group_by_label(report.group_by)
    ));
    out.push_str("## Filters\n\n");
    out.push_str(&render_report_filters_md(&report.filters));
    out.push_str("\n\n## Points\n\n");
    out.push_str("| Period Start | Period End | Closed Tickets |\n");
    out.push_str("| --- | --- | --- |\n");
    for point in &report.points {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            point.period_start, point.period_end, point.closed_tickets
        ));
    }
    out
}

fn render_graph_md(graph: &Graph) -> String {
    let mut out = String::new();
    out.push_str("# Graph\n\n");
    out.push_str(&format!(
        "- nodes: {}\n- edges: {}\n\n",
        graph.nodes.len(),
        graph.edges.len()
    ));
    out.push_str("```mermaid\n");
    out.push_str("graph TD\n");
    out.push_str(&render_mermaid_nodes(graph));
    out.push_str(&render_mermaid_edges(graph));
    out.push_str("```\n\n");
    out.push_str("## Nodes\n\n");
    out.push_str(&render_graph_nodes_md(graph));
    out.push_str("\n\n## Edges\n\n");
    out.push_str(&render_graph_edges_md(graph));
    out
}

fn render_graph_dot(graph: &Graph) -> String {
    let mut out = String::new();
    out.push_str("digraph tik {\n");
    for node in &graph.nodes {
        let label = format!(
            "{}\\n{}\\n{}",
            node.id,
            escape_dot_label(&node.title),
            node.status
        );
        out.push_str(&format!(
            "  \"{}\" [label=\"{}\", shape=\"{}\"];\n",
            node.id,
            label,
            graph_node_shape(&node.id)
        ));
    }
    for edge in &graph.edges {
        out.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{}\"];\n",
            edge.from,
            edge.to,
            escape_dot_label(&edge.relation)
        ));
    }
    out.push_str("}\n");
    out
}

fn render_milestone_md(milestone: &Milestone) -> String {
    format!(
        "# {} {}\n\n- status: {}\n- updated_at: {}\n- due_at: {}\n\n{}\n",
        milestone.id,
        milestone.title,
        milestone.status.as_str(),
        milestone.updated_at,
        milestone.due_at.clone().unwrap_or_default(),
        milestone.description
    )
}

fn render_milestone_list_md(milestones: &[Milestone]) -> String {
    let mut out = String::new();
    out.push_str("| ID | Title | Status | Due | Updated |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for milestone in milestones {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            milestone.id,
            milestone.title,
            milestone.status.as_str(),
            milestone.due_at.clone().unwrap_or_default(),
            milestone.updated_at
        ));
    }
    out
}

fn render_report_filters_md(filters: &ReportFilters) -> String {
    let mut out = String::new();
    out.push_str("| Filter | Value |\n");
    out.push_str("| --- | --- |\n");
    for (key, value) in report_filter_rows(filters) {
        out.push_str(&format!("| {key} | {value} |\n"));
    }
    out
}

fn render_ticket_md(ticket: &Ticket) -> String {
    format!(
        "# {} {}\n\n- status: {}\n- priority: {}\n- severity: {}\n- updated_at: {}\n\n{}\n",
        ticket.id,
        ticket.title,
        ticket.status.as_str(),
        ticket.priority.as_str(),
        ticket.severity.as_str(),
        ticket.updated_at,
        ticket.summary
    )
}

fn render_ticket_list_md(tickets: &[Ticket]) -> String {
    let mut out = String::new();
    out.push_str("| ID | Title | Status | Priority | Updated |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for ticket in tickets {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            ticket.id,
            ticket.title,
            ticket.status.as_str(),
            ticket.priority.as_str(),
            ticket.updated_at
        ));
    }
    out
}

fn render_event_md(event: &Event) -> String {
    format!(
        "- {} {} {} {}",
        event.ts, event.actor, event.kind, event.data
    )
}

fn render_event_list_md(events: &[Event]) -> String {
    events
        .iter()
        .map(render_event_md)
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_recent_tickets_md(tickets: &[TicketSummary]) -> String {
    let mut out = String::new();
    out.push_str("| ID | Title | Status | Priority | Created | Updated | Milestone |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for ticket in tickets {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            ticket.id,
            ticket.title,
            ticket.status,
            ticket.priority,
            ticket.created_at,
            ticket.updated_at,
            ticket.milestone_id.clone().unwrap_or_default()
        ));
    }
    out
}

fn render_milestone_summary_md(milestones: &[MilestoneSummary]) -> String {
    let mut out = String::new();
    out.push_str("| ID | Title | Status | Due | Total | Open | Closed |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");
    for milestone in milestones {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            milestone.id,
            milestone.title,
            milestone.status,
            milestone.due_at.clone().unwrap_or_default(),
            milestone.total_tickets,
            milestone.open_tickets,
            milestone.closed_tickets
        ));
    }
    out
}

fn render_graph_nodes_md(graph: &Graph) -> String {
    let mut out = String::new();
    out.push_str("| ID | Title | Status |\n");
    out.push_str("| --- | --- | --- |\n");
    for node in &graph.nodes {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            node.id, node.title, node.status
        ));
    }
    out
}

fn render_graph_edges_md(graph: &Graph) -> String {
    let mut out = String::new();
    out.push_str("| From | Relation | To |\n");
    out.push_str("| --- | --- | --- |\n");
    for edge in &graph.edges {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            edge.from, edge.relation, edge.to
        ));
    }
    out
}

fn render_mermaid_nodes(graph: &Graph) -> String {
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    for node in &graph.nodes {
        labels.insert(node.id.clone(), node.title.clone());
    }
    for edge in &graph.edges {
        labels
            .entry(edge.from.clone())
            .or_insert_with(|| edge.from.clone());
        labels
            .entry(edge.to.clone())
            .or_insert_with(|| edge.to.clone());
    }
    let mut out = String::new();
    for (id, label) in labels {
        let node_id = mermaid_node_id(&id);
        let label = escape_mermaid_label(&label);
        out.push_str(&format!("  {node_id}[\"{label}\"]\n"));
    }
    out
}

fn render_mermaid_edges(graph: &Graph) -> String {
    let mut out = String::new();
    for edge in &graph.edges {
        let from = mermaid_node_id(&edge.from);
        let to = mermaid_node_id(&edge.to);
        out.push_str(&format!("  {from} --{}--> {to}\n", edge.relation));
    }
    out
}

fn mermaid_node_id(raw: &str) -> String {
    let mut out = String::from("N_");
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn escape_mermaid_label(raw: &str) -> String {
    raw.replace('"', "\\\"").replace('\n', " ")
}

fn render_status_csv(status: &RepoStatus) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .serialize(status)
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_doctor_csv(report: &DoctorReport) -> Result<String> {
    let mut rows = Vec::new();
    for check in &report.checks {
        let details = serde_json::to_string(&check.details)
            .map_err(|err| TikError::internal(&format!("csv details json: {err}")))?;
        rows.push(vec![
            check.status.as_str().to_string(),
            check.scope.clone(),
            check.id.clone(),
            check.message.clone(),
            details,
            check.hint.clone().unwrap_or_default(),
        ]);
    }
    csv_write(
        &["status", "scope", "id", "message", "details_json", "hint"],
        rows,
    )
}

fn render_migration_summary_csv(summary: &MigrationSummary) -> Result<String> {
    let moves = serde_json::to_string(&summary.moves)
        .map_err(|err| TikError::internal(&format!("csv moves json: {err}")))?;
    let created = serde_json::to_string(&summary.created)
        .map_err(|err| TikError::internal(&format!("csv created json: {err}")))?;
    let warnings = serde_json::to_string(&summary.warnings)
        .map_err(|err| TikError::internal(&format!("csv warnings json: {err}")))?;
    let row = vec![
        summary.migrated_at.clone(),
        summary.from_layout.clone(),
        summary.to_layout.clone(),
        summary.project.clone(),
        moves,
        created,
        warnings,
    ];
    csv_write(
        &[
            "migrated_at",
            "from_layout",
            "to_layout",
            "project",
            "moves_json",
            "created_json",
            "warnings_json",
        ],
        vec![row],
    )
}

fn render_index_status_csv(status: &IndexStatus) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .serialize(status)
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_index_summary_csv(summary: &IndexSummary) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .serialize(summary)
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_import_summary_csv(summary: &ImportSummary) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .serialize(summary)
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_stats_csv(stats: &Stats) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record(["group", "key", "count"])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    for row in stats_rows(stats) {
        writer
            .write_record([row.group, row.key, row.count.to_string()])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_report_csv(report: &Report) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record([
            "section",
            "group",
            "key",
            "count",
            "id",
            "title",
            "status",
            "priority",
            "created_at",
            "updated_at",
            "milestone_id",
            "due_at",
            "total_tickets",
            "open_tickets",
            "closed_tickets",
        ])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;

    writer
        .write_record(vec![
            "meta".to_string(),
            String::new(),
            "generated_at".to_string(),
            report.generated_at.clone(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;

    for row in stats_rows(&report.stats) {
        let count = row.count.to_string();
        writer
            .write_record(vec![
                "stats".to_string(),
                row.group,
                row.key,
                count,
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    for ticket in &report.recent_tickets {
        writer
            .write_record(vec![
                "recent_ticket".to_string(),
                String::new(),
                String::new(),
                String::new(),
                ticket.id.clone(),
                ticket.title.clone(),
                ticket.status.clone(),
                ticket.priority.clone(),
                ticket.created_at.clone(),
                ticket.updated_at.clone(),
                ticket.milestone_id.clone().unwrap_or_default(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    for milestone in &report.milestones {
        let total = milestone.total_tickets.to_string();
        let open = milestone.open_tickets.to_string();
        let closed = milestone.closed_tickets.to_string();
        writer
            .write_record(vec![
                "milestone".to_string(),
                String::new(),
                String::new(),
                String::new(),
                milestone.id.clone(),
                milestone.title.clone(),
                milestone.status.clone(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                milestone.due_at.clone().unwrap_or_default(),
                total,
                open,
                closed,
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_burndown_csv(report: &BurndownReport) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record([
            "period_start",
            "period_end",
            "open_tickets",
            "generated_at",
            "group_by",
            "since",
            "until",
            "status",
            "tags",
            "assignees",
            "milestones",
        ])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;

    let status = format_filter_list(&report.filters.status);
    let tags = format_filter_list(&report.filters.tags);
    let assignees = format_filter_list(&report.filters.assignees);
    let milestones = format_filter_list(&report.filters.milestone_ids);
    let group_by = report_group_by_label(report.group_by);

    for point in &report.points {
        let open_tickets = point.open_tickets.to_string();
        writer
            .write_record(vec![
                point.period_start.clone(),
                point.period_end.clone(),
                open_tickets,
                report.generated_at.clone(),
                group_by.to_string(),
                report.range.since.clone(),
                report.range.until.clone(),
                status.clone(),
                tags.clone(),
                assignees.clone(),
                milestones.clone(),
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_throughput_csv(report: &ThroughputReport) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record([
            "period_start",
            "period_end",
            "closed_tickets",
            "generated_at",
            "group_by",
            "since",
            "until",
            "status",
            "tags",
            "assignees",
            "milestones",
        ])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;

    let status = format_filter_list(&report.filters.status);
    let tags = format_filter_list(&report.filters.tags);
    let assignees = format_filter_list(&report.filters.assignees);
    let milestones = format_filter_list(&report.filters.milestone_ids);
    let group_by = report_group_by_label(report.group_by);

    for point in &report.points {
        let closed_tickets = point.closed_tickets.to_string();
        writer
            .write_record(vec![
                point.period_start.clone(),
                point.period_end.clone(),
                closed_tickets,
                report.generated_at.clone(),
                group_by.to_string(),
                report.range.since.clone(),
                report.range.until.clone(),
                status.clone(),
                tags.clone(),
                assignees.clone(),
                milestones.clone(),
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_graph_csv(graph: &Graph) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record(["type", "id", "title", "status", "from", "to", "relation"])
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;

    for node in &graph.nodes {
        writer
            .write_record([
                "node",
                node.id.as_str(),
                node.title.as_str(),
                node.status.as_str(),
                "",
                "",
                "",
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    for edge in &graph.edges {
        writer
            .write_record([
                "edge",
                "",
                "",
                "",
                edge.from.as_str(),
                edge.to.as_str(),
                edge.relation.as_str(),
            ])
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }

    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

fn render_milestone_csv(milestone: &Milestone) -> Result<String> {
    let row = milestone_csv_row(milestone)?;
    csv_write(&milestone_csv_headers(), vec![row])
}

fn render_project_csv(project: &ProjectMeta) -> Result<String> {
    let row = project_csv_row(project)?;
    csv_write(&project_csv_headers(), vec![row])
}

fn render_project_list_csv(projects: &[ProjectMeta]) -> Result<String> {
    let mut rows = Vec::new();
    for project in projects {
        rows.push(project_csv_row(project)?);
    }
    csv_write(&project_csv_headers(), rows)
}

fn render_milestone_list_csv(milestones: &[Milestone]) -> Result<String> {
    let mut rows = Vec::new();
    for milestone in milestones {
        rows.push(milestone_csv_row(milestone)?);
    }
    csv_write(&milestone_csv_headers(), rows)
}

fn render_ticket_csv(ticket: &Ticket) -> Result<String> {
    let row = ticket_csv_row(ticket)?;
    csv_write(&ticket_csv_headers(), vec![row])
}

fn render_ticket_list_csv(tickets: &[Ticket]) -> Result<String> {
    let mut rows = Vec::new();
    for ticket in tickets {
        rows.push(ticket_csv_row(ticket)?);
    }
    csv_write(&ticket_csv_headers(), rows)
}

fn render_event_csv(event: &Event) -> Result<String> {
    let row = event_csv_row(event)?;
    csv_write(&event_csv_headers(), vec![row])
}

fn render_event_list_csv(events: &[Event]) -> Result<String> {
    let mut rows = Vec::new();
    for event in events {
        rows.push(event_csv_row(event)?);
    }
    csv_write(&event_csv_headers(), rows)
}

fn render_stats_jsonl(stats: &Stats) -> Result<String> {
    let mut lines = Vec::new();
    for row in stats_rows(stats) {
        let value = serde_json::json!({
            "type": "stat",
            "group": row.group,
            "key": row.key,
            "count": row.count,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn render_report_jsonl(report: &Report) -> Result<String> {
    let mut lines = Vec::new();
    let meta = serde_json::json!({
        "type": "report",
        "generated_at": report.generated_at.as_str(),
    });
    lines.push(
        serde_json::to_string(&meta)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?,
    );
    let stats_lines = render_stats_jsonl(&report.stats)?;
    for line in stats_lines.lines() {
        lines.push(line.to_string());
    }
    for ticket in &report.recent_tickets {
        let value = serde_json::json!({
            "type": "recent_ticket",
            "ticket": ticket,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    for milestone in &report.milestones {
        let value = serde_json::json!({
            "type": "milestone_summary",
            "milestone": milestone,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn render_burndown_jsonl(report: &BurndownReport) -> Result<String> {
    let mut lines = Vec::new();
    let meta = serde_json::json!({
        "type": "burndown",
        "generated_at": report.generated_at.as_str(),
        "group_by": report_group_by_label(report.group_by),
        "range": &report.range,
        "filters": &report.filters,
    });
    lines.push(
        serde_json::to_string(&meta)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?,
    );
    for point in &report.points {
        let value = serde_json::json!({
            "type": "burndown_point",
            "point": point,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn render_throughput_jsonl(report: &ThroughputReport) -> Result<String> {
    let mut lines = Vec::new();
    let meta = serde_json::json!({
        "type": "throughput",
        "generated_at": report.generated_at.as_str(),
        "group_by": report_group_by_label(report.group_by),
        "range": &report.range,
        "filters": &report.filters,
    });
    lines.push(
        serde_json::to_string(&meta)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?,
    );
    for point in &report.points {
        let value = serde_json::json!({
            "type": "throughput_point",
            "point": point,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn render_graph_jsonl(graph: &Graph) -> Result<String> {
    let mut lines = Vec::new();
    for node in &graph.nodes {
        let value = serde_json::json!({
            "type": "node",
            "node": node,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    for edge in &graph.edges {
        let value = serde_json::json!({
            "type": "edge",
            "edge": edge,
        });
        let line = serde_json::to_string(&value)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn render_jsonl<T: serde::Serialize>(items: &[T]) -> Result<String> {
    let mut lines = Vec::new();
    for item in items {
        let line = serde_json::to_string(item)
            .map_err(|err| TikError::internal(&format!("jsonl render: {err}")))?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

fn project_csv_headers() -> Vec<&'static str> {
    vec!["name", "description", "created_at", "updated_at"]
}

fn project_csv_row(project: &ProjectMeta) -> Result<Vec<String>> {
    Ok(vec![
        project.name.clone(),
        project.description.clone(),
        project.created_at.clone(),
        project.updated_at.clone(),
    ])
}

fn milestone_csv_headers() -> Vec<&'static str> {
    vec![
        "id",
        "title",
        "status",
        "created_at",
        "updated_at",
        "due_at",
        "description",
        "tags_json",
    ]
}

fn milestone_csv_row(milestone: &Milestone) -> Result<Vec<String>> {
    let tags = serde_json::to_string(&milestone.tags)
        .map_err(|err| TikError::internal(&format!("csv tags json: {err}")))?;
    Ok(vec![
        milestone.id.to_string(),
        milestone.title.clone(),
        milestone.status.as_str().to_string(),
        milestone.created_at.clone(),
        milestone.updated_at.clone(),
        milestone.due_at.clone().unwrap_or_default(),
        milestone.description.clone(),
        tags,
    ])
}

fn ticket_csv_headers() -> Vec<&'static str> {
    vec![
        "id",
        "title",
        "status",
        "type",
        "priority",
        "severity",
        "assignees_json",
        "milestone_id",
        "tags_json",
        "created_at",
        "updated_at",
        "closed_at",
        "summary",
        "description",
        "acceptance_json",
        "estimate_value",
        "estimate_unit",
        "due_at",
        "relations_json",
        "artifacts_json",
        "custom_json",
    ]
}

fn ticket_csv_row(ticket: &Ticket) -> Result<Vec<String>> {
    let estimate_value = ticket
        .estimate
        .as_ref()
        .map(|estimate| estimate.value.to_string())
        .unwrap_or_default();
    let estimate_unit = ticket
        .estimate
        .as_ref()
        .map(|estimate| estimate.unit.clone())
        .unwrap_or_default();

    Ok(vec![
        ticket.id.to_string(),
        ticket.title.clone(),
        ticket.status.as_str().to_string(),
        ticket.kind.as_str().to_string(),
        ticket.priority.as_str().to_string(),
        ticket.severity.as_str().to_string(),
        to_json_string(&ticket.assignees)?,
        ticket
            .milestone_id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default(),
        to_json_string(&ticket.tags)?,
        ticket.created_at.clone(),
        ticket.updated_at.clone(),
        ticket.closed_at.clone().unwrap_or_default(),
        ticket.summary.clone(),
        ticket.description.clone(),
        to_json_string(&ticket.acceptance)?,
        estimate_value,
        estimate_unit,
        ticket.due_at.clone().unwrap_or_default(),
        to_json_string(&ticket.relations)?,
        to_json_string(&ticket.artifacts)?,
        to_json_string(&ticket.custom)?,
    ])
}

fn parse_tickets_csv(raw: &str) -> Result<Vec<Ticket>> {
    let mut reader = csv::Reader::from_reader(raw.as_bytes());
    let headers = reader
        .headers()
        .map_err(|err| TikError::ImportExport(format!("read csv headers: {err}")))?
        .clone();
    let header_map = csv_header_map(&headers, &ticket_csv_headers())?;

    let mut tickets = Vec::new();
    for record in reader
        .records()
        .map(|result| result.map_err(|err| TikError::ImportExport(format!("read csv: {err}"))))
    {
        let record = record?;
        let mut map = serde_json::Map::new();
        map.insert("schema_version".to_string(), serde_json::json!("1.0"));
        map.insert(
            "id".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "id")?),
        );
        map.insert(
            "title".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "title")?),
        );
        map.insert(
            "status".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "status")?),
        );
        map.insert(
            "type".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "type")?),
        );
        map.insert(
            "priority".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "priority")?),
        );
        map.insert(
            "severity".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "severity")?),
        );
        map.insert(
            "assignees".to_string(),
            parse_json_array_field(
                csv_get(&record, &header_map, "assignees_json")?,
                "assignees",
            )?,
        );
        let milestone_id = csv_get(&record, &header_map, "milestone_id")?.trim();
        if milestone_id.is_empty() {
            map.insert("milestone_id".to_string(), serde_json::Value::Null);
        } else {
            map.insert("milestone_id".to_string(), serde_json::json!(milestone_id));
        }
        map.insert(
            "tags".to_string(),
            parse_json_array_field(csv_get(&record, &header_map, "tags_json")?, "tags")?,
        );
        map.insert(
            "created_at".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "created_at")?),
        );
        map.insert(
            "updated_at".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "updated_at")?),
        );
        let closed_at = csv_get(&record, &header_map, "closed_at")?.trim();
        if closed_at.is_empty() {
            map.insert("closed_at".to_string(), serde_json::Value::Null);
        } else {
            map.insert("closed_at".to_string(), serde_json::json!(closed_at));
        }
        map.insert(
            "summary".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "summary")?),
        );
        map.insert(
            "description".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "description")?),
        );
        map.insert(
            "acceptance".to_string(),
            parse_json_array_field(
                csv_get(&record, &header_map, "acceptance_json")?,
                "acceptance",
            )?,
        );
        map.insert(
            "estimate".to_string(),
            parse_estimate_field(
                csv_get(&record, &header_map, "estimate_value")?,
                csv_get(&record, &header_map, "estimate_unit")?,
            )?,
        );
        let due_at = csv_get(&record, &header_map, "due_at")?.trim();
        if due_at.is_empty() {
            map.insert("due_at".to_string(), serde_json::Value::Null);
        } else {
            map.insert("due_at".to_string(), serde_json::json!(due_at));
        }
        map.insert(
            "relations".to_string(),
            parse_json_array_field(
                csv_get(&record, &header_map, "relations_json")?,
                "relations",
            )?,
        );
        map.insert(
            "artifacts".to_string(),
            parse_json_array_field(
                csv_get(&record, &header_map, "artifacts_json")?,
                "artifacts",
            )?,
        );
        map.insert(
            "custom".to_string(),
            parse_json_object_field(csv_get(&record, &header_map, "custom_json")?, "custom")?,
        );

        let ticket: Ticket = serde_json::from_value(serde_json::Value::Object(map))
            .map_err(|err| TikError::ImportExport(format!("invalid ticket csv: {err}")))?;
        tickets.push(ticket);
    }
    Ok(tickets)
}

fn parse_milestones_csv(raw: &str) -> Result<Vec<Milestone>> {
    let mut reader = csv::Reader::from_reader(raw.as_bytes());
    let headers = reader
        .headers()
        .map_err(|err| TikError::ImportExport(format!("read csv headers: {err}")))?
        .clone();
    let header_map = csv_header_map(&headers, &milestone_csv_headers())?;

    let mut milestones = Vec::new();
    for record in reader
        .records()
        .map(|result| result.map_err(|err| TikError::ImportExport(format!("read csv: {err}"))))
    {
        let record = record?;
        let mut map = serde_json::Map::new();
        map.insert("schema_version".to_string(), serde_json::json!("1.0"));
        map.insert(
            "id".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "id")?),
        );
        map.insert(
            "title".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "title")?),
        );
        map.insert(
            "status".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "status")?),
        );
        map.insert(
            "created_at".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "created_at")?),
        );
        map.insert(
            "updated_at".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "updated_at")?),
        );
        let due_at = csv_get(&record, &header_map, "due_at")?.trim();
        if due_at.is_empty() {
            map.insert("due_at".to_string(), serde_json::Value::Null);
        } else {
            map.insert("due_at".to_string(), serde_json::json!(due_at));
        }
        map.insert(
            "description".to_string(),
            serde_json::json!(csv_get(&record, &header_map, "description")?),
        );
        map.insert(
            "tags".to_string(),
            parse_json_array_field(csv_get(&record, &header_map, "tags_json")?, "tags")?,
        );

        let milestone: Milestone = serde_json::from_value(serde_json::Value::Object(map))
            .map_err(|err| TikError::ImportExport(format!("invalid milestone csv: {err}")))?;
        milestones.push(milestone);
    }
    Ok(milestones)
}

fn csv_header_map(
    headers: &csv::StringRecord,
    expected: &[&str],
) -> Result<HashMap<String, usize>> {
    let mut map = HashMap::new();
    for (idx, header) in headers.iter().enumerate() {
        map.insert(header.to_string(), idx);
    }
    for key in expected {
        if !map.contains_key(*key) {
            return Err(TikError::ImportExport(format!("missing csv header: {key}")));
        }
    }
    Ok(map)
}

fn csv_get<'a>(
    record: &'a csv::StringRecord,
    map: &HashMap<String, usize>,
    key: &str,
) -> Result<&'a str> {
    let idx = map
        .get(key)
        .ok_or_else(|| TikError::ImportExport(format!("missing csv column: {key}")))?;
    Ok(record.get(*idx).unwrap_or(""))
}

fn parse_json_array_field(raw: &str, field: &str) -> Result<serde_json::Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(serde_json::Value::Array(Vec::new()));
    }
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| TikError::ImportExport(format!("invalid {field} json: {err}")))?;
    if !value.is_array() {
        return Err(TikError::ImportExport(format!(
            "expected {field} to be array json"
        )));
    }
    Ok(value)
}

fn parse_json_object_field(raw: &str, field: &str) -> Result<serde_json::Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|err| TikError::ImportExport(format!("invalid {field} json: {err}")))?;
    if !value.is_object() {
        return Err(TikError::ImportExport(format!(
            "expected {field} to be object json"
        )));
    }
    Ok(value)
}

fn parse_estimate_field(value: &str, unit: &str) -> Result<serde_json::Value> {
    let value = value.trim();
    let unit = unit.trim();
    if value.is_empty() && unit.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    if value.is_empty() || unit.is_empty() {
        return Err(TikError::ImportExport(
            "estimate requires both value and unit".to_string(),
        ));
    }
    let value: f64 = value
        .parse()
        .map_err(|_| TikError::ImportExport("invalid estimate value".to_string()))?;
    Ok(serde_json::json!({
        "value": value,
        "unit": unit,
    }))
}

fn event_csv_headers() -> Vec<&'static str> {
    vec!["event_id", "ts", "actor", "type", "data_json"]
}

fn event_csv_row(event: &Event) -> Result<Vec<String>> {
    Ok(vec![
        event.event_id.to_string(),
        event.ts.clone(),
        event.actor.clone(),
        event.kind.clone(),
        to_json_string(&event.data)?,
    ])
}

fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|err| TikError::internal(&format!("json render: {err}")))
}

fn csv_write(headers: &[&str], rows: Vec<Vec<String>>) -> Result<String> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer
        .write_record(headers)
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    for row in rows {
        writer
            .write_record(row)
            .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    }
    let data = writer
        .into_inner()
        .map_err(|err| TikError::internal(&format!("csv render: {err}")))?;
    String::from_utf8(data).map_err(|err| TikError::internal(&format!("csv utf8: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tik_core::{
        DoctorCheck, DoctorStatus, DoctorSummary, MigrationMove, Priority, Severity, TicketType,
    };

    fn sample_status() -> RepoStatus {
        RepoStatus {
            repo_root: "/repo".to_string(),
            tik_root: "/repo/.tik".to_string(),
            project: "default".to_string(),
            project_root: "/repo/.tik/projects/default".to_string(),
            schema_version: "1.0".to_string(),
            config_version: "1.0".to_string(),
            layout_version: "1.0".to_string(),
            index_present: true,
            ticket_count: 1,
            milestone_count: 0,
        }
    }

    fn sample_project() -> ProjectMeta {
        ProjectMeta {
            schema_version: "1.0".to_string(),
            name: "alpha".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            description: "Project alpha".to_string(),
        }
    }

    fn sample_doctor_report() -> DoctorReport {
        let checks = vec![DoctorCheck {
            id: "layout_version".to_string(),
            status: DoctorStatus::Warn,
            scope: "repo".to_string(),
            message: "layout_version 1.0 (legacy)".to_string(),
            details: vec!["1.0".to_string()],
            hint: Some("run `tik migrate`".to_string()),
        }];
        let summary = DoctorSummary::from_checks(&checks);
        DoctorReport {
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            repo_root: "/repo".to_string(),
            layout_version: "1.0".to_string(),
            checks,
            summary,
        }
    }

    fn sample_migration_summary() -> MigrationSummary {
        MigrationSummary {
            migrated_at: "2026-01-02T00:00:00Z".to_string(),
            from_layout: "1.0".to_string(),
            to_layout: "2.0".to_string(),
            project: "default".to_string(),
            moves: vec![MigrationMove {
                from: "/repo/.tik/tickets".to_string(),
                to: "/repo/.tik/projects/default/tickets".to_string(),
            }],
            created: vec!["/repo/.tik/projects/default/locks".to_string()],
            warnings: vec!["tmp directory missing; created empty".to_string()],
        }
    }

    fn sample_index_summary() -> IndexSummary {
        IndexSummary {
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            ticket_count: 1,
            index_path: "/repo/.tik/index/fts.sqlite".to_string(),
        }
    }

    fn sample_index_status() -> IndexStatus {
        IndexStatus {
            index_present: true,
            fts_path: "/repo/.tik/index/fts.sqlite".to_string(),
            tickets_jsonl_path: "/repo/.tik/index/tickets.jsonl".to_string(),
            metadata_path: "/repo/.tik/index/status.json".to_string(),
            indexed_at: Some("2026-01-02T00:00:00Z".to_string()),
            ticket_count: Some(2),
            last_ticket_write: Some("2026-01-02T00:00:00Z".to_string()),
            last_milestone_write: Some("2026-01-02T00:00:00Z".to_string()),
            stale: false,
        }
    }

    fn sample_import_summary() -> ImportSummary {
        ImportSummary {
            tickets_imported: 2,
            milestones_imported: 1,
        }
    }

    fn sample_ticket() -> Ticket {
        Ticket {
            schema_version: "1.0".to_string(),
            id: TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            title: "Title".to_string(),
            status: TicketStatus::Open,
            kind: TicketType::Task,
            priority: Priority::Medium,
            severity: Severity::Normal,
            assignees: vec![],
            milestone_id: None,
            tags: vec!["mvp".to_string()],
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            closed_at: None,
            summary: "Summary".to_string(),
            description: "Desc".to_string(),
            acceptance: vec![],
            estimate: None,
            due_at: None,
            relations: vec![],
            artifacts: vec![],
            custom: serde_json::Map::new(),
        }
    }

    fn sample_milestone() -> Milestone {
        Milestone {
            schema_version: "1.0".to_string(),
            id: MilestoneId::parse("M-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            title: "Phase 1".to_string(),
            status: MilestoneStatus::Open,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            due_at: None,
            description: "Milestone".to_string(),
            tags: vec!["mvp".to_string()],
        }
    }

    fn sample_event() -> Event {
        Event {
            event_id: tik_core::EventId::parse("E-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            ts: "2026-01-01T00:00:00Z".to_string(),
            actor: "human".to_string(),
            kind: "note".to_string(),
            data: serde_json::json!({"text": "note"}),
        }
    }

    fn map(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for (key, value) in entries {
            out.insert((*key).to_string(), *value);
        }
        out
    }

    fn sample_stats() -> Stats {
        Stats {
            tickets_total: 3,
            milestones_total: 1,
            tickets_by_status: map(&[("open", 2), ("closed", 1)]),
            tickets_by_type: map(&[("task", 3)]),
            tickets_by_priority: map(&[("medium", 2), ("high", 1)]),
            tickets_by_severity: map(&[("normal", 3)]),
            tickets_by_assignee: map(&[("unassigned", 3)]),
            tickets_by_tag: map(&[("mvp", 3)]),
            tickets_by_milestone: map(&[("M-01ARZ3NDEKTSV4RRFFQ69G5FAV", 3)]),
            milestones_by_status: map(&[("open", 1)]),
        }
    }

    fn sample_ticket_summary() -> TicketSummary {
        TicketSummary {
            id: "T-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            title: "Summary".to_string(),
            status: "open".to_string(),
            priority: "medium".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            milestone_id: Some("M-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()),
        }
    }

    fn sample_milestone_summary() -> MilestoneSummary {
        MilestoneSummary {
            id: "M-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            title: "Phase 1".to_string(),
            status: "open".to_string(),
            due_at: None,
            total_tickets: 3,
            open_tickets: 2,
            closed_tickets: 1,
        }
    }

    fn sample_report() -> Report {
        Report {
            generated_at: "2026-01-01T00:00:00Z".to_string(),
            stats: sample_stats(),
            recent_tickets: vec![sample_ticket_summary()],
            milestones: vec![sample_milestone_summary()],
        }
    }

    fn sample_report_filters() -> ReportFilters {
        ReportFilters {
            status: vec!["open".to_string()],
            tags: vec!["mvp".to_string()],
            assignees: vec!["alice".to_string()],
            milestone_ids: vec!["M-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()],
            since: Some("2026-01-01T00:00:00Z".to_string()),
            until: Some("2026-01-07T00:00:00Z".to_string()),
        }
    }

    fn sample_burndown_report() -> BurndownReport {
        BurndownReport {
            generated_at: "2026-01-07T00:00:00Z".to_string(),
            filters: sample_report_filters(),
            range: tik_core::ReportRange {
                since: "2026-01-01T00:00:00Z".to_string(),
                until: "2026-01-07T00:00:00Z".to_string(),
            },
            group_by: ReportGroupBy::Day,
            points: vec![tik_core::BurndownPoint {
                period_start: "2026-01-01T00:00:00Z".to_string(),
                period_end: "2026-01-01T23:59:59Z".to_string(),
                open_tickets: 3,
            }],
        }
    }

    fn sample_throughput_report() -> ThroughputReport {
        ThroughputReport {
            generated_at: "2026-01-07T00:00:00Z".to_string(),
            filters: sample_report_filters(),
            range: tik_core::ReportRange {
                since: "2026-01-01T00:00:00Z".to_string(),
                until: "2026-01-07T00:00:00Z".to_string(),
            },
            group_by: ReportGroupBy::Day,
            points: vec![tik_core::ThroughputPoint {
                period_start: "2026-01-01T00:00:00Z".to_string(),
                period_end: "2026-01-01T23:59:59Z".to_string(),
                closed_tickets: 1,
            }],
        }
    }

    fn sample_graph() -> Graph {
        Graph {
            nodes: vec![
                tik_core::GraphNode {
                    id: "T-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
                    title: "Node A".to_string(),
                    status: "open".to_string(),
                },
                tik_core::GraphNode {
                    id: "T-01ARZ3NDEKTSV4RRFFQ69G5FAA".to_string(),
                    title: "Node B".to_string(),
                    status: "blocked".to_string(),
                },
            ],
            edges: vec![tik_core::GraphEdge {
                from: "T-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
                to: "T-01ARZ3NDEKTSV4RRFFQ69G5FAA".to_string(),
                relation: "blocks".to_string(),
            }],
        }
    }

    #[test]
    fn render_status_all_formats() {
        let status = sample_status();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_status(format, &status).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_project_all_formats() {
        let project = sample_project();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_project(format, &project).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_project_list_all_formats() {
        let projects = vec![sample_project()];
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_project_list(format, &projects).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_doctor_all_formats() {
        let report = sample_doctor_report();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_doctor(format, &report).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_migration_summary_all_formats() {
        let summary = sample_migration_summary();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_migration_summary(format, &summary).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_index_summary_all_formats() {
        let summary = sample_index_summary();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_index_summary(format, &summary).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_index_status_all_formats() {
        let status = sample_index_status();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_index_status(format, &status).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_import_summary_all_formats() {
        let summary = sample_import_summary();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_import_summary(format, &summary).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_stats_all_formats() {
        let stats = sample_stats();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_stats(format, &stats).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_report_all_formats() {
        let report = sample_report();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_report(format, &report).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_burndown_all_formats() {
        let report = sample_burndown_report();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_burndown(format, &report).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_throughput_all_formats() {
        let report = sample_throughput_report();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_throughput(format, &report).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_graph_all_formats() {
        let graph = sample_graph();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_graph(format, &graph).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_graph_dot_output() {
        let graph = sample_graph();
        let output = render_graph_dot(&graph);
        assert!(output.contains("digraph tik"));
        assert!(output.contains("->"));
    }

    #[test]
    fn render_ticket_all_formats() {
        let ticket = sample_ticket();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_ticket(format, &ticket).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_milestone_all_formats() {
        let milestone = sample_milestone();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_milestone(format, &milestone).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_milestone_list_all_formats() {
        let milestone = sample_milestone();
        let milestones = vec![milestone];
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_milestone_list(format, &milestones).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_ticket_list_all_formats() {
        let ticket = sample_ticket();
        let tickets = vec![ticket];
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_ticket_list(format, &tickets).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_event_all_formats() {
        let event = sample_event();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_event(format, &event).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_event_list_all_formats() {
        let event = sample_event();
        let events = vec![event];
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_event_list(format, &events).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn resolve_actor_prefers_explicit() {
        let actor = resolve_actor(Some("explicit".to_string()));
        assert_eq!(actor, "explicit");
    }

    #[test]
    fn resolve_list_operation_requires_single_choice() {
        let err = resolve_list_operation(vec![], vec![], vec![], false, "assign").unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));

        let err = resolve_list_operation(
            vec!["a".to_string()],
            vec!["b".to_string()],
            vec![],
            false,
            "assign",
        )
        .unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn resolve_list_operation_picks_action() {
        let op =
            resolve_list_operation(vec!["a".to_string()], vec![], vec![], false, "assign").unwrap();
        assert!(matches!(op, ListOperation::Add(_)));

        let op = resolve_list_operation(vec![], vec![], vec![], true, "assign").unwrap();
        assert!(matches!(op, ListOperation::Clear));
    }

    #[test]
    fn resolve_milestone_target_requires_id_or_clear() {
        let err = resolve_milestone_target(None, false).unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));

        let err = resolve_milestone_target(Some("M-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()), true)
            .unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));

        let target =
            resolve_milestone_target(Some("M-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()), false)
                .unwrap();
        assert!(target.is_some());

        let target = resolve_milestone_target(None, true).unwrap();
        assert!(target.is_none());
    }

    #[test]
    fn resolve_output_format_prefers_cli() {
        let config = tik_core::Config::default();
        let format = resolve_output_format(Some(OutputFormat::Jsonl), Some(&config)).unwrap();
        assert!(matches!(format, OutputFormat::Jsonl));
    }

    #[test]
    fn resolve_output_format_uses_config() {
        let config = tik_core::Config {
            output_format: "yaml".to_string(),
            ..tik_core::Config::default()
        };
        let format = resolve_output_format(None, Some(&config)).unwrap();
        assert!(matches!(format, OutputFormat::Yaml));
    }

    #[test]
    fn resolve_pager_mode_uses_config() {
        let config = tik_core::Config {
            pager: "never".to_string(),
            ..tik_core::Config::default()
        };
        let pager = resolve_pager_mode(Some(&config)).unwrap();
        assert_eq!(pager, PagerMode::Never);
    }

    #[test]
    fn resolve_pagination_defaults_limit() {
        let (offset, limit) = resolve_pagination(false, None, 0, 50).unwrap();
        assert_eq!(offset, 0);
        assert_eq!(limit, Some(50));
    }

    #[test]
    fn resolve_pagination_all_disables_limit() {
        let (offset, limit) = resolve_pagination(true, Some(10), 5, 50).unwrap();
        assert_eq!(offset, 5);
        assert!(limit.is_none());
    }

    #[test]
    fn resolve_pagination_rejects_zero_limit() {
        let err = resolve_pagination(false, Some(0), 0, 50).unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn render_config_all_formats() {
        let config = tik_core::Config::default();
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_config(format, &config).unwrap();
            assert!(!output.trim().is_empty());
        }
    }

    #[test]
    fn render_config_value_all_formats() {
        for format in [
            OutputFormat::Table,
            OutputFormat::Compact,
            OutputFormat::Json,
            OutputFormat::Jsonl,
            OutputFormat::Yaml,
            OutputFormat::Md,
            OutputFormat::Csv,
        ] {
            let output = render_config_value(format, "output_format", "json").unwrap();
            assert!(!output.trim().is_empty());
        }
    }
}
