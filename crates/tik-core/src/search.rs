use std::collections::HashSet;

use crate::domain::ids::MilestoneId;
use crate::domain::ticket::Ticket;
use crate::{Result, TikError};

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text_tokens: Vec<String>,
    pub fts_query: String,
    pub filters: SearchFilters,
}

impl SearchQuery {
    pub fn parse(input: &str) -> Result<SearchQuery> {
        let input = input.trim();
        if input.is_empty() {
            return Err(TikError::usage("search query cannot be empty"));
        }

        let tokens = shell_words::split(input)
            .map_err(|err| TikError::usage(&format!("invalid query: {err}")))?;

        let mut filters = SearchFilters::default();
        let mut text_tokens = Vec::new();

        for token in tokens {
            if let Some((field, value)) = token.split_once(':') {
                if apply_filter(field, value, &mut filters)? {
                    continue;
                }
            }
            push_text_token(&token, &mut text_tokens);
        }

        let text_tokens = normalize_text_tokens(text_tokens);
        let fts_query = text_tokens.join(" ");

        if text_tokens.is_empty() && filters.is_empty() {
            return Err(TikError::usage("search query cannot be empty"));
        }

        Ok(SearchQuery {
            text_tokens,
            fts_query,
            filters,
        })
    }

    pub fn matches_ticket(&self, ticket: &Ticket, notes: &str) -> bool {
        if !self.filters.matches_ticket(ticket) {
            return false;
        }

        if self.text_tokens.is_empty() {
            return true;
        }

        let mut haystack = String::new();
        haystack.push_str(&ticket.title);
        haystack.push(' ');
        haystack.push_str(&ticket.summary);
        haystack.push(' ');
        haystack.push_str(&ticket.description);
        haystack.push(' ');
        haystack.push_str(notes);
        haystack.push(' ');
        haystack.push_str(&ticket.tags.join(" "));
        haystack.push(' ');
        haystack.push_str(&ticket.assignees.join(" "));

        text_matches(&self.text_tokens, &haystack)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub status: Vec<String>,
    pub tags: Vec<String>,
    pub assignees: Vec<String>,
    pub types: Vec<String>,
    pub priorities: Vec<String>,
    pub severities: Vec<String>,
    pub milestone_ids: Vec<String>,
    pub created: DateFilter,
    pub updated: DateFilter,
    pub closed: DateFilter,
    pub due: DateFilter,
}

impl SearchFilters {
    pub fn is_empty(&self) -> bool {
        self.status.is_empty()
            && self.tags.is_empty()
            && self.assignees.is_empty()
            && self.types.is_empty()
            && self.priorities.is_empty()
            && self.severities.is_empty()
            && self.milestone_ids.is_empty()
            && self.created.is_empty()
            && self.updated.is_empty()
            && self.closed.is_empty()
            && self.due.is_empty()
    }

    pub fn matches_ticket(&self, ticket: &Ticket) -> bool {
        if !self.status.is_empty()
            && !self.status.contains(&ticket.status.as_str().to_string())
        {
            return false;
        }
        if !self.types.is_empty() && !self.types.contains(&ticket.kind.as_str().to_string()) {
            return false;
        }
        if !self.priorities.is_empty()
            && !self
                .priorities
                .contains(&ticket.priority.as_str().to_string())
        {
            return false;
        }
        if !self.severities.is_empty()
            && !self
                .severities
                .contains(&ticket.severity.as_str().to_string())
        {
            return false;
        }
        if !self.tags.is_empty() && !list_contains_all(&ticket.tags, &self.tags) {
            return false;
        }
        if !self.assignees.is_empty() && !list_contains_all(&ticket.assignees, &self.assignees) {
            return false;
        }
        if !self.milestone_ids.is_empty() {
            match &ticket.milestone_id {
                Some(id) => {
                    if !self.milestone_ids.contains(&id.as_str().to_string()) {
                        return false;
                    }
                }
                None => return false,
            }
        }
        if !self.created.matches(Some(&ticket.created_at)) {
            return false;
        }
        if !self.updated.matches(Some(&ticket.updated_at)) {
            return false;
        }
        if !self.closed.matches(ticket.closed_at.as_deref()) {
            return false;
        }
        if !self.due.matches(ticket.due_at.as_deref()) {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct DateFilter {
    pub after: Option<DateBound>,
    pub before: Option<DateBound>,
    pub prefix: Option<String>,
}

impl DateFilter {
    pub fn is_empty(&self) -> bool {
        self.after.is_none() && self.before.is_none() && self.prefix.is_none()
    }

    pub fn matches(&self, value: Option<&str>) -> bool {
        if self.is_empty() {
            return true;
        }
        let value = match value {
            Some(value) => value,
            None => return false,
        };
        if let Some(prefix) = &self.prefix {
            return value.starts_with(prefix);
        }
        if let Some(after) = &self.after {
            if after.inclusive {
                if value < after.value.as_str() {
                    return false;
                }
            } else if value <= after.value.as_str() {
                return false;
            }
        }
        if let Some(before) = &self.before {
            if before.inclusive {
                if value > before.value.as_str() {
                    return false;
                }
            } else if value >= before.value.as_str() {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
pub struct DateBound {
    pub value: String,
    pub inclusive: bool,
}

fn apply_filter(field: &str, value: &str, filters: &mut SearchFilters) -> Result<bool> {
    let field = field.trim().to_lowercase();
    let value = value.trim();
    if value.is_empty() {
        return Err(TikError::usage("search filter value cannot be empty"));
    }

    match field.as_str() {
        "status" => {
            for v in split_values(value) {
                filters.status.push(validate_enum("status", &v, &VALID_STATUS)?);
            }
            Ok(true)
        }
        "tag" | "tags" => {
            for v in split_values(value) {
                filters.tags.push(v.to_lowercase());
            }
            Ok(true)
        }
        "assignee" | "assignees" => {
            for v in split_values(value) {
                filters.assignees.push(v.to_lowercase());
            }
            Ok(true)
        }
        "type" => {
            for v in split_values(value) {
                filters.types.push(validate_enum("type", &v, &VALID_TYPES)?);
            }
            Ok(true)
        }
        "priority" => {
            for v in split_values(value) {
                filters
                    .priorities
                    .push(validate_enum("priority", &v, &VALID_PRIORITIES)?);
            }
            Ok(true)
        }
        "severity" => {
            for v in split_values(value) {
                filters
                    .severities
                    .push(validate_enum("severity", &v, &VALID_SEVERITIES)?);
            }
            Ok(true)
        }
        "milestone" | "milestone_id" => {
            for v in split_values(value) {
                let id = MilestoneId::parse(v)?;
                filters.milestone_ids.push(id.as_str().to_string());
            }
            Ok(true)
        }
        "created" => {
            filters.created = parse_date_filter(value)?;
            Ok(true)
        }
        "updated" => {
            filters.updated = parse_date_filter(value)?;
            Ok(true)
        }
        "closed" => {
            filters.closed = parse_date_filter(value)?;
            Ok(true)
        }
        "due" => {
            filters.due = parse_date_filter(value)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn split_values(value: &str) -> Vec<&str> {
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect()
}

fn validate_enum(field: &str, value: &str, allowed: &[&str]) -> Result<String> {
    let value = value.to_lowercase();
    if !allowed.contains(&value.as_str()) {
        return Err(TikError::usage(&format!("invalid {field}: {value}")));
    }
    Ok(value)
}

fn parse_date_filter(value: &str) -> Result<DateFilter> {
    if value.contains("..") {
        let mut parts = value.splitn(2, "..");
        let start = parts.next().unwrap_or_default().trim();
        let end = parts.next().unwrap_or_default().trim();
        let mut filter = DateFilter::default();
        if !start.is_empty() {
            filter.after = Some(DateBound {
                value: start.to_string(),
                inclusive: true,
            });
        }
        if !end.is_empty() {
            filter.before = Some(DateBound {
                value: end.to_string(),
                inclusive: true,
            });
        }
        return Ok(filter);
    }

    for (op, inclusive, is_after) in [
        (">=", true, true),
        (">", false, true),
        ("<=", true, false),
        ("<", false, false),
    ] {
        if let Some(rest) = value.strip_prefix(op) {
            let rest = rest.trim();
            if rest.is_empty() {
                return Err(TikError::usage("date filter value cannot be empty"));
            }
            let mut filter = DateFilter::default();
            let bound = DateBound {
                value: rest.to_string(),
                inclusive,
            };
            if is_after {
                filter.after = Some(bound);
            } else {
                filter.before = Some(bound);
            }
            return Ok(filter);
        }
    }

    Ok(DateFilter {
        prefix: Some(value.to_string()),
        ..DateFilter::default()
    })
}

fn push_text_token(token: &str, out: &mut Vec<String>) {
    if let Some(rest) = token.strip_prefix('-') {
        if !rest.is_empty() {
            out.push("NOT".to_string());
            out.push(rest.to_string());
            return;
        }
    }
    let upper = token.to_uppercase();
    if upper == "AND" || upper == "OR" || upper == "NOT" {
        out.push(upper);
    } else {
        out.push(token.to_string());
    }
}

fn normalize_text_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut prev_is_term = false;
    for token in tokens {
        let is_op = token == "AND" || token == "OR" || token == "NOT";
        if prev_is_term && (!is_op || token == "NOT") {
            out.push("AND".to_string());
        }
        out.push(token.clone());
        prev_is_term = !is_op || token == ")";
    }
    out
}

fn list_contains_all(values: &[String], required: &[String]) -> bool {
    let set: HashSet<String> = values.iter().map(|v| v.to_lowercase()).collect();
    required.iter().all(|value| {
        let needle = value.to_lowercase();
        if needle.is_empty() {
            return true;
        }
        if !set.contains(&needle) {
            return false;
        }
        true
    })
}

fn text_matches(tokens: &[String], haystack: &str) -> bool {
    if tokens.is_empty() {
        return true;
    }

    let haystack = haystack.to_lowercase();
    let mut output: Vec<TextToken> = Vec::new();
    let mut ops: Vec<TextToken> = Vec::new();

    for token in tokens {
        let token = token.as_str();
        match token {
            "AND" | "OR" | "NOT" => {
                let current = TextToken::from_op(token);
                let prec = current.precedence();
                while let Some(top) = ops.last() {
                    if top.precedence() >= prec {
                        output.push(ops.pop().unwrap());
                    } else {
                        break;
                    }
                }
                ops.push(current);
            }
            _ => output.push(TextToken::Term(token.to_string())),
        }
    }
    while let Some(op) = ops.pop() {
        output.push(op);
    }

    let mut stack: Vec<bool> = Vec::new();
    for token in output {
        match token {
            TextToken::Term(term) => {
                let term = term.to_lowercase();
                stack.push(haystack.contains(&term));
            }
            TextToken::Not => {
                if let Some(val) = stack.pop() {
                    stack.push(!val);
                } else {
                    return false;
                }
            }
            TextToken::And => {
                if stack.len() < 2 {
                    return false;
                }
                let rhs = stack.pop().unwrap();
                let lhs = stack.pop().unwrap();
                stack.push(lhs && rhs);
            }
            TextToken::Or => {
                if stack.len() < 2 {
                    return false;
                }
                let rhs = stack.pop().unwrap();
                let lhs = stack.pop().unwrap();
                stack.push(lhs || rhs);
            }
        }
    }

    stack.pop().unwrap_or(false)
}

#[derive(Debug, Clone)]
enum TextToken {
    Term(String),
    And,
    Or,
    Not,
}

impl TextToken {
    fn from_op(op: &str) -> TextToken {
        match op {
            "AND" => TextToken::And,
            "OR" => TextToken::Or,
            _ => TextToken::Not,
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            TextToken::Not => 3,
            TextToken::And => 2,
            TextToken::Or => 1,
            TextToken::Term(_) => 0,
        }
    }
}

const VALID_STATUS: [&str; 5] = ["open", "in_progress", "blocked", "closed", "archived"];
const VALID_TYPES: [&str; 5] = ["feature", "bug", "chore", "task", "spike"];
const VALID_PRIORITIES: [&str; 4] = ["low", "medium", "high", "critical"];
const VALID_SEVERITIES: [&str; 4] = ["low", "normal", "high", "critical"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ticket::{NewTicket, Ticket};

    fn sample_ticket() -> Ticket {
        Ticket::new(
            NewTicket {
                title: "Title".to_string(),
                summary: Some("Summary".to_string()),
                description: Some("Description".to_string()),
                tags: vec!["mvp".to_string()],
            },
            "2026-01-01T00:00:00Z",
        )
    }

    #[test]
    fn parse_rejects_empty_query() {
        let err = SearchQuery::parse(" ").unwrap_err();
        assert!(matches!(err, TikError::Usage(_)));
    }

    #[test]
    fn parse_collects_filters_and_text() {
        let query = SearchQuery::parse("status:open tag:mvp hello").unwrap();
        assert_eq!(query.filters.status, vec!["open"]);
        assert_eq!(query.filters.tags, vec!["mvp"]);
        assert_eq!(query.text_tokens, vec!["hello"]);
    }

    #[test]
    fn matches_ticket_applies_filters() {
        let ticket = sample_ticket();
        let query = SearchQuery::parse("status:open tag:mvp").unwrap();
        assert!(query.matches_ticket(&ticket, ""));
        let query = SearchQuery::parse("status:closed").unwrap();
        assert!(!query.matches_ticket(&ticket, ""));
    }

    #[test]
    fn text_matches_respects_not() {
        let ticket = sample_ticket();
        let query = SearchQuery::parse("Title NOT missing").unwrap();
        assert!(query.matches_ticket(&ticket, ""));
    }
}
