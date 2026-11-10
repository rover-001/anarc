//! Example plugin metadata and a deterministic tab-sorting helper.
//!
//! The host integration glue should call `sorted_ids` with the result of the
//! WIT `list-tabs` import and activate the first tab after sorting. This example
//! intentionally has no host-side filesystem or network dependencies.

use lumen_plugin_sdk::PluginInfo;

pub const PLUGIN: PluginInfo = lumen_plugin_sdk::plugin_info!(
    id: "dev.example.tidy-tabs",
    version: "0.1.0",
    api: "1",
);

/// Sort tab IDs by title while leaving pinned tabs ahead of regular tabs.
/// Input tuples are `(id, title, pinned)`.
pub fn sorted_ids(tabs: &[(String, String, bool)]) -> Vec<String> {
    let mut tabs = tabs.to_vec();
    tabs.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
            .then_with(|| left.0.cmp(&right.0))
    });
    tabs.into_iter().map(|(id, _, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_tabs_stay_first_and_regular_tabs_sort_by_title() {
        let input = vec![
            ("tab-z".into(), "Zebra".into(), false),
            ("tab-b".into(), "Beta".into(), false),
            ("tab-a".into(), "Alpha".into(), true),
        ];
        assert_eq!(sorted_ids(&input), ["tab-a", "tab-b", "tab-z"]);
    }
}
