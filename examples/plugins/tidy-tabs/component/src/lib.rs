use lumen_tidy_tabs::{PLUGIN, sorted_ids};

wit_bindgen::generate!({
    path: "../../../../wit",
    world: "lumen-plugin",
});

struct TidyTabs;

impl exports::lumen::plugin::guest::Guest for TidyTabs {
    fn get_manifest() -> exports::lumen::plugin::guest::Manifest {
        exports::lumen::plugin::guest::Manifest {
            id: PLUGIN.id.to_owned(),
            version: PLUGIN.version.to_owned(),
            api: PLUGIN.api.to_owned(),
            requested_permissions: vec!["tabs=read-write".to_owned()],
        }
    }

    fn apply(_config: String) -> Result<(), String> {
        Ok(())
    }

    fn on_event(_event: exports::lumen::plugin::guest::Event) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn run_command(command_id: String) -> Result<(), String> {
        if command_id != "tidy-tabs.sort" {
            return Err(format!("unknown command: {command_id}"));
        }
        let tabs = lumen::plugin::host::list_tabs()?;
        let sortable = tabs
            .iter()
            .map(|tab| (tab.id.clone(), tab.title.clone(), tab.pinned))
            .collect::<Vec<_>>();
        for (index, tab_id) in sorted_ids(&sortable).iter().enumerate() {
            lumen::plugin::host::move_tab(tab_id, index as u32)?;
        }
        Ok(())
    }
}

export!(TidyTabs);
