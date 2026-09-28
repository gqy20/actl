//! Window inventories must preserve enumeration and classification failures.
use actl_core::CtlError;

pub(crate) fn inventory<T: Clone>(
    roots: Result<Vec<T>, CtlError>,
    mut children: impl FnMut(&T) -> Result<Vec<T>, CtlError>,
    mut is_window: impl FnMut(&T) -> Result<bool, CtlError>,
) -> Result<Vec<T>, CtlError> {
    let mut out = Vec::new();
    for root in roots? {
        actl_core::wait_control::check()?;
        out.push(root.clone());
        for child in children(&root)? {
            if is_window(&child)? {
                out.push(child);
            }
        }
    }
    Ok(out)
}

pub(crate) fn select<T>(
    items: Vec<T>,
    mut predicate: impl FnMut(&T) -> Result<bool, CtlError>,
) -> Result<Vec<T>, CtlError> {
    let mut out = Vec::new();
    for item in items {
        actl_core::wait_control::check()?;
        if predicate(&item)? {
            out.push(item);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actl_core::ErrorCode;

    fn failed() -> CtlError {
        CtlError::new(ErrorCode::StaleRef, "provider rebuilt")
    }

    #[test]
    fn root_failure_is_not_empty_inventory() {
        assert!(inventory::<u32>(Err(failed()), |_| Ok(vec![]), |_| Ok(true)).is_err());
    }
    #[test]
    fn inventory_failure_envelope_contract() {
        let native = uiautomation::errors::Error::new(-2147220991, "provider rebuilt");
        let error = inventory::<u32>(
            Err(crate::read_channel::failure("children", native)),
            |_| Ok(vec![]),
            |_| Ok(true),
        )
        .unwrap_err();
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../tests/golden/window_inventory_failed.json"))
                .unwrap();
        assert_eq!(
            serde_json::to_value(actl_core::ErrorEnvelope::new("list-windows", &error, 0)).unwrap(),
            expected
        );
    }
    #[test]
    fn child_failure_is_not_partial_inventory() {
        assert!(inventory(Ok(vec![1]), |_| Err(failed()), |_| Ok(true)).is_err());
    }
    #[test]
    fn classification_failure_is_not_missing_dialog() {
        assert!(inventory(Ok(vec![1]), |_| Ok(vec![2]), |_| Err(failed())).is_err());
    }
    #[test]
    fn failed_pid_read_cannot_prove_unique_match() {
        assert!(
            select(vec![1, 2], |n| if *n == 1 {
                Ok(true)
            } else {
                Err(failed())
            })
            .is_err()
        );
    }
    #[test]
    fn empty_and_filtered_children_are_valid() {
        assert_eq!(
            inventory(Ok(vec![1]), |_| Ok(vec![2, 3]), |n| Ok(*n == 3)).unwrap(),
            vec![1, 3]
        );
        assert!(
            inventory::<u32>(Ok(vec![]), |_| Ok(vec![]), |_| Ok(true))
                .unwrap()
                .is_empty()
        );
    }
}
