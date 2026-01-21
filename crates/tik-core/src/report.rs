use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{Date, Duration, Month, OffsetDateTime, Time, UtcOffset};

use crate::domain::event::Event;
use crate::domain::ids::{MilestoneId, TicketId};
use crate::domain::milestone::Milestone;
use crate::domain::ticket::{RelationType, Ticket, TicketStatus};
use crate::sort::{sort_tickets, TicketSort};
use crate::timeutil;
use crate::{Result, TikError};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReportFilters {
    pub status: Vec<String>,
    pub tags: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone_ids: Vec<String>,
    pub since: Option<String>,
    pub until: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportGroupBy {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRange {
    pub since: String,
    pub until: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurndownPoint {
    pub period_start: String,
    pub period_end: String,
    pub open_tickets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurndownReport {
    pub generated_at: String,
    pub filters: ReportFilters,
    pub range: ReportRange,
    pub group_by: ReportGroupBy,
    pub points: Vec<BurndownPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputPoint {
    pub period_start: String,
    pub period_end: String,
    pub closed_tickets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputReport {
    pub generated_at: String,
    pub filters: ReportFilters,
    pub range: ReportRange,
    pub group_by: ReportGroupBy,
    pub points: Vec<ThroughputPoint>,
}

#[derive(Debug, Clone)]
pub(crate) struct TicketHistory {
    pub ticket: Ticket,
    pub events: Vec<Event>,
}

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
    pub created_at: String,
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

#[derive(Debug, Clone, Default)]
pub struct GraphOptions {
    pub roots: Vec<TicketId>,
    pub depth: Option<usize>,
    pub relations: Vec<RelationType>,
    pub include_milestones: bool,
}

impl ReportFilters {
    pub fn parse(
        status: Vec<String>,
        tags: Vec<String>,
        assignees: Vec<String>,
        milestone_ids: Vec<String>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<ReportFilters> {
        let mut status_out = Vec::new();
        for value in status {
            let value = value.trim().to_lowercase();
            if value.is_empty() {
                return Err(TikError::usage("status filter cannot be empty"));
            }
            let parsed = TicketStatus::parse(&value)?;
            status_out.push(parsed.as_str().to_string());
        }
        status_out.sort();
        status_out.dedup();

        let tags_out = normalize_filter_values(tags, "tag")?;
        let assignees_out = normalize_filter_values(assignees, "assignee")?;

        let mut milestone_out = Vec::new();
        for value in milestone_ids {
            let value = value.trim();
            if value.is_empty() {
                return Err(TikError::usage("milestone filter cannot be empty"));
            }
            let milestone = MilestoneId::parse(value)?;
            milestone_out.push(milestone.as_str().to_string());
        }
        milestone_out.sort();
        milestone_out.dedup();

        let since = match since {
            Some(value) => Some(normalize_bound(value, BoundKind::Since)?),
            None => None,
        };
        let until = match until {
            Some(value) => Some(normalize_bound(value, BoundKind::Until)?),
            None => None,
        };
        if let (Some(since), Some(until)) = (&since, &until) {
            let since_dt = parse_rfc3339(since)?;
            let until_dt = parse_rfc3339(until)?;
            if since_dt > until_dt {
                return Err(TikError::usage("since must be before or equal to until"));
            }
        }

        Ok(ReportFilters {
            status: status_out,
            tags: tags_out,
            assignees: assignees_out,
            milestone_ids: milestone_out,
            since,
            until,
        })
    }

    pub fn without_range(&self) -> ReportFilters {
        let mut out = self.clone();
        out.since = None;
        out.until = None;
        out
    }

    fn matches_ticket(&self, ticket: &Ticket, bounds: &ReportBounds) -> Result<bool> {
        if !self.status.is_empty() && !self.status.contains(&ticket.status.as_str().to_string()) {
            return Ok(false);
        }
        if !self.tags.is_empty() && !list_contains_all(&ticket.tags, &self.tags) {
            return Ok(false);
        }
        if !self.assignees.is_empty() && !list_contains_all(&ticket.assignees, &self.assignees) {
            return Ok(false);
        }
        if !self.milestone_ids.is_empty() {
            match &ticket.milestone_id {
                Some(id) => {
                    if !self.milestone_ids.contains(&id.as_str().to_string()) {
                        return Ok(false);
                    }
                }
                None => return Ok(false),
            }
        }
        if bounds.since.is_some() || bounds.until.is_some() {
            let updated_at = parse_rfc3339(&ticket.updated_at)?;
            if let Some(since) = bounds.since {
                if updated_at < since {
                    return Ok(false);
                }
            }
            if let Some(until) = bounds.until {
                if updated_at > until {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

pub(crate) fn filter_tickets(tickets: &[Ticket], filters: &ReportFilters) -> Result<Vec<Ticket>> {
    let bounds = ReportBounds::from_filters(filters)?;
    let mut out = Vec::new();
    for ticket in tickets {
        if filters.matches_ticket(ticket, &bounds)? {
            out.push(ticket.clone());
        }
    }
    Ok(out)
}

pub(crate) fn filter_milestones(
    milestones: &[Milestone],
    tickets: &[Ticket],
    filters: &ReportFilters,
) -> Vec<Milestone> {
    let mut milestone_ids: Vec<String> = if filters.milestone_ids.is_empty() {
        tickets
            .iter()
            .filter_map(|ticket| ticket.milestone_id.as_ref())
            .map(|id| id.as_str().to_string())
            .collect()
    } else {
        filters.milestone_ids.clone()
    };
    milestone_ids.sort();
    milestone_ids.dedup();

    milestones
        .iter()
        .filter(|milestone| milestone_ids.contains(&milestone.id.as_str().to_string()))
        .cloned()
        .collect()
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

#[allow(dead_code)]
pub fn compute_report(
    tickets: &[Ticket],
    milestones: &[Milestone],
    recent_limit: usize,
) -> Result<Report> {
    compute_report_sorted(tickets, milestones, recent_limit, TicketSort::Updated)
}

pub fn compute_report_sorted(
    tickets: &[Ticket],
    milestones: &[Milestone],
    recent_limit: usize,
    sort: TicketSort,
) -> Result<Report> {
    let stats = compute_stats(tickets, milestones);
    let mut recent = tickets.to_vec();
    sort_tickets(&mut recent, sort);
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

pub fn compute_graph_scoped(
    tickets: &[Ticket],
    milestones: &[Milestone],
    options: &GraphOptions,
) -> Result<Graph> {
    let mut ticket_map: HashMap<String, &Ticket> = HashMap::new();
    for ticket in tickets {
        ticket_map.insert(ticket.id.as_str().to_string(), ticket);
    }

    let relation_filter: Option<HashSet<RelationType>> = if options.relations.is_empty() {
        None
    } else {
        Some(options.relations.iter().cloned().collect())
    };

    let mut included: HashSet<String> = HashSet::new();
    if options.roots.is_empty() {
        for ticket in tickets {
            included.insert(ticket.id.as_str().to_string());
        }
    } else {
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        for root in &options.roots {
            let root_id = root.as_str().to_string();
            if !ticket_map.contains_key(&root_id) {
                return Err(TikError::NotFound(format!("ticket {root_id}")));
            }
            if included.insert(root_id.clone()) {
                queue.push_back((root_id, 0));
            }
        }
        while let Some((current, depth)) = queue.pop_front() {
            if let Some(limit) = options.depth {
                if depth >= limit {
                    continue;
                }
            }
            let ticket = match ticket_map.get(&current) {
                Some(ticket) => *ticket,
                None => continue,
            };
            for relation in &ticket.relations {
                if let Some(filter) = &relation_filter {
                    if !filter.contains(&relation.kind) {
                        continue;
                    }
                }
                let target = relation.id.as_str().to_string();
                if !ticket_map.contains_key(&target) {
                    continue;
                }
                if included.insert(target.clone()) {
                    queue.push_back((target, depth + 1));
                }
            }
        }
    }

    let mut nodes: Vec<GraphNode> = included
        .iter()
        .filter_map(|id| ticket_map.get(id).map(|ticket| GraphNode {
            id: ticket.id.as_str().to_string(),
            title: ticket.title.clone(),
            status: ticket.status.as_str().to_string(),
        }))
        .collect();

    let mut edges = Vec::new();
    for id in &included {
        let Some(ticket) = ticket_map.get(id) else {
            continue;
        };
        for relation in &ticket.relations {
            if let Some(filter) = &relation_filter {
                if !filter.contains(&relation.kind) {
                    continue;
                }
            }
            let target = relation.id.as_str();
            if included.contains(target) {
                edges.push(GraphEdge {
                    from: ticket.id.as_str().to_string(),
                    to: target.to_string(),
                    relation: relation.kind.as_str().to_string(),
                });
            }
        }
    }

    if options.include_milestones {
        let mut milestone_map: HashMap<String, &Milestone> = HashMap::new();
        for milestone in milestones {
            milestone_map.insert(milestone.id.as_str().to_string(), milestone);
        }
        let mut milestone_ids = HashSet::new();
        for id in &included {
            if let Some(ticket) = ticket_map.get(id) {
                if let Some(milestone_id) = &ticket.milestone_id {
                    milestone_ids.insert(milestone_id.as_str().to_string());
                }
            }
        }
        for milestone_id in milestone_ids {
            if let Some(milestone) = milestone_map.get(&milestone_id) {
                nodes.push(GraphNode {
                    id: milestone.id.as_str().to_string(),
                    title: milestone.title.clone(),
                    status: milestone.status.as_str().to_string(),
                });
            } else {
                nodes.push(GraphNode {
                    id: milestone_id.clone(),
                    title: "missing milestone".to_string(),
                    status: "missing".to_string(),
                });
            }
        }
        for id in &included {
            if let Some(ticket) = ticket_map.get(id) {
                if let Some(milestone_id) = &ticket.milestone_id {
                    edges.push(GraphEdge {
                        from: ticket.id.as_str().to_string(),
                        to: milestone_id.as_str().to_string(),
                        relation: "milestone".to_string(),
                    });
                }
            }
        }
    }

    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));

    Ok(Graph { nodes, edges })
}

pub fn compute_burndown(
    histories: &[TicketHistory],
    filters: &ReportFilters,
    group_by: ReportGroupBy,
) -> Result<BurndownReport> {
    let range = resolve_range(histories, filters)?;
    let buckets = build_buckets(&range, group_by)?;
    let events = collect_status_events(histories)?;

    let mut points = Vec::with_capacity(buckets.len());
    let mut status_map: HashMap<String, TicketStatus> = HashMap::new();
    let mut open_count = 0usize;
    let mut event_idx = 0usize;

    for bucket in &buckets {
        while event_idx < events.len() && events[event_idx].ts <= bucket.end {
            apply_status_change(&events[event_idx], &mut status_map, &mut open_count);
            event_idx += 1;
        }
        points.push(BurndownPoint {
            period_start: format_rfc3339(bucket.start)?,
            period_end: format_rfc3339(bucket.end)?,
            open_tickets: open_count,
        });
    }

    Ok(BurndownReport {
        generated_at: timeutil::now_rfc3339()?,
        filters: filters.clone(),
        range: ReportRange {
            since: format_rfc3339(range.since)?,
            until: format_rfc3339(range.until)?,
        },
        group_by,
        points,
    })
}

pub fn compute_throughput(
    histories: &[TicketHistory],
    filters: &ReportFilters,
    group_by: ReportGroupBy,
) -> Result<ThroughputReport> {
    let range = resolve_range(histories, filters)?;
    let buckets = build_buckets(&range, group_by)?;
    let events = collect_status_events(histories)?;

    let mut counts = vec![0usize; buckets.len()];
    let mut status_map: HashMap<String, TicketStatus> = HashMap::new();
    let mut bucket_idx = 0usize;

    for event in events {
        if event.ts > range.until {
            break;
        }
        let prev = status_map.get(&event.ticket_id).cloned();
        let prev_closed = prev.as_ref().is_some_and(is_closed_status);
        let next_closed = is_closed_status(&event.status);

        if event.ts >= range.since && next_closed && !prev_closed {
            while bucket_idx < buckets.len() && event.ts > buckets[bucket_idx].end {
                bucket_idx += 1;
            }
            if bucket_idx < buckets.len() {
                counts[bucket_idx] += 1;
            }
        }

        status_map.insert(event.ticket_id.clone(), event.status.clone());
    }

    let mut points = Vec::with_capacity(buckets.len());
    for (bucket, count) in buckets.iter().zip(counts.into_iter()) {
        points.push(ThroughputPoint {
            period_start: format_rfc3339(bucket.start)?,
            period_end: format_rfc3339(bucket.end)?,
            closed_tickets: count,
        });
    }

    Ok(ThroughputReport {
        generated_at: timeutil::now_rfc3339()?,
        filters: filters.clone(),
        range: ReportRange {
            since: format_rfc3339(range.since)?,
            until: format_rfc3339(range.until)?,
        },
        group_by,
        points,
    })
}

struct ReportBounds {
    since: Option<OffsetDateTime>,
    until: Option<OffsetDateTime>,
}

struct ReportRangeBounds {
    since: OffsetDateTime,
    until: OffsetDateTime,
}

struct TimeBucket {
    start: OffsetDateTime,
    end: OffsetDateTime,
}

struct StatusEvent {
    ts: OffsetDateTime,
    ticket_id: String,
    status: TicketStatus,
}

enum BoundKind {
    Since,
    Until,
}

impl ReportBounds {
    fn from_filters(filters: &ReportFilters) -> Result<ReportBounds> {
        let since = match filters.since.as_deref() {
            Some(value) => Some(parse_rfc3339(value)?),
            None => None,
        };
        let until = match filters.until.as_deref() {
            Some(value) => Some(parse_rfc3339(value)?),
            None => None,
        };
        Ok(ReportBounds { since, until })
    }
}

fn resolve_range(
    histories: &[TicketHistory],
    filters: &ReportFilters,
) -> Result<ReportRangeBounds> {
    let now = parse_rfc3339(&timeutil::now_rfc3339()?)?;
    let since = match filters.since.as_deref() {
        Some(value) => parse_rfc3339(value)?,
        None => earliest_created_at(histories)?.unwrap_or(now),
    };
    let until = match filters.until.as_deref() {
        Some(value) => parse_rfc3339(value)?,
        None => now,
    };
    if since > until {
        return Err(TikError::usage("since must be before or equal to until"));
    }
    Ok(ReportRangeBounds { since, until })
}

fn build_buckets(range: &ReportRangeBounds, group_by: ReportGroupBy) -> Result<Vec<TimeBucket>> {
    let mut buckets = Vec::new();
    let mut start = align_start(range.since, group_by);
    if start > range.until {
        start = range.since;
    }

    loop {
        let next_start = next_bucket_start(start, group_by)?;
        let mut end = next_start - Duration::nanoseconds(1);
        if end > range.until {
            end = range.until;
        }
        buckets.push(TimeBucket { start, end });
        if next_start > range.until {
            break;
        }
        start = next_start;
    }

    Ok(buckets)
}

fn collect_status_events(histories: &[TicketHistory]) -> Result<Vec<StatusEvent>> {
    let mut out = Vec::new();
    for history in histories {
        let mut ticket_events = Vec::new();
        for event in &history.events {
            if let Some(status) = status_from_event(event)? {
                let ts = parse_rfc3339(&event.ts)?;
                ticket_events.push(StatusEvent {
                    ts,
                    ticket_id: history.ticket.id.as_str().to_string(),
                    status,
                });
            }
        }
        if ticket_events.is_empty() {
            let ts = parse_rfc3339(&history.ticket.created_at)?;
            ticket_events.push(StatusEvent {
                ts,
                ticket_id: history.ticket.id.as_str().to_string(),
                status: history.ticket.status.clone(),
            });
        }
        out.extend(ticket_events);
    }
    out.sort_by(|a, b| a.ts.cmp(&b.ts).then_with(|| a.ticket_id.cmp(&b.ticket_id)));
    Ok(out)
}

fn apply_status_change(
    event: &StatusEvent,
    status_map: &mut HashMap<String, TicketStatus>,
    open_count: &mut usize,
) {
    let prev = status_map.get(&event.ticket_id);
    let prev_open = prev.is_some_and(is_open_status);
    let next_open = is_open_status(&event.status);
    if prev_open != next_open {
        if next_open {
            *open_count += 1;
        } else if *open_count > 0 {
            *open_count -= 1;
        }
    }
    status_map.insert(event.ticket_id.clone(), event.status.clone());
}

fn status_from_event(event: &Event) -> Result<Option<TicketStatus>> {
    match event.kind.as_str() {
        "created" | "imported" | "ticket_edited" => {
            let snapshot = extract_ticket_snapshot(event)?;
            Ok(Some(snapshot.status))
        }
        "status_change" => {
            let value = extract_str(&event.data, "to")?;
            let status = TicketStatus::parse(value)?;
            Ok(Some(status))
        }
        _ => Ok(None),
    }
}

fn extract_ticket_snapshot(event: &Event) -> Result<Ticket> {
    let value = event
        .data
        .get("ticket")
        .ok_or_else(|| TikError::Schema("ticket snapshot missing".to_string()))?;
    serde_json::from_value(value.clone())
        .map_err(|err| TikError::Schema(format!("invalid ticket snapshot: {err}")))
}

fn extract_str<'a>(data: &'a Value, key: &str) -> Result<&'a str> {
    data.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| TikError::Schema(format!("event data missing {key}")))
}

fn align_start(since: OffsetDateTime, group_by: ReportGroupBy) -> OffsetDateTime {
    let date = since.date();
    match group_by {
        ReportGroupBy::Day => start_of_day(date),
        ReportGroupBy::Week => {
            let weekday = date.weekday().number_from_monday() as i64;
            let start_date = date - Duration::days(weekday - 1);
            start_of_day(start_date)
        }
        ReportGroupBy::Month => {
            let start_date = Date::from_calendar_date(date.year(), date.month(), 1).unwrap_or(date);
            start_of_day(start_date)
        }
    }
}

fn next_bucket_start(start: OffsetDateTime, group_by: ReportGroupBy) -> Result<OffsetDateTime> {
    match group_by {
        ReportGroupBy::Day => Ok(start + Duration::days(1)),
        ReportGroupBy::Week => Ok(start + Duration::days(7)),
        ReportGroupBy::Month => {
            let date = start.date();
            let (year, month) = (date.year(), date.month());
            let (next_year, next_month) = match month {
                Month::December => (year + 1, Month::January),
                _ => (year, next_month(month)),
            };
            let next_date = Date::from_calendar_date(next_year, next_month, 1)
                .map_err(|err| TikError::internal(&format!("invalid month boundary: {err}")))?;
            Ok(start_of_day(next_date))
        }
    }
}

fn earliest_created_at(histories: &[TicketHistory]) -> Result<Option<OffsetDateTime>> {
    let mut earliest: Option<OffsetDateTime> = None;
    for history in histories {
        let created_at = parse_rfc3339(&history.ticket.created_at)?;
        earliest = match earliest {
            Some(existing) => Some(existing.min(created_at)),
            None => Some(created_at),
        };
    }
    Ok(earliest)
}

fn start_of_day(date: Date) -> OffsetDateTime {
    date.with_time(Time::MIDNIGHT).assume_utc()
}

fn end_of_day(date: Date) -> OffsetDateTime {
    let time = Time::from_hms_nano(23, 59, 59, 999_999_999).unwrap_or(Time::MIDNIGHT);
    date.with_time(time).assume_utc()
}

fn is_open_status(status: &TicketStatus) -> bool {
    matches!(
        status,
        TicketStatus::Open | TicketStatus::InProgress | TicketStatus::Blocked
    )
}

fn is_closed_status(status: &TicketStatus) -> bool {
    matches!(status, TicketStatus::Closed | TicketStatus::Archived)
}

fn normalize_filter_values(values: Vec<String>, label: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(TikError::usage(&format!("{label} filter cannot be empty")));
        }
        out.push(trimmed.to_lowercase());
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn normalize_bound(value: &str, kind: BoundKind) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(TikError::usage("date bound cannot be empty"));
    }
    if let Ok(parsed) =
        OffsetDateTime::parse(trimmed, &time::format_description::well_known::Rfc3339)
    {
        return format_rfc3339(parsed);
    }

    let parts: Vec<&str> = trimmed.split('-').collect();
    let (year, month, day) = match parts.len() {
        1 => (parse_year(parts[0])?, None, None),
        2 => (parse_year(parts[0])?, Some(parse_month(parts[1])?), None),
        3 => (
            parse_year(parts[0])?,
            Some(parse_month(parts[1])?),
            Some(parse_day(parts[2])?),
        ),
        _ => {
            return Err(TikError::usage(
                "date bound must be RFC3339 or YYYY[-MM[-DD]]",
            ))
        }
    };

    let (start_date, end_date) = match (month, day) {
        (None, None) => {
            let start = Date::from_calendar_date(year, Month::January, 1)
                .map_err(|err| TikError::usage(&format!("invalid year: {err}")))?;
            let end = Date::from_calendar_date(year + 1, Month::January, 1)
                .map_err(|err| TikError::usage(&format!("invalid year: {err}")))?
                - Duration::days(1);
            (start, end)
        }
        (Some(month), None) => {
            let start = Date::from_calendar_date(year, month, 1)
                .map_err(|err| TikError::usage(&format!("invalid month: {err}")))?;
            let (next_year, next_month) = match month {
                Month::December => (year + 1, Month::January),
                _ => (year, next_month(month)),
            };
            let end = Date::from_calendar_date(next_year, next_month, 1)
                .map_err(|err| TikError::usage(&format!("invalid month: {err}")))?
                - Duration::days(1);
            (start, end)
        }
        (Some(month), Some(day)) => {
            let date = Date::from_calendar_date(year, month, day)
                .map_err(|err| TikError::usage(&format!("invalid date: {err}")))?;
            (date, date)
        }
        (None, Some(_)) => {
            return Err(TikError::usage(
                "date bound must be RFC3339 or YYYY[-MM[-DD]]",
            ))
        }
    };

    let dt = match kind {
        BoundKind::Since => start_of_day(start_date),
        BoundKind::Until => end_of_day(end_date),
    };
    format_rfc3339(dt)
}

fn parse_year(value: &str) -> Result<i32> {
    if value.len() != 4 || !value.chars().all(|c| c.is_ascii_digit()) {
        return Err(TikError::usage("year must be 4 digits"));
    }
    value
        .parse::<i32>()
        .map_err(|_| TikError::usage("invalid year"))
}

fn parse_month(value: &str) -> Result<Month> {
    if value.len() != 2 || !value.chars().all(|c| c.is_ascii_digit()) {
        return Err(TikError::usage("month must be 2 digits"));
    }
    let value = value
        .parse::<u8>()
        .map_err(|_| TikError::usage("invalid month"))?;
    match value {
        1 => Ok(Month::January),
        2 => Ok(Month::February),
        3 => Ok(Month::March),
        4 => Ok(Month::April),
        5 => Ok(Month::May),
        6 => Ok(Month::June),
        7 => Ok(Month::July),
        8 => Ok(Month::August),
        9 => Ok(Month::September),
        10 => Ok(Month::October),
        11 => Ok(Month::November),
        12 => Ok(Month::December),
        _ => Err(TikError::usage("invalid month")),
    }
}

fn next_month(month: Month) -> Month {
    match month {
        Month::January => Month::February,
        Month::February => Month::March,
        Month::March => Month::April,
        Month::April => Month::May,
        Month::May => Month::June,
        Month::June => Month::July,
        Month::July => Month::August,
        Month::August => Month::September,
        Month::September => Month::October,
        Month::October => Month::November,
        Month::November => Month::December,
        Month::December => Month::January,
    }
}

fn parse_day(value: &str) -> Result<u8> {
    if value.len() != 2 || !value.chars().all(|c| c.is_ascii_digit()) {
        return Err(TikError::usage("day must be 2 digits"));
    }
    value
        .parse::<u8>()
        .map_err(|_| TikError::usage("invalid day"))
}

fn parse_rfc3339(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|dt| dt.to_offset(UtcOffset::UTC))
        .map_err(|err| TikError::Schema(format!("invalid timestamp {value}: {err}")))
}

fn format_rfc3339(value: OffsetDateTime) -> Result<String> {
    value
        .to_offset(UtcOffset::UTC)
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|err| TikError::internal(&format!("timestamp format error: {err}")))
}

fn list_contains_all(values: &[String], required: &[String]) -> bool {
    if required.is_empty() {
        return true;
    }
    let mut haystack = std::collections::HashSet::new();
    for value in values {
        haystack.insert(value.to_lowercase());
    }
    required.iter().all(|req| haystack.contains(req))
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
        created_at: ticket.created_at,
        updated_at: ticket.updated_at,
        milestone_id: ticket
            .milestone_id
            .as_ref()
            .map(|id| id.as_str().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::milestone::Milestone;
    use crate::domain::ids::TicketId;
    use crate::domain::ticket::{NewTicket, Ticket};
    use serde_json::json;

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
        let mut target = sample_ticket("B");
        target.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        ticket.relations.push(crate::domain::ticket::Relation {
            kind: crate::domain::ticket::RelationType::Blocks,
            id: target.id.clone(),
        });
        let graph =
            compute_graph_scoped(&[ticket, target], &[], &GraphOptions::default()).unwrap();
        assert_eq!(graph.edges.len(), 1);
    }

    #[test]
    fn graph_scopes_by_depth() {
        let mut ticket_a = sample_ticket("A");
        ticket_a.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut ticket_b = sample_ticket("B");
        ticket_b.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();
        let mut ticket_c = sample_ticket("C");
        ticket_c.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAX").unwrap();

        ticket_a.relations.push(crate::domain::ticket::Relation {
            kind: crate::domain::ticket::RelationType::Blocks,
            id: ticket_b.id.clone(),
        });
        ticket_b.relations.push(crate::domain::ticket::Relation {
            kind: crate::domain::ticket::RelationType::Blocks,
            id: ticket_c.id.clone(),
        });

        let options = GraphOptions {
            roots: vec![ticket_a.id.clone()],
            depth: Some(1),
            relations: Vec::new(),
            include_milestones: false,
        };
        let graph =
            compute_graph_scoped(&[ticket_a.clone(), ticket_b.clone(), ticket_c], &[], &options)
                .unwrap();
        assert!(graph.nodes.iter().any(|node| node.id == ticket_a.id.as_str()));
        assert!(graph.nodes.iter().any(|node| node.id == ticket_b.id.as_str()));
        assert_eq!(graph.nodes.len(), 2);
    }

    #[test]
    fn graph_includes_milestone_nodes() {
        let mut ticket = sample_ticket("A");
        ticket.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut milestone = Milestone::new(
            crate::domain::milestone::NewMilestone {
                title: "Phase 1".to_string(),
                description: None,
                due_at: None,
                tags: vec![],
            },
            "2026-01-01T00:00:00Z",
        );
        milestone.id =
            crate::domain::ids::MilestoneId::parse("M-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        ticket.milestone_id = Some(milestone.id.clone());

        let options = GraphOptions {
            roots: vec![ticket.id.clone()],
            depth: None,
            relations: Vec::new(),
            include_milestones: true,
        };
        let graph = compute_graph_scoped(&[ticket], &[milestone], &options).unwrap();
        assert!(graph.nodes.iter().any(|node| node.id.starts_with("M-")));
        assert!(graph.edges.iter().any(|edge| edge.relation == "milestone"));
    }

    #[test]
    fn burndown_counts_open_tickets() {
        let mut ticket_a = sample_ticket("A");
        ticket_a.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut ticket_b = sample_ticket("B");
        ticket_b.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();
        ticket_b.created_at = "2026-01-02T00:00:00Z".to_string();
        ticket_b.updated_at = ticket_b.created_at.clone();

        let events_a = vec![
            Event::new(
                "created",
                "human",
                "2026-01-01T00:00:00Z",
                json!({"ticket": ticket_a.clone()}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-03T12:00:00Z",
                json!({"from": "open", "to": "closed"}),
            ),
        ];
        let events_b = vec![
            Event::new(
                "created",
                "human",
                "2026-01-02T00:00:00Z",
                json!({"ticket": ticket_b.clone()}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-04T12:00:00Z",
                json!({"from": "open", "to": "closed"}),
            ),
        ];

        let histories = vec![
            TicketHistory {
                ticket: ticket_a,
                events: events_a,
            },
            TicketHistory {
                ticket: ticket_b,
                events: events_b,
            },
        ];

        let filters = ReportFilters::parse(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some("2026-01-01"),
            Some("2026-01-04"),
        )
        .unwrap();

        let report = compute_burndown(&histories, &filters, ReportGroupBy::Day).unwrap();
        let counts: Vec<usize> = report
            .points
            .iter()
            .map(|point| point.open_tickets)
            .collect();
        assert_eq!(counts, vec![1, 2, 1, 0]);
    }

    #[test]
    fn throughput_counts_closed_tickets() {
        let mut ticket_a = sample_ticket("A");
        ticket_a.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let mut ticket_b = sample_ticket("B");
        ticket_b.id = TicketId::parse("T-01ARZ3NDEKTSV4RRFFQ69G5FAW").unwrap();
        ticket_b.created_at = "2026-01-02T00:00:00Z".to_string();
        ticket_b.updated_at = ticket_b.created_at.clone();

        let events_a = vec![
            Event::new(
                "created",
                "human",
                "2026-01-01T00:00:00Z",
                json!({"ticket": ticket_a.clone()}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-03T12:00:00Z",
                json!({"from": "open", "to": "closed"}),
            ),
        ];
        let events_b = vec![
            Event::new(
                "created",
                "human",
                "2026-01-02T00:00:00Z",
                json!({"ticket": ticket_b.clone()}),
            ),
            Event::new(
                "status_change",
                "human",
                "2026-01-04T12:00:00Z",
                json!({"from": "open", "to": "closed"}),
            ),
        ];

        let histories = vec![
            TicketHistory {
                ticket: ticket_a,
                events: events_a,
            },
            TicketHistory {
                ticket: ticket_b,
                events: events_b,
            },
        ];

        let filters = ReportFilters::parse(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Some("2026-01-01"),
            Some("2026-01-04"),
        )
        .unwrap();

        let report = compute_throughput(&histories, &filters, ReportGroupBy::Day).unwrap();
        let counts: Vec<usize> = report
            .points
            .iter()
            .map(|point| point.closed_tickets)
            .collect();
        assert_eq!(counts, vec![0, 0, 1, 1]);
    }
}
