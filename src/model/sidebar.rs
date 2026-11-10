use super::{Space, SplitPair, Tab};

pub(crate) const FAVORITES_PER_ROW: usize = 6;

/// Approximate row where a closing tab's exit animation belongs once the
/// sidebar is split into favorites, pinned, folders, and regular tabs.
pub(crate) fn closing_tab_row_index(
    space: &Space,
    split: Option<&SplitPair>,
    split_expanded: bool,
    closed: &Tab,
    closed_index: usize,
) -> usize {
    let is_member = |id: &str| split.is_some_and(|pair| pair.first == id || pair.second == id);
    let favorites = space
        .tabs
        .iter()
        .filter(|tab| tab.favorite && !is_member(&tab.id))
        .collect::<Vec<_>>();
    let pinned = space
        .tabs
        .iter()
        .filter(|tab| !tab.favorite && tab.pinned && !is_member(&tab.id))
        .count();
    let favorites_rows = if favorites.is_empty() {
        1
    } else {
        favorites.len().div_ceil(FAVORITES_PER_ROW)
    };
    let pinned_rows = pinned.max(1);
    let folder_rows: usize = space
        .folders
        .iter()
        .map(|folder| {
            1 + if folder.expanded {
                space
                    .tabs
                    .iter()
                    .filter(|tab| {
                        !tab.favorite
                            && !tab.pinned
                            && tab.folder.as_deref() == Some(folder.id.as_str())
                            && !is_member(&tab.id)
                    })
                    .count()
            } else {
                0
            }
        })
        .sum();

    if closed.favorite {
        let position = favorites
            .iter()
            .position(|tab| tab.id == closed.id)
            .unwrap_or(favorites.len());
        return 1 + position / FAVORITES_PER_ROW;
    }
    if closed.pinned {
        let position = space
            .tabs
            .iter()
            .filter(|tab| !tab.favorite && tab.pinned && !is_member(&tab.id))
            .position(|tab| tab.id == closed.id)
            .unwrap_or(pinned);
        return 1 + favorites_rows + 1 + position;
    }
    if let Some(folder_id) = closed.folder.as_deref() {
        let mut row = 1 + favorites_rows + 1 + pinned_rows;
        for folder in &space.folders {
            row += 1;
            if folder.id == folder_id {
                let position = space
                    .tabs
                    .iter()
                    .take(closed_index)
                    .filter(|tab| {
                        !tab.favorite
                            && !tab.pinned
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
                        !tab.favorite
                            && !tab.pinned
                            && tab.folder.as_deref() == Some(folder.id.as_str())
                            && !is_member(&tab.id)
                    })
                    .count();
            }
        }
        return row;
    }

    let mut row = 1 + favorites_rows + 1 + pinned_rows + 1 + folder_rows;
    if split.is_some() {
        row += 1 + if split_expanded { 2 } else { 0 };
    }
    row + space
        .tabs
        .iter()
        .take(closed_index)
        .filter(|tab| !tab.favorite && !tab.pinned && tab.folder.is_none() && !is_member(&tab.id))
        .count()
}
