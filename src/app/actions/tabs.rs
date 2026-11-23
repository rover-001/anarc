use super::super::*;
use super::seed_pinned_destination;

/// Compact a split after one of its panes closes. Returns the new split (or
/// `None` when fewer than two panes remain), the adjusted focus index and the
/// pane tab that should become active.
fn split_after_close(
    split: &Option<SplitPair>,
    focused_pane: usize,
    tab_id: &str,
) -> Option<(Option<SplitPair>, usize, Option<String>)> {
    let pair = split.as_ref()?;
    let closed_index = pair.panes().iter().position(|id| *id == tab_id)?;
    let remaining_ids = pair.panes_without(tab_id);
    let last_focused = remaining_ids.len().saturating_sub(1);
    let focused = focused_pane.min(pair.pane_count() - 1);
    let next_focused = if focused > closed_index {
        focused - 1
    } else {
        focused.min(last_focused)
    }
    .min(last_focused);
    let activated = remaining_ids.get(next_focused).cloned();
    Some((
        SplitPair::from_panes(&remaining_ids),
        next_focused,
        activated,
    ))
}

impl Anarc {
    pub(crate) fn open_tab_context_menu(
        &mut self,
        tab_id: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) {
        self.tab_context_menu = Some(super::super::TabContextMenu {
            tab_id,
            showing_split_targets: false,
            x,
            y,
            width,
            height,
        });
    }

    pub(crate) fn activate_tab_in_active_space(&mut self, tab_id: &str) {
        self.close_peek();
        self.close_app_page();
        self.plugins_menu_open = false;
        self.tab_context_menu = None;
        self.space_context_menu = None;
        self.folder_context_menu = None;
        self.web_context_menu = None;
        self.theme_editor = None;
        self.show_suggestions = false;
        self.selected_suggestion = None;
        let mut target_nav = None;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        {
            let was_already_active = space.active.as_deref() == Some(tab_id);
            if let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                seed_pinned_destination(tab);
                if tab.is_pinned()
                    && let Some(pinned_url) = tab.pinned_url.clone()
                    && tab.url != pinned_url
                {
                    tab.url = pinned_url.clone();
                    if let Some(pinned_title) = tab.pinned_title.clone() {
                        tab.title = pinned_title;
                    }
                    target_nav = Some(pinned_url);
                }
            }
            if !was_already_active {
                space.active = Some(tab_id.to_owned());
                let _ = space;
                self.emit_plugin_text("tab.activated", tab_id.to_owned());
            }
        }
        if let Some(url) = target_nav {
            self.engine.navigate(tab_id, &url);
            self.save_state();
        }
    }

    pub(crate) fn reset_tab_to_pinned_url(&mut self, tab_id: &str) {
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
            && let Some(pinned_url) = tab.pinned_url.clone()
        {
            tab.url = pinned_url.clone();
            if let Some(pinned_title) = tab.pinned_title.clone() {
                tab.title = pinned_title;
            }
            self.engine.navigate(tab_id, &pinned_url);
            self.save_state();
        }
    }

    /// Favorites and pins are the same kept-tab section. `favorite_tab` is kept
    /// as an alias of `pin_tab` for the agent/CLI/MCP surface.
    pub(crate) fn favorite_tab(&mut self, tab_id: &str, favorite: bool) {
        self.set_tab_kept(tab_id, favorite);
    }

    /// Pins (or unpins) a tab in the single kept-tab section at the top of the
    /// sidebar. Both `pinned` and `favorite` are written so the alias never
    /// diverges in saved state.
    pub(crate) fn pin_tab(&mut self, tab_id: &str, pinned: bool) {
        self.set_tab_kept(tab_id, pinned);
    }

    fn set_tab_kept(&mut self, tab_id: &str, kept: bool) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
        {
            tab.set_pinned(kept);
            if kept {
                tab.folder = None;
                if tab.pinned_url.is_none() {
                    tab.pinned_url = Some(tab.url.clone());
                }
                if tab.pinned_title.is_none() && !tab.title.is_empty() {
                    tab.pinned_title = Some(tab.title.clone());
                }
            } else {
                tab.pinned_url = None;
                tab.pinned_title = None;
                tab.kept_group = None;
            }
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }

    /// Drag targets decide the shape: dropping on a box body joins its group,
    /// dropping on the lower strip gives the tab its own full-width row.
    pub(crate) fn set_kept_group(&mut self, tab_id: &str, group: Option<String>) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
            && tab.kept_group != group
        {
            tab.kept_group = group;
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }

    /// Dragging a tab out of the kept/folder section and dropping it on the
    /// regular tab list returns it to the plain section.
    pub(crate) fn clear_tab_section(&mut self, tab_id: &str) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
            && (tab.is_pinned() || tab.folder.is_some())
        {
            tab.set_pinned(false);
            tab.pinned_url = None;
            tab.pinned_title = None;
            tab.folder = None;
            tab.kept_group = None;
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }
    pub(crate) fn reorder_tab_relative(
        &mut self,
        tab_id: &str,
        target_id: &str,
        after: bool,
    ) {
        if tab_id == target_id {
            return;
        }
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        {
            let section = |id: &str| {
                space
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id)
                    .map(|tab| (tab.is_pinned(), tab.folder.clone()))
            };
            if section(tab_id).is_none() || section(tab_id) != section(target_id) {
                return;
            }
            let Some(source_index) = space.tabs.iter().position(|tab| tab.id == tab_id) else {
                return;
            };
            let moving = space.tabs.remove(source_index);
            let Some(target_index) = space.tabs.iter().position(|tab| tab.id == target_id) else {
                space.tabs.insert(source_index, moving);
                return;
            };
            let destination = if after {
                target_index + 1
            } else {
                target_index
            };
            changed = destination != source_index;
            space.tabs.insert(destination, moving);
        }
        if changed {
            self.save_state();
        }
    }
    pub(in crate::app) fn move_tab_to_end(&mut self, tab_id: &str) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(index) = space.tabs.iter().position(|tab| tab.id == tab_id)
        {
            let tab = space.tabs.remove(index);
            space.tabs.push(tab);
            changed = index + 1 != space.tabs.len();
        }
        if changed {
            self.save_state();
        }
    }
    pub(crate) fn drop_tab_in_target_section(
        &mut self,
        tab_id: &str,
        target_id: &str,
        after: bool,
    ) {
        self.kept_drag_source_id = None;
        self.kept_drag_hovered_tab = None;
        self.kept_drag_hover_after = false;
        self.kept_section_drag_hovered = false;
        let target_section = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == target_id)
            .map(|tab| (tab.is_pinned(), tab.folder.clone(), tab.kept_group.clone()));
        let Some((kept, folder, group)) = target_section else {
            return;
        };
        if let Some(folder_id) = folder {
            self.move_tab_to_folder(tab_id, &folder_id);
        } else if kept {
            self.pin_tab(tab_id, true);
            // Body drop: join the target's box group (creating one if needed).
            // Lower-strip drop: keep the tab in its own full-width row.
            let joined_group = if after {
                None
            } else {
                Some(group.unwrap_or_else(|| target_id.to_owned()))
            };
            self.set_kept_group(tab_id, joined_group.clone());
            if let Some(group) = joined_group {
                self.set_kept_group(target_id, Some(group));
            }
        } else {
            self.clear_tab_section(tab_id);
        }
        self.reorder_tab_relative(tab_id, target_id, after);
    }

    /// Drop a kept tab onto the hover-expanded before/after slot beside `anchor`.
    ///
    /// A slot beside an ungrouped tab must create a two-tab group: using the
    /// anchor's id as the group key joins both tabs. Previously the slot only
    /// set the dragged tab's group to `None`, so the first group could never be
    /// formed (adding a third tab worked because existing groups carry an id).
    pub(crate) fn drop_kept_tab_in_slot(
        &mut self,
        tab_id: &str,
        anchor: &str,
        group: Option<String>,
        after: bool,
    ) {
        self.kept_drag_source_id = None;
        self.kept_drag_hovered_tab = None;
        self.kept_drag_hover_after = false;
        self.kept_section_drag_hovered = false;
        self.pin_tab(tab_id, true);
        let joined_group = group.or_else(|| Some(anchor.to_owned()));
        self.set_kept_group(tab_id, joined_group.clone());
        self.set_kept_group(anchor, joined_group);
        self.reorder_tab_relative(tab_id, anchor, after);
    }
    pub(crate) fn add_tab(&mut self) {
        self.close_peek();
        self.close_app_page();
        self.plugins_menu_open = false;
        self.tab_context_menu = None;
        self.space_context_menu = None;
        self.folder_context_menu = None;
        self.web_context_menu = None;
        self.theme_editor = None;
        self.show_suggestions = false;
        self.address_editing = false;
        self.reset_address_for_tab_change = true;
        let space = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            .expect("active space must exist");
        let id = next_tab_id(space);
        let tab = Tab::new(&id, "New tab", "about:blank");
        space.tabs.push(tab);
        space.active = Some(id.clone());
        self.added_tab = Some((self.active_space.clone(), id, Instant::now()));
        self.emit_plugin_text("tab.created", self.active_tab_id().to_owned());
        self.save_state();
    }
    pub(crate) fn close_active_tab(&mut self) {
        self.show_suggestions = false;
        self.address_editing = false;
        self.reset_address_for_tab_change = true;
        let active_id = self.focused_tab_id().to_owned();
        if active_id.is_empty() {
            return;
        }
        let is_kept = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == active_id)
            .is_some_and(|tab| tab.is_pinned());

        if is_kept {
            self.unload_kept_tab(&active_id);
        } else {
            self.close_tab_by_id(&active_id);
        }
    }
    pub(crate) fn close_tab_by_id(&mut self, tab_id: &str) -> bool {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.tabs.iter().any(|tab| tab.id == tab_id))
        else {
            return false;
        };
        let Some(index) = space.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let was_active = space.active.as_deref() == Some(tab_id);
        let closing_space_id = space.id.clone();
        let closing_snapshot = space.tabs[index].clone();
        let closed_tab_id = closing_snapshot.id.clone();
        let is_active_space = space.id == self.active_space;
        let split_fallback = split_after_close(&space.split, self.focused_pane, tab_id).map(
            |(remaining_split, next_focused, fallback)| {
                if is_active_space {
                    self.focused_pane = next_focused;
                }
                space.split = remaining_split;
                fallback
            },
        );
        self.engine.close_browser(tab_id);
        let closed = space.tabs.remove(index);
        self.recently_closed_tabs.push((space.id.clone(), closed));
        const MAX_RECENTLY_CLOSED: usize = 25;
        if self.recently_closed_tabs.len() > MAX_RECENTLY_CLOSED {
            let excess = self.recently_closed_tabs.len() - MAX_RECENTLY_CLOSED;
            self.recently_closed_tabs.drain(..excess);
        }
        let mut activated_new_tab = None;
        if space.tabs.is_empty() {
            let id = next_tab_id(space);
            space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
            space.active = Some(id.clone());
            activated_new_tab = Some(id);
        } else if let Some(act) = split_fallback.flatten() {
            space.active = Some(act.clone());
            activated_new_tab = Some(act);
        } else if was_active {
            // Closing a regular tab should never open a favorite or pinned page.
            // Kept tabs stay unloaded unless the user selects them, so keep a
            // regular tab active even when it is the last regular tab.
            let next_regular = (index..space.tabs.len())
                .chain((0..index.min(space.tabs.len())).rev())
                .find_map(|candidate| {
                    let tab = &space.tabs[candidate];
                    (!tab.is_pinned() && tab.folder.is_none()).then(|| tab.id.clone())
                });
            if let Some(next_regular) = next_regular {
                space.active = Some(next_regular.clone());
                activated_new_tab = Some(next_regular);
            } else {
                let id = next_tab_id(space);
                space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
                space.active = Some(id.clone());
                activated_new_tab = Some(id);
            }
        }
        let is_empty_tab = closing_snapshot.url == "about:blank"
            || closing_snapshot.url == "lumen://welcome"
            || closing_snapshot.url.trim().is_empty();
        if !is_empty_tab {
            self.closing_tab_serial = self.closing_tab_serial.wrapping_add(1);
            self.closing_tab = Some((
                closing_space_id,
                closing_snapshot,
                index,
                Instant::now(),
                self.closing_tab_serial,
            ));
        } else {
            self.closing_tab = None;
        }
        self.emit_plugin_text("tab.closed", closed_tab_id);
        if let Some(act) = activated_new_tab {
            self.activate_tab_in_active_space(&act);
        }
        self.save_state();
        true
    }
    pub(crate) fn close_section_tab(&mut self, tab_id: &str) {
        let is_kept_page = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_some_and(|tab| tab.is_pinned());
        if is_kept_page {
            self.unload_kept_tab(tab_id);
        } else {
            self.close_tab_by_id(tab_id);
        }
    }
    pub(in crate::app) fn unload_kept_tab(&mut self, tab_id: &str) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        let Some(index) = space.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        let folder_id = space.tabs[index].folder.clone();
        if folder_id.is_none() && !space.tabs[index].is_pinned() {
            return;
        }

        let was_active = space.active.as_deref() == Some(tab_id);
        let split_fallback = split_after_close(&space.split, self.focused_pane, tab_id).map(
            |(remaining_split, next_focused, fallback)| {
                self.focused_pane = next_focused;
                space.split = remaining_split;
                fallback
            },
        );
        self.engine.close_browser(tab_id);
        space.tabs[index].state = TabState::Discarded;
        if space.tabs[index].is_pinned() {
            seed_pinned_destination(&mut space.tabs[index]);
            if let Some(pinned_url) = space.tabs[index].pinned_url.clone() {
                space.tabs[index].url = pinned_url;
            }
            if let Some(pinned_title) = space.tabs[index].pinned_title.clone() {
                space.tabs[index].title = pinned_title;
            }
        }

        if let Some(folder_id) = folder_id {
            let another_folder_page_is_open = space.tabs.iter().any(|tab| {
                tab.id != tab_id
                    && tab.folder.as_deref() == Some(folder_id.as_str())
                    && tab.state != TabState::Discarded
            });
            if !another_folder_page_is_open
                && let Some(folder) = space
                    .folders
                    .iter_mut()
                    .find(|folder| folder.id == folder_id)
            {
                folder.expanded = false;
            }
        }

        if was_active {
            let next_active = split_fallback
                .flatten()
                .filter(|other| other != tab_id)
                // A remaining pane is the natural focus target even when its
                // tab has not been loaded yet, but never reactivate a pinned
                // page: that would reopen it on the next render.
                .filter(|other| {
                    space
                        .tabs
                        .iter()
                        .any(|tab| tab.id == *other && !tab.is_pinned())
                })
                // Unloading a pinned tab must never activate another pinned
                // page: doing so reopens it on the next render and cascades
                // into more tabs when Ctrl+W is pressed repeatedly. Prefer a
                // live regular tab, then an existing blank tab, then create
                // one new blank tab.
                .or_else(|| {
                    space
                        .tabs
                        .iter()
                        .find(|tab| {
                            tab.id != tab_id
                                && tab.state != TabState::Discarded
                                && !tab.is_pinned()
                                && tab.folder.is_none()
                        })
                        .map(|tab| tab.id.clone())
                })
                .or_else(|| {
                    space
                        .tabs
                        .iter()
                        .find(|tab| {
                            tab.id != tab_id
                                && tab.folder.is_none()
                                && (tab.url == "about:blank" || tab.url == "lumen://welcome")
                        })
                        .map(|tab| tab.id.clone())
                });
            space.active = next_active.or_else(|| {
                let id = next_tab_id(space);
                space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
                Some(id)
            });
        }
        self.save_state();
    }
    pub(in crate::app) fn split_tabs(&mut self, first: &str, second: &str) -> bool {
        if !crate::plugins::split_view_enabled() {
            return false;
        }
        if first == second
            || !self.active_space().tabs.iter().any(|tab| tab.id == first)
            || !self.active_space().tabs.iter().any(|tab| tab.id == second)
        {
            return false;
        }
        let space = self.active_space_mut();
        if let Some(mut pair) = space.split.clone() {
            if pair.is_quad() {
                return false;
            }
            if pair.panes().contains(&first) || pair.panes().contains(&second) {
                return false;
            }
            if pair.third.is_some() {
                // Three panes already: only the free quadrant can be filled.
                pair.fourth = Some(second.to_owned());
                space.split = Some(pair);
            } else {
                let extra = space
                    .tabs
                    .iter()
                    .map(|tab| tab.id.clone())
                    .find(|id| !pair.panes().contains(&id.as_str()) && id != second);
                let Some(extra) = extra else {
                    return false;
                };
                pair.third = Some(second.to_owned());
                pair.fourth = Some(extra);
                space.split = Some(pair);
            }
        } else {
            space.split = Some(SplitPair::new(first.to_owned(), second.to_owned()));
        }
        self.focused_pane = 0;
        self.split_ratio = 0.5;
        self.split_ratio_y = 0.5;
        self.save_state();
        true
    }
    /// Drop a sidebar tab onto a content pane. With no split this creates a
    /// two-pane split with the dropped tab in the target pane; an existing
    /// split grows to the 2x2 grid, and a full grid replaces that pane.
    pub(crate) fn drop_tab_into_pane(&mut self, tab_id: &str, pane_index: usize) {
        if !crate::plugins::split_view_enabled() {
            return;
        }
        let space = self.active_space();
        if !space.tabs.iter().any(|tab| {
            tab.id == tab_id && tab.url != "about:blank" && !tab.url.is_empty()
        }) {
            return;
        }
        let active = space.active.clone().unwrap_or_default();
        let Some(mut pair) = space.split.clone() else {
            if tab_id == active {
                return;
            }
            let new_pair = if pane_index == 0 {
                SplitPair::new(tab_id.to_owned(), active.clone())
            } else {
                SplitPair::new(active.clone(), tab_id.to_owned())
            };
            self.active_space_mut().split = Some(new_pair);
            self.focused_pane = pane_index.min(1);
            self.split_ratio = 0.5;
            self.split_ratio_y = 0.5;
            self.save_state();
            return;
        };
        if let Some(existing) = pair
            .panes()
            .iter()
            .position(|id| *id == tab_id)
        {
            self.set_focused_pane(existing);
            return;
        }
        if pair.is_quad() {
            match pane_index {
                0 => pair.first = tab_id.to_owned(),
                1 => pair.second = tab_id.to_owned(),
                2 => pair.third = Some(tab_id.to_owned()),
                _ => pair.fourth = Some(tab_id.to_owned()),
            }
            self.active_space_mut().split = Some(pair);
            self.set_focused_pane(pane_index);
            self.save_state();
            return;
        }
        if pair.third.is_some() {
            // Three-pane layout has one free quadrant (the bottom-right pane).
            pair.fourth = Some(tab_id.to_owned());
            self.active_space_mut().split = Some(pair);
            self.set_focused_pane(3);
            self.save_state();
            return;
        }
        // The free quadrant gets a fresh blank tab. Reusing an existing tab
        // here silently pulls in unrelated pages (for example a pinned inbox).
        let space = self.active_space_mut();
        let blank_id = next_tab_id(space);
        space.tabs.push(Tab::new(&blank_id, "New tab", "about:blank"));
        let (third, fourth) = if pane_index == 0 {
            (tab_id.to_owned(), blank_id)
        } else {
            (blank_id, tab_id.to_owned())
        };
        pair.third = Some(third);
        pair.fourth = Some(fourth);
        self.active_space_mut().split = Some(pair);
        self.focused_pane = pane_index.min(3);
        self.split_ratio_y = 0.5;
        self.save_state();
    }

    pub(in crate::app) fn choose_split_target(&mut self, target_id: &str) -> bool {
        let Some(menu) = self.tab_context_menu.as_ref() else {
            return false;
        };
        if !menu.showing_split_targets {
            return false;
        }
        let first_id = menu.tab_id.clone();
        let split = self.split_tabs(&first_id, target_id);
        if split {
            self.tab_context_menu = None;
        }
        split
    }
    pub(in crate::app) fn duplicate_tab(&mut self, tab_id: &str) {
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        else {
            return;
        };
        let Some(source) = space.tabs.iter().find(|tab| tab.id == tab_id).cloned() else {
            return;
        };
        let id = next_tab_id(space);
        let mut duplicate = source;
        duplicate.id = id.clone();
        duplicate.state = TabState::Discarded;
        space.tabs.push(duplicate);
        space.active = Some(id);
        self.tab_context_menu = None;
        self.save_state();
    }
    pub(crate) fn reopen_closed_tab(&mut self) {
        let Some((space_id, tab)) = self.recently_closed_tabs.pop() else {
            return;
        };
        if let Some(space) = self.spaces.iter_mut().find(|space| space.id == space_id) {
            let id = if space.tabs.iter().any(|existing| existing.id == tab.id) {
                next_tab_id(space)
            } else {
                tab.id.clone()
            };
            let mut tab = tab;
            tab.id = id.clone();
            space.tabs.push(tab);
            space.active = Some(id);
            let _ = space;
            self.activate_space(&space_id);
        }
        self.save_state();
    }
    /// Links that would normally open a new tab now surface as an Arc-style
    /// peek overlay. The preview can be promoted with its "open in tab" button.
    pub(in crate::app) fn open_popup_tab(&mut self, space_id: &str, url: String) {
        let url = match self
            .plugins
            .emit(
                "popup.before",
                crate::plugin::EventMode::Bail,
                crate::plugin::EventPayload::Text(url.clone()),
            )
            .into_iter()
            .next()
        {
            Some(crate::plugin::EventPayload::Flag(true)) => return,
            Some(crate::plugin::EventPayload::Text(redirected)) => redirected,
            _ => url,
        };
        if !self.spaces.iter().any(|space| space.id == space_id) {
            return;
        }
        if self.active_space != space_id {
            self.activate_space(space_id);
        }
        self.open_peek(space_id, url);
        self.save_state();
    }

    /// Shows a window CEF created for the page (sign-in and OAuth popups) in
    /// the peek overlay. The browser already exists and keeps its
    /// `window.opener`, so nothing is created here; the overlay just points at
    /// it. Closing the overlay, or the page calling `window.close()`, ends it.
    pub(in crate::app) fn show_popup_window(
        &mut self,
        space_id: &str,
        url: String,
        browser_id: String,
    ) {
        if !self.spaces.iter().any(|space| space.id == space_id) {
            self.engine.close_browser(&browser_id);
            return;
        }
        if self.active_space != space_id {
            self.activate_space(space_id);
        }
        if self.peek.as_ref().is_some_and(|peek| peek.id == browser_id) {
            return;
        }
        self.close_peek();
        self.emit_plugin_text("peek.opened", url.clone());
        self.peek = Some(crate::app::state::PeekState {
            id: browser_id,
            space_id: space_id.to_owned(),
            title: super::peek::peek_title_for(&url),
            url,
        });
        self.peek_focused = false;
        self.save_state();
    }
}
