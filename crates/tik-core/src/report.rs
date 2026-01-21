use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::domain::milestone::Milestone;
use crate::domain::ticket::Ticket;
use crate::timeutil;
use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub tickets_total: usize,
    pub milestones_total: usize,
    pub tickets_by_status: BTreeMap<String, usize>,
    pub tickets_by_type: BTreeMap<String, usize>,
    pub tickets_by_priority: BTreeMap<String, usize>,
    pub tickets_by_severity: BTreeMap<String, usize>,
    pub tickets_by_assignee: BTreeMap<String, usize>,
    pub tickets_by_tag: BTreeMap<String, usize>,
    pub tickets_by_milestone: BTreeMap<String, usize>,
    pub milestones_by_status: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub updated_at: String,
    pub milestone_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub due_at: Option<String>,
    pub total_tickets: usize,
    pub open_tickets: usize,
    pub closed_tickets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub generated_at: String,
    pub stats: Stats,
    pub recent_tickets: Vec<TicketSummary>,
    pub milestones: Vec<MilestoneSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub relation: String,
}

pub fn compute_stats(tickets: &[Ticket], milestones: &[Milestone]) -> Stats {
    let mut stats = Stats {
        tickets_total: tickets.len(),
        milestones_total: milestones.len(),
        tickets_by_status: BTreeMap::new(),
        tickets_by_type: BTreeMap::new(),
        tickets_by_priority: BTreeMap::new(),
        tickets_by_severity: BTreeMap::new(),
        tickets_by_assignee: BTreeMap::new(),
        tickets_by_tag: BTreeMap::new(),
        tickets_by_milestone: BTreeMap::new(),
        milestones_by_status: BTreeMap::new(),
    };

    for ticket in tickets {
        incr(&mut stats.tickets_by_status, ticket.status.as_str());
        incr(&mut stats.tickets_by_type, ticket.kind.as_str());
        incr(&mut stats.tickets_by_priority, ticket.priority.as_str());
        incr(&mut stats.tickets_by_severity, ticket.severity.as_str());

        if ticket.assignees.is_empty() {
            incr(&mut stats.tickets_by_assignee, "unassigned");
        } else {
            for assignee in &ticket.assignees {
                incr(&mut stats.tickets_by_assignee, assignee);
            }
        }

        if ticket.tags.is_empty() {
            incr(&mut stats.tickets_by_tag, "untagged");
        } else {
            for tag in &ticket.tags {
                incr(&mut stats.tickets_by_tag, tag);
            }
        }

        if let Some(milestone_id) = &ticket.milestone_id {
            incr(&mut stats.tickets_by_milestone, milestone_id.as_str());
        } else {
            incr(&mut stats.tickets_by_milestone, "none");
        }
    }

    for milestone in milestones {
        incr(&mut stats.milestones_by_status, milestone.status.as_str());
    }

    stats
}

pub fn compute_report(
    tickets: &[Ticket],
    milestones: &[Milestone],
    recent_limit: usize,
) -> Result<Report> {
    let stats = compute_stats(tickets, milestones);
    let mut recent = tickets.to_vec();
    recent.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let recent_tickets = recent
        .into_iter()
        .take(recent_limit)
        .map(ticket_summary)
        .collect();

    let mut milestone_map: HashMap<String, MilestoneSummary> = HashMap::new();
    for milestone in milestones {
        milestone_map.insert(
            milestone.id.as_str().to_string(),
            MilestoneSummary {
                id: milestone.id.as_str().to_string(),
                title: milestone.title.clone(),
                status: milestone.status.as_str().to_string(),
                due_at: milestone.due_at.clone(),
                total_tickets: 0,
                open_tickets: 0,
                closed_tickets: 0,
            },
        );
    }

    for ticket in tickets {
        let milestone_id = match &ticket.milestone_id {
            Some(id) => id.as_str(),
            None => continue,
        };
        if let Some(summary) = milestone_map.get_mut(milestone_id) {
            summary.total_tickets += 1;
            if ticket.status.as_str() == "closed" {
                summary.closed_tickets += 1;
            } else {
                summary.open_tickets += 1;
            }
        }
    }

    let mut milestones: Vec<MilestoneSummary> = milestone_map.into_values().collect();
    milestones.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(Report {
        generated_at: timeutil::now_rfc3339()?,
        stats,
        recent_tickets,
        milestones,
    })
}

pub fn compute_graph(tickets: &[Ticket]) -> Graph {
    let mut nodes: Vec<GraphNode> = tickets
        .iter()
        .map(|ticket| GraphNode {
            id: ticket.id.as_str().to_string(),
            title: ticket.title.clone(),
            status: ticket.status.as_str().to_string(),
        })
        .collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let mut edges = Vec::new();
    for ticket in tickets {
        for relation in &ticket.relations {
            edges.push(GraphEdge {
                from: ticket.id.as_str().to_string(),
                to: relation.id.as_str().to_string(),
                relation: relation.kind.as_str().to_string(),
            });
        }
    }
    edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));

    Graph { nodes, edges }
}

fn incr(map: &mut BTreeMap<String, usize>, key: &str) {
    *map.entry(key.to_string()).or_insert(0) += 1;
}

fn ticket_summary(ticket: Ticket) -> TicketSummary {
    TicketSummary {
        id: ticket.id.as_str().to_string(),
        title: ticket.title,
        status: ticket.status.as_str().to_string(),
        priority: ticket.priority.as_str().to_string(),
        updated_at: ticket.updated_at,
        milestone_id: ticket.milestone_id.as_ref().map(|id| id.as_str().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::{NewTicket, Ticket};

    fn sample_ticket(title: &str) -> Ticket {
        Ticket::new(
            NewTicket {
                title: title.to_string(),
                summary: None,
                description: None,
                tags: vec!["mvp".to_string()],
            },
            "2026-01-01T00:00:00Z",
        )
    }

    #[test]
    fn stats_counts_tickets() {
        let tickets = vec![sample_ticket("A"), sample_ticket("B")];
        let stats = compute_stats(&tickets, &[]);
        assert_eq!(stats.tickets_total, 2);
        assert_eq!(stats.milestones_total, 0);
        assert_eq!(stats.tickets_by_status.get("open"), Some(&2));
    }

    #[test]
    fn graph_includes_edges() {
        let mut ticket = sample_ticket("A");
        ticket.relations.push(crate::domain::ticket::Relation {
            kind: crate::domain::ticket::RelationType::Blocks,
            id: crate::domain::ids::TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
        });
        let graph = compute_graph(&[ticket]);
        assert_eq!(graph.edges.len(), 1);
    }
}
