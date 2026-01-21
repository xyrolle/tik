use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use ulid::Ulid;

use crate::{Result, TikError};

const ULID_LEN: usize = 26;
const ULID_CHARS: &str = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TicketId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MilestoneId(String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(String);

impl Default for TicketId {
    fn default() -> Self {
        Self::new()
    }
}

impl TicketId {
    pub fn new() -> Self {
        Self(format!("T-{}", Ulid::new()))
    }

    pub fn parse(input: &str) -> Result<Self> {
        parse_prefixed_id(input, "T-").map(TicketId)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MilestoneId {
    fn default() -> Self {
        Self::new()
    }
}

impl MilestoneId {
    pub fn new() -> Self {
        Self(format!("M-{}", Ulid::new()))
    }

    pub fn parse(input: &str) -> Result<Self> {
        parse_prefixed_id(input, "M-").map(MilestoneId)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl EventId {
    pub fn new() -> Self {
        Self(format!("E-{}", Ulid::new()))
    }

    pub fn parse(input: &str) -> Result<Self> {
        parse_prefixed_id(input, "E-").map(EventId)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for MilestoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for TicketId {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        TicketId::parse(s)
    }
}

impl FromStr for MilestoneId {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        MilestoneId::parse(s)
    }
}

impl FromStr for EventId {
    type Err = TikError;

    fn from_str(s: &str) -> Result<Self> {
        EventId::parse(s)
    }
}

fn parse_prefixed_id(input: &str, prefix: &str) -> Result<String> {
    if !input.starts_with(prefix) {
        return Err(TikError::Schema(format!("expected prefix {prefix}")));
    }

    let raw = &input[prefix.len()..];
    if !is_ulid(raw) {
        return Err(TikError::Schema("invalid ulid".to_string()));
    }

    Ok(input.to_string())
}

fn is_ulid(value: &str) -> bool {
    if value.len() != ULID_LEN {
        return false;
    }
    value.chars().all(|ch| ULID_CHARS.contains(ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_id_new_has_prefix() {
        let id = TicketId::new();
        assert!(id.as_str().starts_with("T-"));
        assert_eq!(id.as_str().len(), 2 + ULID_LEN);
    }

    #[test]
    fn milestone_id_new_has_prefix() {
        let id = MilestoneId::new();
        assert!(id.as_str().starts_with("M-"));
        assert_eq!(id.as_str().len(), 2 + ULID_LEN);
    }

    #[test]
    fn event_id_new_has_prefix() {
        let id = EventId::new();
        assert!(id.as_str().starts_with("E-"));
        assert_eq!(id.as_str().len(), 2 + ULID_LEN);
    }

    #[test]
    fn parse_ticket_id_validates_prefix() {
        let id = TicketId::new();
        let parsed = TicketId::parse(id.as_str()).unwrap();
        assert_eq!(parsed.as_str(), id.as_str());

        let err = TicketId::parse("X-00000000000000000000000000").unwrap_err();
        assert!(matches!(err, TikError::Schema(_)));
    }

    #[test]
    fn parse_ticket_id_rejects_invalid_ulid() {
        let err = TicketId::parse("T-0000").unwrap_err();
        assert!(matches!(err, TikError::Schema(_)));
    }

    #[test]
    fn ulid_validation_accepts_valid_charset() {
        assert!(is_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAV"));
        assert!(!is_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAI"));
        assert!(!is_ulid("01ARZ3NDEKTSV4RRFFQ69G5faV"));
    }
}
