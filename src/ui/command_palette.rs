use crate::history::HistoryEntry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteTarget {
    Tab { space_id: String, tab_id: String },
    History(String),
    Action(PaletteAction),
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

const ACTIONS: &[(&str, &str, PaletteAction)] = &[
    ("New tab", "Action", PaletteAction::NewTab),
    ("Close tab", "Action", PaletteAction::CloseTab),
    (
        "Reopen closed tab",
        "Action",
        PaletteAction::ReopenClosedTab,
    ),
    ("Reload", "Action", PaletteAction::Reload),
    ("Find in page", "Action", PaletteAction::FindInPage),
    ("Zoom in", "Action", PaletteAction::ZoomIn),
    ("Zoom out", "Action", PaletteAction::ZoomOut),
    ("Reset zoom", "Action", PaletteAction::ZoomReset),
    ("New space", "Action", PaletteAction::NewSpace),
];

pub fn rank_items(
    query: &str,
    tabs: &[PaletteTab<'_>],
    history: &[HistoryEntry],
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
    for (name, detail, action) in ACTIONS {
        if let Some(score) = fuzzy_score(query, name) {
            items.push(PaletteItem {
                title: (*name).to_string(),
                detail: (*detail).to_string(),
                target: PaletteTarget::Action(*action),
                score: score + 30,
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
        let items = rank_items("github", &tabs, &[]);
        assert_eq!(items[0].title, "GitHub");
        assert!(!items.iter().any(|item| item.title == "GitLab"));
        let actions = rank_items("new", &[], &[]);
        assert!(actions.iter().any(|item| item.title == "New tab"));
    }

    #[test]
    fn fuzzy_matching_accepts_subsequences() {
        assert!(fuzzy_score("rld", "Reload").is_some());
        assert!(fuzzy_score("zz", "Reload").is_none());
    }
}
