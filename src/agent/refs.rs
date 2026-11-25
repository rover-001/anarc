use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Ref resolution outcomes are recorded as a boolean only: no ref label or
// element content reaches telemetry. The counter is infallible and best-effort.
fn note_resolve(success: bool) {
    super::telemetry::telemetry()
        .counters
        .note_ref_resolve(success);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawElement {
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub flags: Vec<String>,
    pub x: i32,
    pub y: i32,
    #[serde(default)]
    pub box_bounds: Option<[i32; 4]>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub is_banner: bool,
    #[serde(default)]
    pub is_dialog: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawIndex {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub scroll_remaining: i32,
    pub elements: Vec<RawElement>,
}

#[derive(Clone, Debug)]
pub struct ElementRef {
    pub id: String,
    pub kind: char,
    pub name: String,
    pub flags: Vec<String>,
    pub center: (i32, i32),
    pub bounds: (i32, i32, i32, i32),
    pub fingerprint: String,
    pub is_banner: bool,
    pub is_dialog: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SnapshotDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

impl SnapshotDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    pub fn format_diff(&self) -> String {
        let mut parts = Vec::new();
        for a in &self.added {
            parts.push(format!("+{a}"));
        }
        for r in &self.removed {
            parts.push(format!("-{r}"));
        }
        for c in &self.changed {
            parts.push(format!("~{c}"));
        }
        parts.join("  ")
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotHeader {
    pub generation: u64,
    pub display_url: String,
    pub title: String,
    pub scroll_remaining: i32,
}

#[derive(Clone, Debug)]
pub struct RefRegistry {
    pub generation: u64,
    pub current_elements: HashMap<String, ElementRef>,
    pub previous_elements: HashMap<String, ElementRef>,
    pub ref_by_fingerprint: HashMap<String, String>,
    pub ref_by_path: HashMap<String, String>,
    pub last_url: String,
    /// Cursor for `see +` pagination into the last full index.
    pub see_plus_offset: usize,
    next_index_by_kind: HashMap<char, usize>,
}

impl Default for RefRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ElementRef {
    /// Fingerprint layout is `kind:name:path`; `splitn` keeps colons inside paths intact.
    pub fn dom_path(&self) -> &str {
        self.fingerprint.splitn(3, ':').nth(2).unwrap_or("")
    }
}

impl RefRegistry {
    pub fn new() -> Self {
        Self {
            generation: 1,
            current_elements: HashMap::new(),
            previous_elements: HashMap::new(),
            ref_by_fingerprint: HashMap::new(),
            ref_by_path: HashMap::new(),
            last_url: String::new(),
            see_plus_offset: 0,
            next_index_by_kind: HashMap::new(),
        }
    }

    pub fn update_from_raw(
        &mut self,
        raw: RawIndex,
    ) -> (SnapshotHeader, Vec<ElementRef>, SnapshotDiff, bool) {
        let mut generation_changed = false;
        let clean_url = strip_url_scheme(&raw.url);

        if !self.last_url.is_empty() && self.last_url != clean_url {
            self.generation = self.generation.saturating_add(1);
            generation_changed = true;
            self.next_index_by_kind.clear();
            self.ref_by_fingerprint.clear();
            self.ref_by_path.clear();
            self.see_plus_offset = 0;
        }
        self.last_url = clean_url.clone();

        self.previous_elements = std::mem::take(&mut self.current_elements);
        let mut new_elements = Vec::new();
        let mut current_map = HashMap::new();

        for raw_el in raw.elements {
            let kind_char = raw_el.kind.chars().next().unwrap_or('x');
            let fingerprint = format!("{}:{}:{}", kind_char, raw_el.name, raw_el.path);

            let path_key = format!("{kind_char}:{}", raw_el.path);
            let ref_id = if let Some(existing_id) = self
                .ref_by_fingerprint
                .get(&fingerprint)
                .filter(|id| !current_map.contains_key(id.as_str()))
            {
                existing_id.clone()
            } else if kind_char == 'i'
                && !raw_el.path.is_empty()
                && let Some(existing_id) = self
                    .ref_by_path
                    .get(&path_key)
                    .filter(|id| !current_map.contains_key(id.as_str()))
            {
                self.ref_by_fingerprint
                    .insert(fingerprint.clone(), existing_id.clone());
                existing_id.clone()
            } else {
                let idx = self.next_index_by_kind.entry(kind_char).or_insert(1);
                let id = format!("{kind_char}{idx}");
                *idx = idx.saturating_add(1);
                self.ref_by_fingerprint.insert(fingerprint.clone(), id.clone());
                if kind_char == 'i' && !raw_el.path.is_empty() {
                    self.ref_by_path.insert(path_key, id.clone());
                }
                id
            };

            let bounds = if let Some(b) = raw_el.box_bounds {
                let x = b.first().copied().unwrap_or(raw_el.x);
                let y = b.get(1).copied().unwrap_or(raw_el.y);
                let w = b.get(2).copied().unwrap_or(0);
                let h = b.get(3).copied().unwrap_or(0);
                (x, y, w, h)
            } else {
                (raw_el.x, raw_el.y, 0, 0)
            };

            let el_ref = ElementRef {
                id: ref_id.clone(),
                kind: kind_char,
                name: raw_el.name,
                flags: raw_el.flags,
                center: (raw_el.x, raw_el.y),
                bounds,
                fingerprint,
                is_banner: raw_el.is_banner,
                is_dialog: raw_el.is_dialog,
            };

            current_map.insert(ref_id, el_ref.clone());
            new_elements.push(el_ref);
        }

        // Calculate diff against previous elements in DOM order, prioritizing
        // newly opened dialog/popover elements so modals aren't truncated.
        let mut diff = SnapshotDiff::default();
        let mut added_dialog = Vec::new();
        let mut added_normal = Vec::new();
        for cur in &new_elements {
            let flags_str = if cur.flags.is_empty() {
                String::new()
            } else {
                format!(" {}", cur.flags.join(" "))
            };
            if let Some(prev) = self.previous_elements.get(&cur.id) {
                if prev.name != cur.name || prev.flags != cur.flags {
                    diff.changed
                        .push(format!("{} \"{}\"{flags_str}", cur.id, cur.name));
                }
            } else {
                let entry = format!("{} \"{}\"{flags_str}", cur.id, cur.name);
                if cur.is_dialog {
                    added_dialog.push(entry);
                } else {
                    added_normal.push(entry);
                }
            }
        }
        diff.added.extend(added_dialog);
        diff.added.extend(added_normal);

        for id in self.previous_elements.keys() {
            if !current_map.contains_key(id) {
                diff.removed.push(id.clone());
            }
        }

        self.current_elements = current_map;

        let header = SnapshotHeader {
            generation: self.generation,
            display_url: clean_url,
            title: raw.title,
            scroll_remaining: raw.scroll_remaining,
        };

        (header, new_elements, diff, generation_changed)
    }

    pub fn resolve_ref_with_fallback(&self, ref_id: &str) -> Option<&ElementRef> {
        // 1. Exact match in current elements
        if let Some(el) = self.current_elements.get(ref_id) {
            note_resolve(true);
            return Some(el);
        }

        // 2. Fallback: match by prior element metadata across generation shift
        if let Some(prev) = self.previous_elements.get(ref_id) {
            // 2a. Match by persistent fingerprint
            if let Some(new_id) = self.ref_by_fingerprint.get(&prev.fingerprint)
                && let Some(cur) = self.current_elements.get(new_id) {
                    note_resolve(true);
                    return Some(cur);
                }
            // 2b. Match by kind and identical name
            let prev_name_lower = prev.name.to_lowercase();
            for cur in self.current_elements.values() {
                if cur.kind == prev.kind && cur.name.to_lowercase() == prev_name_lower {
                    note_resolve(true);
                    return Some(cur);
                }
            }
            // 2c. Substring match if name is distinctive
            if prev.name.len() >= 3 {
                let candidates: Vec<&ElementRef> = self.current_elements.values()
                    .filter(|cur| cur.kind == prev.kind && cur.name.to_lowercase().contains(&prev_name_lower))
                    .collect();
                if candidates.len() == 1 {
                    note_resolve(true);
                    return candidates.first().copied();
                }
            }
        }

        note_resolve(false);
        None
    }
}

pub fn strip_url_scheme(url: &str) -> String {
    let mut s = url.trim();
    if let Some(stripped) = s.strip_prefix("https://") {
        s = stripped;
    } else if let Some(stripped) = s.strip_prefix("http://") {
        s = stripped;
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_stability_across_snapshots() {
        let mut reg = RefRegistry::new();
        let raw1 = RawIndex {
            title: "Test Page".into(),
            url: "https://example.com/form".into(),
            scroll_remaining: 0,
            elements: vec![
                RawElement {
                    kind: "i".into(),
                    name: "Username".into(),
                    flags: vec!["req".into()],
                    x: 10,
                    y: 20,
                    box_bounds: Some([0, 0, 100, 30]),
                    path: "input#user".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "b".into(),
                    name: "Submit".into(),
                    flags: vec!["primary".into()],
                    x: 10,
                    y: 60,
                    box_bounds: Some([0, 40, 100, 30]),
                    path: "button#submit".into(),
                    is_banner: false,
                    is_dialog: false,
                },
            ],
        };

        let (header1, els1, _, _) = reg.update_from_raw(raw1);
        assert_eq!(header1.generation, 1);
        assert_eq!(els1.first().map(|e| e.id.as_str()), Some("i1"));
        assert_eq!(els1.get(1).map(|e| e.id.as_str()), Some("b1"));

        // Second snapshot, same elements plus a new one
        let raw2 = RawIndex {
            title: "Test Page".into(),
            url: "https://example.com/form".into(),
            scroll_remaining: 0,
            elements: vec![
                RawElement {
                    kind: "i".into(),
                    name: "Username".into(),
                    flags: vec!["req".into()],
                    x: 10,
                    y: 20,
                    box_bounds: Some([0, 0, 100, 30]),
                    path: "input#user".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "i".into(),
                    name: "Password".into(),
                    flags: vec!["secret".into()],
                    x: 10,
                    y: 40,
                    box_bounds: Some([0, 30, 100, 30]),
                    path: "input#pass".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "b".into(),
                    name: "Submit".into(),
                    flags: vec!["primary".into()],
                    x: 10,
                    y: 60,
                    box_bounds: Some([0, 60, 100, 30]),
                    path: "button#submit".into(),
                    is_banner: false,
                    is_dialog: false,
                },
            ],
        };

        let (_, els2, diff, _) = reg.update_from_raw(raw2);
        // Existing elements kept their ref ids:
        assert_eq!(els2.first().map(|e| e.id.as_str()), Some("i1"));
        assert_eq!(els2.get(1).map(|e| e.id.as_str()), Some("i2"));
        assert_eq!(els2.get(2).map(|e| e.id.as_str()), Some("b1"));

        // Diff shows added i2
        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.first().is_some_and(|line| line.contains("i2")));
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn test_dom_path_keeps_colons_in_paths() {
        let el = ElementRef {
            id: "x13".into(),
            kind: 'x',
            name: "bubble".into(),
            flags: Vec::new(),
            center: (1, 2),
            bounds: (0, 0, 1, 1),
            fingerprint: "x:bubble:div.msg>span[data-id='a:b']".into(),
            is_banner: false,
            is_dialog: false,
        };
        assert_eq!(el.dom_path(), "div.msg>span[data-id='a:b']");
    }

    #[test]
    fn test_see_plus_cursor_resets_on_navigation() {
        let mut reg = RefRegistry::new();
        reg.see_plus_offset = 42;
        reg.update_from_raw(RawIndex {
            title: "Same".into(),
            url: "https://example.com/a".into(),
            scroll_remaining: 0,
            elements: vec![],
        });
        assert_eq!(reg.see_plus_offset, 42);
        reg.update_from_raw(RawIndex {
            title: "New".into(),
            url: "https://example.com/b".into(),
            scroll_remaining: 0,
            elements: vec![],
        });
        assert_eq!(reg.see_plus_offset, 0);
    }

    #[test]
    fn test_dialog_elements_prioritized_in_diff() {
        let mut reg = RefRegistry::new();
        reg.update_from_raw(RawIndex {
            title: "App".into(),
            url: "https://example.com".into(),
            scroll_remaining: 0,
            elements: vec![],
        });
        let (_, _, diff, _) = reg.update_from_raw(RawIndex {
            title: "App".into(),
            url: "https://example.com".into(),
            scroll_remaining: 0,
            elements: vec![
                RawElement {
                    kind: "b".into(),
                    name: "Background".into(),
                    flags: vec![],
                    x: 10,
                    y: 10,
                    box_bounds: None,
                    path: "button.bg".into(),
                    is_banner: false,
                    is_dialog: false,
                },
                RawElement {
                    kind: "b".into(),
                    name: "Add a new color".into(),
                    flags: vec![],
                    x: 50,
                    y: 50,
                    box_bounds: None,
                    path: "div[role=dialog]>button.add".into(),
                    is_banner: false,
                    is_dialog: true,
                },
            ],
        });
        assert_eq!(diff.added.len(), 2);
        assert!(diff.added[0].contains("Add a new color"), "{:?}", diff.added);
        assert!(diff.added[1].contains("Background"), "{:?}", diff.added);
    }

    #[test]
    fn test_input_ref_stays_stable_when_value_name_changes() {
        let mut reg = RefRegistry::new();
        reg.update_from_raw(RawIndex {
            title: "Sheets".into(),
            url: "https://docs.google.com/spreadsheets/d/1".into(),
            scroll_remaining: 0,
            elements: vec![RawElement {
                kind: "i".into(),
                name: "A1".into(),
                flags: vec![],
                x: 40,
                y: 80,
                box_bounds: None,
                path: "input#t-name-box".into(),
                is_banner: false,
                is_dialog: false,
            }],
        });
        let (_, els2, _, _) = reg.update_from_raw(RawIndex {
            title: "Sheets".into(),
            url: "https://docs.google.com/spreadsheets/d/1".into(),
            scroll_remaining: 0,
            elements: vec![RawElement {
                kind: "i".into(),
                name: "A2".into(),
                flags: vec![],
                x: 40,
                y: 80,
                box_bounds: None,
                path: "input#t-name-box".into(),
                is_banner: false,
                is_dialog: false,
            }],
        });
        assert_eq!(els2[0].id, "i1");
    }
}
