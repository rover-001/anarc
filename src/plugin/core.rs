use super::{Manifest, Order, Plugin, PluginCtx, PluginError};

pub(crate) const PALETTE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand {
        id: "new-tab",
        title: "New tab",
        detail: "Action",
    },
    PaletteCommand {
        id: "close-tab",
        title: "Close tab",
        detail: "Action",
    },
    PaletteCommand {
        id: "reopen-closed-tab",
        title: "Reopen closed tab",
        detail: "Action",
    },
    PaletteCommand {
        id: "reload",
        title: "Reload",
        detail: "Action",
    },
    PaletteCommand {
        id: "find-in-page",
        title: "Find in page",
        detail: "Action",
    },
    PaletteCommand {
        id: "zoom-in",
        title: "Zoom in",
        detail: "Action",
    },
    PaletteCommand {
        id: "zoom-out",
        title: "Zoom out",
        detail: "Action",
    },
    PaletteCommand {
        id: "zoom-reset",
        title: "Reset zoom",
        detail: "Action",
    },
    PaletteCommand {
        id: "new-space",
        title: "New space",
        detail: "Action",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaletteCommand {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) detail: &'static str,
}

pub(crate) const SHORTCUT_BINDINGS: &[ShortcutBinding] = &[
    ShortcutBinding {
        key: "ctrl+l",
        action: "focus-address",
    },
    ShortcutBinding {
        key: "ctrl+shift+b",
        action: "toggle-shield",
    },
    ShortcutBinding {
        key: "ctrl+k",
        action: "command-palette",
    },
    ShortcutBinding {
        key: "ctrl+f",
        action: "find-in-page",
    },
    ShortcutBinding {
        key: "ctrl+shift+f",
        action: "find-in-page",
    },
    ShortcutBinding {
        key: "ctrl+t",
        action: "new-tab",
    },
    ShortcutBinding {
        key: "ctrl+shift+t",
        action: "reopen-closed-tab",
    },
    ShortcutBinding {
        key: "ctrl+w",
        action: "close-tab",
    },
    ShortcutBinding {
        key: "ctrl+tab",
        action: "next-tab",
    },
    ShortcutBinding {
        key: "ctrl+shift+tab",
        action: "previous-tab",
    },
    ShortcutBinding {
        key: "ctrl+r",
        action: "reload",
    },
    ShortcutBinding {
        key: "ctrl+shift+r",
        action: "hard-reload",
    },
    ShortcutBinding {
        key: "ctrl+=",
        action: "zoom-in",
    },
    ShortcutBinding {
        key: "ctrl+shift+=",
        action: "zoom-in",
    },
    ShortcutBinding {
        key: "ctrl++",
        action: "zoom-in",
    },
    ShortcutBinding {
        key: "ctrl+shift++",
        action: "zoom-in",
    },
    ShortcutBinding {
        key: "ctrl+-",
        action: "zoom-out",
    },
    ShortcutBinding {
        key: "ctrl+shift+-",
        action: "zoom-out",
    },
    ShortcutBinding {
        key: "ctrl+0",
        action: "zoom-reset",
    },
    ShortcutBinding {
        key: "ctrl+shift+0",
        action: "zoom-reset",
    },
    ShortcutBinding {
        key: "ctrl+1..9",
        action: "activate-tab-number",
    },
    ShortcutBinding {
        key: "ctrl+shift+s",
        action: "split-view",
    },
    ShortcutBinding {
        key: "ctrl+shift+w",
        action: "close-window",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShortcutBinding {
    pub(crate) key: &'static str,
    pub(crate) action: &'static str,
}

pub(crate) struct CorePlugin {
    manifest: Manifest,
}

impl CorePlugin {
    pub(crate) fn new() -> Self {
        Self {
            manifest: Manifest {
                id: "lumen.core".into(),
                required: Vec::new(),
                optional: Vec::new(),
                provides: vec!["commands".into(), "keymap".into(), "settings".into()],
            },
        }
    }
}

impl Plugin for CorePlugin {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn apply(&mut self, ctx: &mut PluginCtx<'_>) -> Result<(), PluginError> {
        for command in PALETTE_COMMANDS {
            ctx.contribute("palette.action", command.id, Order::default());
        }
        for binding in SHORTCUT_BINDINGS {
            ctx.contribute(
                "keymap.binding",
                format!("{}={}", binding.key, binding.action),
                Order::default(),
            );
        }
        Ok(())
    }
}
