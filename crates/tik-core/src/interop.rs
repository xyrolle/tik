use serde::{Deserialize, Serialize};

use crate::domain::event::Event;
use crate::domain::milestone::Milestone;
use crate::domain::ticket::Ticket;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub schema_version: String,
    pub exported_at: String,
    pub tickets: Vec<ExportTicket>,
    pub milestones: Vec<ExportMilestone>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTicket {
    pub ticket: Ticket,
    pub events: Vec<Event>,
    pub notes_md: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMilestone {
    pub milestone: Milestone,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSummary {
    pub tickets_imported: usize,
    pub milestones_imported: usize,
}
