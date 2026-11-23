use super::{Space, SplitPair, Tab};

/// Max favicon boxes per grouped kept-tab row; matches the renderer.
const KEPT_PER_ROW: usize = 3;

/// Approximate row where a closing tab's exit animation belongs once the
/// sidebar is split into kept (favorite/pinned), folders, and regular tabs.
pub(crate) fn closing_tab_row_index(
    space: &Space,
    split: Option<&SplitPair>,
    split_expanded: bool,
    closed: &Tab,
    closed_index: usize,
) -> usize {
    let is_member = |id: &str| split.is_some_and(|pair| pair.first == id || pair.second == id);
    let kept_tabs = space
        .tabs
        .iter()
        .filter(|tab| tab.is_pinned() && !is_member(&tab.id))
        .collect::<Vec<_>>();

    // A kept tab with no group is its own full-width row. Grouped tabs share
    // rows of at most three boxes, in first-appearance order.
    let mut kept_order: Vec<Option<String>> = Vec::new();
    for tab in &kept_tabs {
        match &tab.kept_group {
            None => kept_order.push(None),
            Some(group) => {
                if !kept_order.contains(&Some(group.clone())) {
                    kept_order.push(Some(group.clone()));
                }
            }
        }
    }
    let members_of = |group: &str| -> Vec<&Tab> {
        kept_tabs
            .iter()
            .copied()
            .filter(|tab| tab.kept_group.as_deref() == Some(group))
            .collect()
    };
    let rows_for = |key: &Option<String>| -> usize {
        match key {
            None => 1,
            Some(group) => members_of(group).len().div_ceil(KEPT_PER_ROW).max(1),
        }
    };
    let kept_rows = kept_order
        .iter()
        .map(rows_for)
        .sum::<usize>()
        .max(1);

    if closed.is_pinned() {
        let mut row = 1usize;
        for key in &kept_order {
            match key {
                None => {
                    if kept_tabs
                        .iter()
                        .find(|tab| tab.kept_group.is_none())
                        .is_some_and(|tab| tab.id == closed.id)
                    {
                        return row;
                    }
                    row += 1;
                }
                Some(group) => {
                    if let Some(position) = members_of(group)
                        .iter()
                        .position(|tab| tab.id == closed.id)
                    {
                        return row + position / KEPT_PER_ROW;
                    }
                    row += rows_for(key);
                }
            }
        }
        return row;
    }

    let folder_rows: usize = space
        .folders
        .iter()
        .map(|folder| {
            1 + if folder.expanded {
                space
                    .tabs
                    .iter()
                    .filter(|tab| {
                        !tab.is_pinned()
                            && tab.folder.as_deref() == Some(folder.id.as_str())
                            && !is_member(&tab.id)
                    })
                    .count()
            } else {
                0
            }
        })
        .sum();

    if let Some(folder_id) = closed.folder.as_deref() {
        let mut row = 1 + kept_rows;
        for folder in &space.folders {
            row += 1;
            if folder.id == folder_id {
                let position = space
                    .tabs
                    .iter()
                    .take(closed_index)
                    .filter(|tab| {
                        !tab.is_pinned()
                            && tab.folder.as_deref() == Some(folder_id)
                            && !is_member(&tab.id)
                    })
                    .count();
                return row + position;
            }
            if folder.expanded {
                row += space
                    .tabs
                    .iter()
                    .filter(|tab| {
                        !tab.is_pinned()
                            && tab.folder.as_deref() == Some(folder.id.as_str())
                            && !is_member(&tab.id)
                    })
                    .count();
            }
        }
        return row;
    }

    let mut row = 1 + kept_rows + 1 + folder_rows;
    if split.is_some() {
        row += 1 + if split_expanded { 2 } else { 0 };
    }
    row + space
        .tabs
        .iter()
        .take(closed_index)
        .filter(|tab| !tab.is_pinned() && tab.folder.is_none() && !is_member(&tab.id))
        .count()
}
