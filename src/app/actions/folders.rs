use super::super::*;

impl Lumen {
    pub(in crate::app) fn move_tab_to_folder(&mut self, tab_id: &str, folder_id: &str) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        {
            if !space.folders.iter().any(|folder| folder.id == folder_id) {
                return;
            }
            if let Some(folder) = space
                .folders
                .iter_mut()
                .find(|folder| folder.id == folder_id)
            {
                folder.expanded = true;
            }
            if let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id) {
                tab.favorite = false;
                tab.pinned = false;
                tab.pinned_url = None;
                tab.pinned_title = None;
                tab.folder = Some(folder_id.to_owned());
                changed = true;
            }
        }
        if changed {
            self.save_state();
        }
    }
    pub(in crate::app) fn remove_tab_from_folder(&mut self, tab_id: &str) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
        {
            tab.folder = None;
            changed = true;
        }
        if changed {
            self.save_state();
        }
    }
    pub(in crate::app) fn create_folder(
        &mut self,
        name: &str,
        target_tab: Option<&str>,
    ) -> Option<String> {
        let space = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)?;
        let mut index = space.folders.len() + 1;
        let mut id = format!("folder-{index}");
        while space.folders.iter().any(|folder| folder.id == id) {
            index += 1;
            id = format!("folder-{index}");
        }
        space.folders.push(Folder::new(id.clone(), name));
        if let Some(tab_id) = target_tab
            && let Some(tab) = space.tabs.iter_mut().find(|tab| tab.id == tab_id)
        {
            tab.favorite = false;
            tab.pinned = false;
            tab.pinned_url = None;
            tab.pinned_title = None;
            tab.folder = Some(id.clone());
        }
        self.save_state();
        Some(id)
    }
    pub(in crate::app) fn rename_folder(&mut self, folder_id: &str, name: &str) {
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
            && let Some(folder) = space
                .folders
                .iter_mut()
                .find(|folder| folder.id == folder_id)
        {
            folder.name = name.to_owned();
            self.save_state();
        }
    }
    pub(in crate::app) fn delete_folder(&mut self, folder_id: &str) {
        let mut changed = false;
        if let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == self.active_space)
        {
            let before = space.folders.len();
            space.folders.retain(|folder| folder.id != folder_id);
            for tab in &mut space.tabs {
                if tab.folder.as_deref() == Some(folder_id) {
                    tab.folder = None;
                }
            }
            changed = before != space.folders.len();
        }
        if changed {
            self.save_state();
        }
    }
    pub(in crate::app) fn open_create_folder_modal(
        &mut self,
        target_tab: Option<String>,
        anchor: gpui_kit::Point<gpui_kit::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.space_modal = None;
        self.tab_context_menu = None;
        self.folder_context_menu = None;
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("New folder"));
        name_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        let sub = cx.subscribe(&name_input, |this: &mut Lumen, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_folder_modal(cx);
                cx.notify();
            }
        });
        self.folder_modal = Some(FolderModal {
            mode: FolderModalMode::Create { target_tab },
            name_input,
            _name_sub: sub,
            anchor_x: f32::from(anchor.x),
            anchor_y: f32::from(anchor.y),
        });
    }
    pub(in crate::app) fn open_rename_folder_modal(
        &mut self,
        folder_id: &str,
        anchor: gpui_kit::Point<gpui_kit::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .active_space()
            .folders
            .iter()
            .find(|folder| folder.id == folder_id)
            .map(|folder| folder.name.clone())
        else {
            return;
        };
        self.space_modal = None;
        self.tab_context_menu = None;
        self.folder_context_menu = None;
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Folder name"));
        name_input.update(cx, |input, cx| {
            input.set_value(&name, window, cx);
            input.focus(window, cx);
        });
        let sub = cx.subscribe(&name_input, |this: &mut Lumen, _, event, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.commit_folder_modal(cx);
                cx.notify();
            }
        });
        self.folder_modal = Some(FolderModal {
            mode: FolderModalMode::Rename {
                folder_id: folder_id.to_owned(),
            },
            name_input,
            _name_sub: sub,
            anchor_x: f32::from(anchor.x),
            anchor_y: f32::from(anchor.y),
        });
    }
    pub(in crate::app) fn commit_folder_modal(&mut self, cx: &Context<Self>) {
        let Some(modal) = self.folder_modal.take() else {
            return;
        };
        let input_val = modal.name_input.read(cx).value().to_string();
        let name = if input_val.trim().is_empty() {
            match &modal.mode {
                FolderModalMode::Create { .. } => "New folder".to_string(),
                FolderModalMode::Rename { .. } => "Untitled folder".to_string(),
            }
        } else {
            input_val.trim().to_string()
        };
        match modal.mode {
            FolderModalMode::Create { target_tab } => {
                self.create_folder(&name, target_tab.as_deref());
            }
            FolderModalMode::Rename { folder_id } => {
                self.rename_folder(&folder_id, &name);
            }
        }
    }
}
