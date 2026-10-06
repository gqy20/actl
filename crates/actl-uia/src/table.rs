//! Table extraction only falls back after confirmed absence of GridPattern.
use crate::read_channel::{available, failure};
use actl_core::{CtlError, ErrorCode, target::Target};
use uiautomation::{UIElement, patterns::UIGridPattern, types::UIProperty};

pub struct TableResult {
    pub rows: Vec<Vec<String>>,
    pub source: &'static str,
    pub fallback_reason: Option<&'static str>,
}

fn dimensions(rows: i32, cols: i32) -> Result<(usize, usize), CtlError> {
    if rows < 0 || cols < 0 {
        return Err(CtlError::internal("grid returned negative dimensions"));
    }
    let (rows, cols) = (rows as usize, cols as usize);
    if rows > crate::MAX_ELEMENTS
        || cols > crate::MAX_ELEMENTS
        || rows
            .checked_mul(cols)
            .is_none_or(|n| n > crate::MAX_ELEMENTS)
    {
        return Err(crate::observation::incomplete(&["table_limit"]));
    }
    Ok((rows, cols))
}

fn grid_rows(
    rows: i32,
    cols: i32,
    mut cell: impl FnMut(usize, usize) -> Result<String, CtlError>,
) -> Result<Vec<Vec<String>>, CtlError> {
    let (rows, cols) = dimensions(rows, cols)?;
    let mut table = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for c in 0..cols {
            actl_core::wait_control::check()?;
            row.push(cell(r, c)?);
        }
        table.push(row);
    }
    Ok(table)
}

fn cell_text(element: &UIElement) -> Result<String, CtlError> {
    match crate::read::read_value(element)? {
        Some(value) => Ok(value),
        None => element
            .get_name()
            .map_err(|e| failure("table.cell.name", e)),
    }
}

pub fn extract_table(app: Option<&str>, target: &Target) -> Result<TableResult, CtlError> {
    let loc = crate::locate::locate(app, target, None)?;
    if available(
        &loc.element,
        UIProperty::IsGridPatternAvailable,
        "grid-pattern",
    )? {
        let grid = loc
            .element
            .get_pattern::<UIGridPattern>()
            .map_err(|e| failure("grid-pattern", e))?;
        let rows = grid.get_row_count().map_err(|e| failure("grid.rows", e))?;
        let cols = grid
            .get_column_count()
            .map_err(|e| failure("grid.columns", e))?;
        let table = grid_rows(rows, cols, |r, c| {
            let element = grid
                .get_item(r as i32, c as i32)
                .map_err(|e| failure("grid.cell", e))?;
            cell_text(&element)
        })?;
        if grid.get_row_count().map_err(|e| failure("grid.rows", e))? != rows
            || grid
                .get_column_count()
                .map_err(|e| failure("grid.columns", e))?
                != cols
        {
            return Err(crate::observation::incomplete(&["table_changed"]));
        }
        return Ok(TableResult {
            rows: table,
            source: "grid-pattern",
            fallback_reason: None,
        });
    }
    let entries = crate::observation::tree_live(&loc.element)?.require_complete()?;
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut active_row: Option<(u32, usize)> = None;
    for entry in entries.into_iter().skip(1) {
        if active_row.is_some_and(|(depth, _)| entry.depth <= depth) {
            active_row = None;
        }
        let role = entry
            .element
            .get_control_type()
            .map_err(|e| failure("table.role", e))?;
        use uiautomation::types::ControlType;
        if matches!(role, ControlType::DataItem | ControlType::ListItem) && active_row.is_none() {
            active_row = Some((entry.depth, table.len()));
            table.push(Vec::new());
        } else if matches!(role, ControlType::Text | ControlType::Edit)
            && let Some((_, row)) = active_row
        {
            table[row].push(cell_text(&entry.element)?);
        }
    }
    if table.is_empty() {
        return Err(CtlError::new(
            ErrorCode::NotActionable,
            "observed tree contains no table rows",
        ));
    }
    Ok(TableResult {
        rows: table,
        source: "tree-flatten",
        fallback_reason: Some("grid_pattern_unsupported"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_failure_never_returns_partial_table() {
        let result = grid_rows(2, 2, |r, c| {
            if r == 1 && c == 0 {
                Err(CtlError::new(ErrorCode::StaleRef, "recreated"))
            } else {
                Ok(String::new())
            }
        });
        assert!(matches!(result, Err(e) if e.code == ErrorCode::StaleRef));
    }
    #[test]
    fn empty_cells_and_empty_grid_are_preserved() {
        assert_eq!(
            grid_rows(1, 2, |_, _| Ok(String::new())).unwrap(),
            vec![vec!["", ""]]
        );
        assert!(
            grid_rows(0, 2, |_, _| panic!("no cells"))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn invalid_or_large_dimensions_are_not_a_fallback() {
        assert!(dimensions(-1, 2).is_err());
        assert!(dimensions(i32::MAX, i32::MAX).is_err());
        assert!(dimensions(100, 100).is_err());
    }
}
