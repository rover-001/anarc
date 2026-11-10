use super::super::*;

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
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        {
            space.active = Some(tab_id.to_owned());
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
    pub(in crate::app) fn add_tab(&mut self) {
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
        self.save_state();
    }
    pub(in crate::app) fn close_active_tab(&mut self) {
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
            .find(|space| space.id == self.active_space)
        else {
            return false;
        };
        let Some(index) = space.tabs.iter().position(|tab| tab.id == tab_id) else {
            return false;
        };
        let was_active = space.active.as_deref() == Some(tab_id);
        let closing_space_id = space.id.clone();
        let closing_snapshot = space.tabs[index].clone();
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
        self.closing_tab_serial = self.closing_tab_serial.wrapping_add(1);
        self.closing_tab = Some((
            closing_space_id,
            closing_snapshot,
            index,
            Instant::now(),
            self.closing_tab_serial,
        ));
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
    pub(in crate::app) fn reopen_closed_tab(&mut self) {
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
    pub(in crate::app) fn open_popup_tab(&mut self, space_id: &str, url: String) {
        {
            let Some(space) = self.spaces.iter_mut().find(|space| space.id == space_id) else {
                return;
            };
            let id = next_tab_id(space);
            let mut tab = Tab::new(&id, "New tab", url);
            tab.state = TabState::Live;
            space.tabs.push(tab);
            space.active = Some(id);
        }
        self.activate_space(space_id);
        self.save_state();
    }
}
