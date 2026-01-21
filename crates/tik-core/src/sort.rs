use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::domain::ticket::{Priority, Ticket, TicketStatus};
use crate::report::TicketSummary;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TicketSort {
    Id,
    Updated,
    Created,
    Priority,
    Status,
    Title,
}

pub fn sort_tickets(tickets: &mut [Ticket], sort: TicketSort) {
    tickets.sort_by(|a, b| compare_ticket(a, b, sort));
}

pub fn sort_ticket_summaries(tickets: &mut [TicketSummary], sort: TicketSort) {
    tickets.sort_by(|a, b| compare_ticket_summary(a, b, sort));
}

fn compare_ticket(a: &Ticket, b: &Ticket, sort: TicketSort) -> Ordering {
    match sort {
        TicketSort::Id => {
            a.id.as_str()
                .cmp(b.id.as_str())
                .then_with(|| a.title.cmp(&b.title))
        }
        TicketSort::Updated => b
            .updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.as_str().cmp(b.id.as_str())),
        TicketSort::Created => b
            .created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.as_str().cmp(b.id.as_str())),
        TicketSort::Priority => priority_rank(&b.priority)
            .cmp(&priority_rank(&a.priority))
            .then_with(|| a.id.as_str().cmp(b.id.as_str())),
        TicketSort::Status => status_rank(&a.status)
            .cmp(&status_rank(&b.status))
            .then_with(|| a.id.as_str().cmp(b.id.as_str())),
        TicketSort::Title => a
            .title
            .cmp(&b.title)
            .then_with(|| a.id.as_str().cmp(b.id.as_str())),
    }
}

fn compare_ticket_summary(a: &TicketSummary, b: &TicketSummary, sort: TicketSort) -> Ordering {
    match sort {
        TicketSort::Id => a.id.cmp(&b.id).then_with(|| a.title.cmp(&b.title)),
        TicketSort::Updated => b
            .updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.cmp(&b.id)),
        TicketSort::Created => b
            .created_at
            .cmp(&a.created_at)
            .then_with(|| a.id.cmp(&b.id)),
        TicketSort::Priority => priority_rank_str(&b.priority)
            .cmp(&priority_rank_str(&a.priority))
            .then_with(|| a.id.cmp(&b.id)),
        TicketSort::Status => status_rank_str(&a.status)
            .cmp(&status_rank_str(&b.status))
            .then_with(|| a.id.cmp(&b.id)),
        TicketSort::Title => a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id)),
    }
}

fn priority_rank(priority: &Priority) -> u8 {
    match priority {
        Priority::Low => 0,
        Priority::Medium => 1,
        Priority::High => 2,
        Priority::Critical => 3,
    }
}

fn status_rank(status: &TicketStatus) -> u8 {
    match status {
        TicketStatus::Open => 0,
        TicketStatus::InProgress => 1,
        TicketStatus::Blocked => 2,
        TicketStatus::Closed => 3,
        TicketStatus::Archived => 4,
    }
}

fn priority_rank_str(priority: &str) -> u8 {
    match priority {
        "low" => 0,
        "medium" => 1,
        "high" => 2,
        "critical" => 3,
        _ => 4,
    }
}

fn status_rank_str(status: &str) -> u8 {
    match status {
        "open" => 0,
        "in_progress" => 1,
        "blocked" => 2,
        "closed" => 3,
        "archived" => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::NewTicket;

    #[test]
    fn sort_tickets_by_updated_is_deterministic() {
        let mut a = Ticket::new(
            NewTicket {
                title: "A".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        let mut b = Ticket::new(
            NewTicket {
                title: "B".to_string(),
                summary: None,
                description: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        a.id = crate::domain::ids::TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        b.id = crate::domain::ids::TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();
        a.updated_at = "2026-01-02T00:00:00Z".to_string();
        b.updated_at = "2026-01-02T00:00:00Z".to_string();

        let mut tickets = vec![b.clone(), a.clone()];
        sort_tickets(&mut tickets, TicketSort::Updated);
        assert_eq!(tickets[0].id, a.id);
        assert_eq!(tickets[1].id, b.id);
    }

    #[test]
    fn sort_ticket_summaries_by_priority() {
        let summary_a = TicketSummary {
            id: "T-01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            title: "A".to_string(),
            status: "open".to_string(),
            priority: "low".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            milestone_id: None,
        };
        let mut summary_b = summary_a.clone();
        summary_b.id = "T-01ARZ3NDEKTSV4RRFFQ69G5FAW".to_string();
        summary_b.priority = "critical".to_string();

        let mut summaries = vec![summary_a.clone(), summary_b.clone()];
        sort_ticket_summaries(&mut summaries, TicketSort::Priority);
        assert_eq!(summaries[0].id, summary_b.id);
        assert_eq!(summaries[1].id, summary_a.id);
    }
}
