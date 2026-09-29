//! Only a complete strict element match can turn NOT_FOUND into false.
use actl_core::{CtlError, ErrorCode};
pub fn outcome(result: Result<(), CtlError>, element_exists: bool) -> Result<bool, CtlError> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.code == ErrorCode::AssertionFailed => Ok(false),
        Err(error)
            if element_exists
                && error.code == ErrorCode::NotFound
                && error.evidence.as_ref().is_some_and(|e| {
                    e["phase"] == "match" && e["count"] == 0 && e["truncated"] == false
                }) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_confirmed_target_absence_is_false() {
        for (phase, expected) in [("match", true), ("within", false), ("window", false)] {
            let error = CtlError::with_evidence(
                ErrorCode::NotFound,
                "missing",
                serde_json::json!({"phase":phase,"count":0,"truncated":false}),
            );
            let result = outcome(Err(error), true);
            if expected {
                assert!(!result.unwrap());
            } else {
                assert!(result.is_err());
            }
        }
        assert!(outcome(Err(CtlError::new(ErrorCode::NotFound, "unknown")), true).is_err());
        assert!(
            outcome(
                Err(CtlError::with_evidence(
                    ErrorCode::NotFound,
                    "partial",
                    serde_json::json!({"phase":"match","count":0,"truncated":true})
                )),
                true
            )
            .is_err()
        );
    }
}
