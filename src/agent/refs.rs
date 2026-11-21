use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    pub last_url: String,
    next_index_by_kind: HashMap<char, usize>,
}

impl Default for RefRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RefRegistry {
    pub fn new() -> Self {
        Self {
            generation: 1,
            current_elements: HashMap::new(),
            previous_elements: HashMap::new(),
            ref_by_fingerprint: HashMap::new(),
            last_url: String::new(),
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
        }
        self.last_url = clean_url.clone();

        self.previous_elements = std::mem::take(&mut self.current_elements);
        let mut new_elements = Vec::new();
        let mut current_map = HashMap::new();

        for raw_el in raw.elements {
            let kind_char = raw_el.kind.chars().next().unwrap_or('x');
            let fingerprint = format!("{}:{}:{}", kind_char, raw_el.name, raw_el.path);

            let ref_id = if let Some(existing_id) = self.ref_by_fingerprint.get(&fingerprint) {
                existing_id.clone()
            } else {
                let idx = self.next_index_by_kind.entry(kind_char).or_insert(1);
                let id = format!("{kind_char}{idx}");
                *idx = idx.saturating_add(1);
                self.ref_by_fingerprint.insert(fingerprint.clone(), id.clone());
                id
            };

            let bounds = if let Some(b) = raw_el.box_bounds {
                let x = b.get(0).copied().unwrap_or(raw_el.x);
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

        // Calculate diff against previous elements
        let mut diff = SnapshotDiff::default();
        for (id, cur) in &current_map {
            if let Some(prev) = self.previous_elements.get(id) {
                if prev.name != cur.name || prev.flags != cur.flags {
                    let flags_str = if cur.flags.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", cur.flags.join(" "))
                    };
                    diff.changed.push(format!("{id} \"{}\"{flags_str}", cur.name));
                }
            } else {
                let flags_str = if cur.flags.is_empty() {
                    String::new()
                } else {
                    format!(" {}", cur.flags.join(" "))
                };
                diff.added.push(format!("{id} \"{}\"{flags_str}", cur.name));
            }
        }

        for (id, _) in &self.previous_elements {
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

    pub fn resolve_ref(&self, ref_id: &str) -> Option<&ElementRef> {
        self.current_elements.get(ref_id)
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
        assert_eq!(els1.get(0).map(|e| e.id.as_str()), Some("i1"));
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
        assert_eq!(els2.get(0).map(|e| e.id.as_str()), Some("i1"));
        assert_eq!(els2.get(1).map(|e| e.id.as_str()), Some("i2"));
        assert_eq!(els2.get(2).map(|e| e.id.as_str()), Some("b1"));

        // Diff shows added i2
        assert_eq!(diff.added.len(), 1);
        assert!(diff.added.get(0).unwrap().contains("i2"));
        assert!(diff.removed.is_empty());
    }
}
