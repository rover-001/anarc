use super::super::*;
use super::seed_pinned_destination;

impl Lumen {
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
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        {
            let is_already_active = space.active.as_deref() == Some(tab_id);
            if is_already_active {
                if let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    if (tab.pinned || tab.favorite)
                        && let Some(pinned_url) = tab.pinned_url.clone()
                    {
                        if tab.url != pinned_url {
                            tab.url = pinned_url.clone();
                            if let Some(pinned_title) = tab.pinned_title.clone() {
                                tab.title = pinned_title;
                            }
                            self.engine.navigate(tab_id, &pinned_url);
                            self.save_state();
                        }
                    }
                }
            } else {
                if let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                    // Adopt the destination before the browser loads it, so a
                    // later close cannot mistake a navigation for the pin.
                    seed_pinned_destination(tab);
                }
                space.active = Some(tab_id.to_owned());
                let _ = space;
                self.emit_plugin_text("tab.activated", tab_id.to_owned());
            }
        }
    }

    pub(crate) fn reset_tab_to_pinned_url(&mut self, tab_id: &str) {
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
        {
            if let Some(pinned_url) = tab.pinned_url.clone() {
                tab.url = pinned_url.clone();
                if let Some(pinned_title) = tab.pinned_title.clone() {
                    tab.title = pinned_title;
                }
                self.engine.navigate(tab_id, &pinned_url);
                self.save_state();
            }
        }
    }

    /// Favorites take precedence over pinning and folders so each tab lives in
    /// exactly one sidebar section.
    pub(in crate::app) fn favorite_tab(&mut self, tab_id: &str, favorite: bool) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
        {
            tab.favorite = favorite;
            if favorite {
                tab.pinned = false;
                tab.folder = None;
                if tab.pinned_url.is_none() {
                    tab.pinned_url = Some(tab.url.clone());
                }
                if tab.pinned_title.is_none() && !tab.title.is_empty() {
                    tab.pinned_title = Some(tab.title.clone());
                }
            } else if !tab.pinned {
                tab.pinned_url = None;
                tab.pinned_title = None;
            }
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }

    /// Pinned tabs render directly below the favorites grid.
    pub(in crate::app) fn pin_tab(&mut self, tab_id: &str, pinned: bool) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
        {
            tab.pinned = pinned;
            if pinned {
                tab.favorite = false;
                tab.folder = None;
                if tab.pinned_url.is_none() {
                    tab.pinned_url = Some(tab.url.clone());
                }
                if tab.pinned_title.is_none() && !tab.title.is_empty() {
                    tab.pinned_title = Some(tab.title.clone());
                }
            } else if !tab.favorite {
                tab.pinned_url = None;
                tab.pinned_title = None;
            }
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }

    /// Dragging a tab out of a favorite/pinned/folder section and dropping it
    /// on the regular tab list returns it to the plain section.
    pub(in crate::app) fn clear_tab_section(&mut self, tab_id: &str) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
            && (tab.favorite || tab.pinned || tab.folder.is_some())
        {
            tab.favorite = false;
            tab.pinned = false;
            tab.pinned_url = None;
            tab.pinned_title = None;
            tab.folder = None;
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }
    pub(in crate::app) fn reorder_tab_relative(
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
                    .map(|tab| (tab.favorite, tab.pinned, tab.folder.clone()))
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
        let target_section = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == target_id)
            .map(|tab| (tab.favorite, tab.pinned, tab.folder.clone()));
        let Some((favorite, pinned, folder)) = target_section else {
            return;
        };
        if let Some(folder_id) = folder {
            self.move_tab_to_folder(tab_id, &folder_id);
        } else if favorite {
            self.favorite_tab(tab_id, true);
        } else if pinned {
            self.pin_tab(tab_id, true);
        } else {
            self.clear_tab_section(tab_id);
        }
        self.reorder_tab_relative(tab_id, target_id, after);
    }
    pub(crate) fn add_tab(&mut self) {
        self.close_peek();
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
        let active_id = self.focused_tab_id().to_owned();
        if active_id.is_empty() {
            self.add_tab();
            return;
        }
        let is_kept_page = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == active_id)
            .is_some_and(|tab| tab.favorite || tab.pinned || tab.folder.is_some());
        if is_kept_page {
            self.unload_kept_tab(&active_id);
        } else if !self.close_tab_by_id(&active_id) {
            self.add_tab();
        }
    }
    pub(in crate::app) fn close_tab_by_id(&mut self, tab_id: &str) -> bool {
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
        let split_other = space.split.as_ref().and_then(|pair| {
            if pair.first == tab_id {
                Some(pair.second.clone())
            } else if pair.second == tab_id {
                Some(pair.first.clone())
            } else {
                None
            }
        });
        if split_other.is_some() {
            space.split = None;
            self.focused_pane = 0;
        }
        self.engine.close_browser(tab_id);
        let closed = space.tabs.remove(index);
        self.recently_closed_tabs.push((space.id.clone(), closed));
        if space.tabs.is_empty() {
            let id = next_tab_id(space);
            space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
            space.active = Some(id);
        } else if let Some(other) = split_other {
            space.active = Some(other);
        } else if was_active {
            // Kept tabs remain in the sidebar even when they have never been
            // opened. Closing a regular tab should not unexpectedly activate
            // one of those pages.
            let next_regular = (index..space.tabs.len())
                .chain((0..index.min(space.tabs.len())).rev())
                .find_map(|candidate| {
                    let tab = &space.tabs[candidate];
                    (!tab.favorite && !tab.pinned && tab.folder.is_none()).then(|| tab.id.clone())
                });
            if let Some(next_regular) = next_regular {
                space.active = Some(next_regular);
            } else {
                let id = next_tab_id(space);
                space.tabs.push(Tab::new(&id, "New tab", "about:blank"));
                space.active = Some(id);
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
        self.save_state();
        true
    }
    pub(crate) fn close_section_tab(&mut self, tab_id: &str) {
        let is_kept_page = self
            .active_space()
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_some_and(|tab| tab.favorite || tab.pinned || tab.folder.is_some());
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
        if folder_id.is_none() && !space.tabs[index].favorite && !space.tabs[index].pinned {
            return;
        }

        let was_active = space.active.as_deref() == Some(tab_id);
        let split_other = space.split.as_ref().and_then(|pair| {
            if pair.first == tab_id {
                Some(pair.second.clone())
            } else if pair.second == tab_id {
                Some(pair.first.clone())
            } else {
                None
            }
        });
        if split_other.is_some() {
            space.split = None;
            self.focused_pane = 0;
        }
        self.engine.close_browser(tab_id);
        space.tabs[index].state = TabState::Discarded;
        if space.tabs[index].pinned || space.tabs[index].favorite {
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
            let next_active = split_other
                .filter(|other| other != tab_id)
                .filter(|other| {
                    space
                        .tabs
                        .iter()
                        .any(|tab| tab.id == *other && tab.state != TabState::Discarded)
                })
                .or_else(|| {
                    space
                        .tabs
                        .iter()
                        .find(|tab| tab.id != tab_id && tab.state != TabState::Discarded)
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
        self.active_space_mut().split = Some(SplitPair {
            first: first.to_owned(),
            second: second.to_owned(),
        });
        self.focused_pane = 0;
        self.split_ratio = 0.5;
        self.save_state();
        true
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
