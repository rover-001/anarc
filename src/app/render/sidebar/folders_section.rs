//! Sidebar folders section with git-tree connectors.

use super::SidebarTab;
use super::super::*;

impl Anarc {
    pub(super) fn render_folders_section(
        &self,
        theme: visual::SpaceTheme,
        active_tab_id: Option<&str>,
        tabs: &[SidebarTab],
        stored_split_pair: Option<&SplitPair>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_space = self.active_space();
        let folders = active_space.folders.clone();
        let mut folders_section = div()
            .id("folders-section")
            .w_full()
            .mt_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(Icon::new(IconName::Folder).size(px(12.0 * UI_SCALE))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.muted))
                            .child("Spaces"),
                    )
                    .child(
                        div()
                            .id("new-folder-button")
                            .size(px(18.0 * UI_SCALE))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .text_color(rgb(theme.muted))
                            .hover(move |style| {
                                style
                                    .bg(crate::ui::visual::translucent_surface(
                                        theme.surface_hover,
                                        0x40,
                                    ))
                                    .text_color(rgb(theme.text))
                            })
                            .child(Icon::new(IconName::FolderPlus).size(px(13.0 * UI_SCALE)))
                            .on_click(cx.listener(
                                |this, event: &gpui_kit::ClickEvent, window, cx| {
                                    this.open_create_folder_modal(
                                        None,
                                        event.position(),
                                        window,
                                        cx,
                                    );
                                    cx.notify();
                                },
                            )),
                    ),
            );

        for folder in &folders {
            let members = tabs
                .iter()
                .filter(|(id, _, _)| {
                    let is_split =
                        stored_split_pair.is_some_and(|pair| pair.panes().contains(&id.as_str()));
                    let in_folder = active_space
                        .tabs
                        .iter()
                        .find(|tab| tab.id == *id)
                        .is_some_and(|tab| {
                            !tab.is_pinned() && tab.folder.as_deref() == Some(folder.id.as_str())
                        });
                    !is_split && in_folder
                })
                .cloned()
                .collect::<Vec<_>>();
            folders_section =
                folders_section.child(render_folder_block(folder, members, active_tab_id, theme, cx));
        }
        folders_section
    }
}

fn render_folder_block(
    folder: &Folder,
    members: Vec<SidebarTab>,
    active_tab_id: Option<&str>,
    theme: visual::SpaceTheme,
    cx: &mut Context<Anarc>,
) -> impl IntoElement {
    let folder_id = folder.id.clone();
    let toggle_id = folder.id.clone();
    let context_id = folder.id.clone();
    let drop_folder_id = folder.id.clone();
    let folder_name = folder.name.clone();
    let expanded = folder.expanded;

    let mut folder_block = div()
        .id(format!("folder-block-{folder_id}"))
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .id(format!("folder-{folder_id}"))
                .w_full()
                .h(px(visual::TAB_ROW_HEIGHT))
                .cursor_pointer()
                .active(|style| style.opacity(0.92))
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_center()
                        .gap(px(visual::SIDEBAR_ROW_GAP))
                        .px(px(visual::SIDEBAR_ROW_PADDING_X))
                        .rounded_lg()
                        .hover(|row| {
                            row.bg(crate::ui::visual::translucent_surface(
                                theme.surface_hover,
                                0x40,
                            ))
                        })
                        .child(
                            div().text_color(rgb(theme.muted)).child(
                                Icon::new(if expanded {
                                    IconName::FolderOpen
                                } else {
                                    IconName::Folder
                                })
                                .size(px(14.0 * UI_SCALE)),
                            ),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .text_color(rgb(theme.text))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(folder_name),
                        ),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(folder) = this
                        .active_space_mut()
                        .folders
                        .iter_mut()
                        .find(|folder| folder.id == toggle_id)
                    {
                        folder.expanded = !folder.expanded;
                    }
                    this.save_state();
                    cx.notify();
                }))
                .on_mouse_down(
                    gpui_kit::MouseButton::Right,
                    cx.listener(
                        move |this, event: &gpui_kit::MouseDownEvent, window, cx| {
                            let (x, y, width, height) = tab_context_menu_position(event, window);
                            this.folder_context_menu = Some(FolderContextMenu {
                                folder_id: context_id.clone(),
                                x,
                                y,
                                width,
                                height,
                            });
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                )
                .drag_over::<TabDrag>(move |style, _, _, _| {
                    style.bg(crate::ui::visual::translucent_surface(theme.surface, 0x30))
                })
                .on_drop::<TabDrag>(cx.listener(move |this, drag: &TabDrag, _window, cx| {
                    this.move_tab_to_folder(&drag.tab_id, &drop_folder_id);
                    cx.notify();
                })),
        );

    if expanded && !members.is_empty() {
        // Git-tree style connector: a vertical line drops from the
        // folder row and a short tick branches into each tab.
        let tree_indent = visual::SIDEBAR_ROW_PADDING_X + 7.0 * UI_SCALE;
        let mut members_tree = div()
            .id(format!("folder-tree-{folder_id}"))
            .relative()
            .w_full()
            .flex()
            .flex_col();
        for (id, title, favicon) in &members {
            members_tree = members_tree.child(
                div()
                    .relative()
                    .w_full()
                    .child(section_tab_row(
                        id.clone(),
                        title.clone(),
                        favicon.clone(),
                        active_tab_id == Some(id.as_str()),
                        false,
                        theme,
                        tree_indent,
                        cx,
                    ))
                    .child(
                        div()
                            .absolute()
                            .left(px(tree_indent - UI_SCALE))
                            .top(px(visual::TAB_ROW_HEIGHT / 2.0))
                            .w(px(visual::SIDEBAR_ROW_PADDING_X + UI_SCALE))
                            .h(px(1.0))
                            .bg(rgb(theme.border)),
                    ),
            );
        }
        members_tree = members_tree.child(
            div()
                .absolute()
                .left(px(tree_indent))
                .top_0()
                .w(px(1.0 * UI_SCALE))
                .h(px(
                    visual::TAB_ROW_HEIGHT * members.len() as f32 - visual::TAB_ROW_HEIGHT / 2.0,
                ))
                .bg(rgb(theme.border)),
        );
        folder_block = folder_block.child(members_tree);
    }

    folder_block
}
