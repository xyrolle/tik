use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::{Result, TikError};

pub fn now_rfc3339() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|err| TikError::internal(&format!("timestamp format error: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_rfc3339_formats() {
        let ts = now_rfc3339().unwrap();
        assert!(ts.contains('T'));
        assert!(ts.ends_with('Z'));
    }
}
