use crate::history::HistoryEntry;
use crate::plugin::{PaletteCommand, PluginRow};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteTarget {
    Tab { space_id: String, tab_id: String },
    History(String),
    Action(PaletteAction),
    PluginToggle { id: String, enabled: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteAction {
    NewTab,
    CloseTab,
    ReopenClosedTab,
    Reload,
    FindInPage,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    NewSpace,
    EnablePlugin,
    DisablePlugin,
    ReloadPlugins,
    OpenPluginFolder,
}

impl PaletteAction {
    pub fn from_plugin_id(id: &str) -> Option<Self> {
        Some(match id {
            "new-tab" => Self::NewTab,
            "close-tab" => Self::CloseTab,
            "reopen-closed-tab" => Self::ReopenClosedTab,
            "reload" => Self::Reload,
            "find-in-page" => Self::FindInPage,
            "zoom-in" => Self::ZoomIn,
            "zoom-out" => Self::ZoomOut,
            "zoom-reset" => Self::ZoomReset,
            "new-space" => Self::NewSpace,
            "enable-plugin" => Self::EnablePlugin,
            "disable-plugin" => Self::DisablePlugin,
            "reload-plugins" => Self::ReloadPlugins,
            "open-plugin-folder" => Self::OpenPluginFolder,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteItem {
    pub title: String,
    pub detail: String,
    pub target: PaletteTarget,
    score: i32,
}

#[derive(Clone, Debug)]
pub struct PaletteTab<'a> {
    pub space_id: &'a str,
    pub space_name: &'a str,
    pub tab_id: &'a str,
    pub title: &'a str,
    pub url: &'a str,
}

pub fn rank_items(
    query: &str,
    tabs: &[PaletteTab<'_>],
    history: &[HistoryEntry],
    actions: &[PaletteCommand],
    plugins: &[PluginRow],
) -> Vec<PaletteItem> {
    let mut items = Vec::new();
    for tab in tabs {
        let title = if tab.title.is_empty() {
            tab.url
        } else {
            tab.title
        };
        let haystack = format!("{title} {} {}", tab.url, tab.space_name);
        if let Some(score) = fuzzy_score(query, &haystack) {
            items.push(PaletteItem {
                title: title.to_string(),
                detail: format!("{} · {}", tab.space_name, tab.url),
                target: PaletteTarget::Tab {
                    space_id: tab.space_id.to_string(),
                    tab_id: tab.tab_id.to_string(),
                },
                score: score + 10,
            });
        }
    }
    for entry in history {
        let title = if entry.title.is_empty() {
            &entry.url
        } else {
            &entry.title
        };
        if let Some(score) = fuzzy_score(query, &format!("{title} {}", entry.url)) {
            items.push(PaletteItem {
                title: title.to_string(),
                detail: entry.url.clone(),
                target: PaletteTarget::History(entry.url.clone()),
                score: score + (entry.visit_count.min(20) as i32),
            });
        }
    }
    for command in actions {
        let Some(action) = PaletteAction::from_plugin_id(command.id) else {
            continue;
        };
        if let Some(score) = fuzzy_score(query, command.title) {
            items.push(PaletteItem {
                title: command.title.to_string(),
                detail: command.detail.to_string(),
                target: PaletteTarget::Action(action),
                score: score + 30,
            });
        }
    }
    for plugin in plugins
        .iter()
        .filter(|plugin| plugin.descriptor.can_toggle && !query.trim().is_empty())
    {
        let (enabled, verb) = if plugin.enabled {
            (false, "Disable")
        } else {
            (true, "Enable")
        };
        let title = format!("{verb} {}", plugin.descriptor.name);
        let search_text = format!(
            "{verb} plugin {} {} {}",
            plugin.descriptor.name, plugin.descriptor.id, plugin.descriptor.description
        );
        if let Some(score) = fuzzy_score(query, &search_text) {
            items.push(PaletteItem {
                title,
                detail: format!("Plugin · {}", plugin.descriptor.id),
                target: PaletteTarget::PluginToggle {
                    id: plugin.descriptor.id.clone(),
                    enabled,
                },
                score: score + 28,
            });
        }
    }
    items.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    items.truncate(12);
    items
}

fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let text = text.to_lowercase();
    if let Some(position) = text.find(&query) {
        return Some(100 - position.min(80) as i32 - (text.len() - query.len()).min(40) as i32 / 5);
    }
    let mut next = 0;
    let mut first = None;
    let mut previous = None;
    let mut score = 0;
    for (index, ch) in text.char_indices() {
        if query[next..].starts_with(ch) {
            first.get_or_insert(index);
            if previous.is_some_and(|prev| prev == index) {
                score += 8;
            }
            score += if index == 0 || text[..index].ends_with([' ', '/', '-', '_']) {
                8
            } else {
                1
            };
            previous = Some(index + ch.len_utf8());
            next += ch.len_utf8();
            if next == query.len() {
                break;
            }
        }
    }
    if next != query.len() {
        return None;
    }
    Some(40 + score - first.unwrap_or(0).min(30) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_exact_tab_matches_before_partial_matches_and_actions() {
        let tabs = [
            PaletteTab {
                space_id: "personal",
                space_name: "Personal",
                tab_id: "a",
                title: "GitHub",
                url: "https://github.com",
            },
            PaletteTab {
                space_id: "work",
                space_name: "Work",
                tab_id: "b",
                title: "GitLab",
                url: "https://gitlab.com",
            },
        ];
        let items = rank_items("github", &tabs, &[], crate::plugin::PALETTE_COMMANDS, &[]);
        assert_eq!(items[0].title, "GitHub");
        assert!(!items.iter().any(|item| item.title == "GitLab"));
        let actions = rank_items("new", &[], &[], crate::plugin::PALETTE_COMMANDS, &[]);
        assert!(actions.iter().any(|item| item.title == "New tab"));
    }

    #[test]
    fn fuzzy_matching_accepts_subsequences() {
        assert!(fuzzy_score("rld", "Reload").is_some());
        assert!(fuzzy_score("zz", "Reload").is_none());
    }

    #[test]
    fn plugin_enable_and_disable_queries_target_the_matching_plugin() {
        let row = PluginRow {
            descriptor: crate::plugin::PluginDescriptor {
                id: "example.notes".into(),
                name: "Notes Panel".into(),
                version: "1.0.0".into(),
                kind: crate::plugin::PluginKind::Panel,
                description: "A private notes panel".into(),
                permissions: Vec::new(),
                enabled_by_default: false,
                can_toggle: true,
            },
            enabled: false,
            runtime: crate::plugin::RuntimeStatus::Pending,
        };
        let enabled = rank_items(
            "enable plugin notes",
            &[],
            &[],
            &[],
            std::slice::from_ref(&row),
        );
        assert!(enabled.iter().any(|item| {
            item.title == "Enable Notes Panel"
                && item.target
                    == (PaletteTarget::PluginToggle {
                        id: "example.notes".into(),
                        enabled: true,
                    })
        }));

        let mut row = row;
        row.enabled = true;
        let disabled = rank_items("disable plugin notes", &[], &[], &[], &[row]);
        assert!(disabled.iter().any(|item| {
            item.title == "Disable Notes Panel"
                && item.target
                    == (PaletteTarget::PluginToggle {
                        id: "example.notes".into(),
                        enabled: false,
                    })
        }));
    }
}
