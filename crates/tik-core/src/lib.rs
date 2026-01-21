mod config;
mod doctor;
mod domain;
mod error;
mod fs;
mod index;
mod interop;
mod lock;
mod migrate;
mod repo;
mod report;
mod schema;
mod search;
mod sort;
mod status;
mod store;
mod timeutil;
mod workspace;

pub use config::Config;
pub use doctor::{
    check_estimate_units, check_event_log_consistency, check_index_staleness,
    check_referential_integrity, check_schema_version, check_stale_locks, check_tag_normalization,
    run_comprehensive_checks, DoctorCheck, DoctorReport, DoctorStatus, DoctorSummary,
};
pub use domain::event::{Actor, ActorType, Event, EventType};
pub use domain::ids::{EventId, MilestoneId, TicketId};
pub use domain::milestone::{Milestone, MilestoneStatus, NewMilestone};
pub use domain::ticket::{
    normalize_assignee, normalize_tag, AcceptanceCriterion, ArtifactType, Estimate, EstimateUnit,
    NewTicket, Priority, RelationType, Severity, Ticket, TicketStatus, TicketType,
};
pub use error::{ErrorCode, Result, TikError};
pub use index::{IndexStatus, IndexSummary};
pub use interop::{ExportBundle, ExportMilestone, ExportTicket, ImportSummary};
pub use migrate::{
    normalize_actor, normalize_estimate_unit, normalize_event_type, run_migrations, DataMigration,
    DataMigrationReport, MigrationMove, MigrationRegistry, MigrationResult, MigrationSummary,
    SchemaVersion,
};
pub use repo::{BackupSummary, Repo, RestoreSummary};
pub use report::{
    BurndownPoint, BurndownReport, Graph, GraphEdge, GraphNode, GraphOptions, MilestoneSummary,
    Report, ReportFilters, ReportGroupBy, ReportRange, Stats, ThroughputPoint, ThroughputReport,
    TicketSummary,
};
pub use search::SearchQuery;
pub use sort::{sort_ticket_summaries, sort_tickets, TicketSort};
pub use status::RepoStatus;
pub use store::ticket_store::TicketStore;
pub use workspace::{ProjectMeta, WorkspaceConfig};
