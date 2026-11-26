//! Kept/pinned tab grid, group-of-3 wrapping, and drag-drop slot handling.

use super::{KeptTabRow, SidebarTab};
use super::super::*;

const MAX_KEPT_PER_ROW: usize = 3;

impl Anarc {
    pub(super) fn render_kept_section(
        &self,
        theme: visual::SpaceTheme,
        active_tab_id: Option<&str>,
        tabs: &[SidebarTab],
        stored_split_pair: Option<&SplitPair>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_space = self.active_space();
        let dragging = cx.has_active_drag();
        let dragged_kept_source = self.kept_drag_source_id.as_deref();
        let kept_tabs = tabs
            .iter()
            .filter(|(id, _, _)| {
                let is_split = stored_split_pair.is_some_and(|pair| pair.panes().contains(&id.as_str()));
                let is_kept = active_space
                    .tabs
                    .iter()
                    .find(|tab| tab.id == *id)
                    .is_some_and(|tab| tab.is_pinned());
                !is_split && is_kept && !(dragging && dragged_kept_source == Some(id.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();

        let mut kept_boxes = div()
            .id("kept-tab-boxes")
            .w_full()
            .flex()
            .flex_col()
            .gap_1();

        if kept_tabs.is_empty() && !dragging {
            kept_boxes = kept_boxes.child(
                div()
                    .id("kept-empty-hint")
                    .w_full()
                    .h(px(visual::TAB_ROW_HEIGHT))
                    .rounded_lg()
                    .border_1()
                    .border_dashed()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Icon::new(IconName::StarFill).size(px(14.0 * UI_SCALE))),
            );
        } else if !kept_tabs.is_empty() {
            let rows = build_kept_rows(&kept_tabs, active_space);
            // While a tab is in flight, the hovered position opens a slot
            // and neighboring tiles flex around it.
            let hovered_kept_tab = self.kept_drag_hovered_tab.as_deref();
            let hovered_after = self.kept_drag_hover_after;
            for (row_index, (group, members)) in rows.iter().enumerate() {
                if group.is_none() || members.len() == 1 {
                    let (id, title, favicon) = &members[0];
                    let kept_row = div()
                        .id(format!("kept-single-row-{row_index}"))
                        .w_full()
                        .flex()
                        .gap(px(2.0 * UI_SCALE))
                        .child(kept_drop_slot(
                            format!("single-before-{row_index}"),
                            group.clone(),
                            id.clone(),
                            false,
                            0.5,
                            dragging && hovered_kept_tab == Some(id.as_str()) && !hovered_after,
                            theme,
                            cx,
                        ))
                        .child(kept_tab_box(
                            id.clone(),
                            title.clone(),
                            favicon.clone(),
                            active_tab_id == Some(id.as_str()),
                            !dragging,
                            theme,
                            cx,
                        ))
                        .child(kept_drop_slot(
                            format!("single-after-{row_index}"),
                            group.clone(),
                            id.clone(),
                            true,
                            0.5,
                            dragging && hovered_kept_tab == Some(id.as_str()) && hovered_after,
                            theme,
                            cx,
                        ));
                    kept_boxes = kept_boxes.child(kept_row);
                } else {
                    for (chunk_index, chunk) in members.chunks(MAX_KEPT_PER_ROW).enumerate() {
                        let mut kept_row = div()
                            .id(format!("kept-group-{row_index}-row-{chunk_index}"))
                            .w_full()
                            .flex()
                            .gap(px(2.0 * UI_SCALE));
                        let slot_ratio = 1.0 / (chunk.len() as f32 + 1.0);
                        for (id, title, favicon) in chunk {
                            kept_row = kept_row
                                .child(kept_drop_slot(
                                    format!("group-{row_index}-{chunk_index}-before-{id}"),
                                    group.clone(),
                                    id.clone(),
                                    false,
                                    slot_ratio,
                                    dragging
                                        && hovered_kept_tab == Some(id.as_str())
                                        && !hovered_after,
                                    theme,
                                    cx,
                                ))
                                .child(kept_tab_box(
                                    id.clone(),
                                    title.clone(),
                                    favicon.clone(),
                                    active_tab_id == Some(id.as_str()),
                                    false,
                                    theme,
                                    cx,
                                ))
                                .child(kept_drop_slot(
                                    format!("group-{row_index}-{chunk_index}-after-{id}"),
                                    group.clone(),
                                    id.clone(),
                                    true,
                                    slot_ratio,
                                    dragging
                                        && hovered_kept_tab == Some(id.as_str())
                                        && hovered_after,
                                    theme,
                                    cx,
                                ));
                        }
                        kept_boxes = kept_boxes.child(kept_row);
                    }
                }
            }
        }

        if dragging {
            kept_boxes = kept_boxes.child(
                div()
                    .id("kept-own-row-drop-target")
                    .w_full()
                    .h(px(visual::TAB_ROW_HEIGHT))
                    .rounded_xl()
                    .border_1()
                    .border_dashed()
                    .border_color(rgb(0x00000000))
                    .flex()
                    .items_center()
                    .justify_center()
                    .drag_over::<TabDrag>(move |style, _, _, _| style.border_color(rgb(theme.accent)))
                    .on_drop(cx.listener(|this, drag: &TabDrag, _, cx| {
                        this.kept_drag_hovered_tab = None;
                        this.kept_drag_hover_after = false;
                        this.kept_drag_source_id = None;
                        this.kept_section_drag_hovered = false;
                        this.pin_tab(&drag.tab_id, true);
                        this.set_kept_group(&drag.tab_id, None);
                        this.move_tab_to_end(&drag.tab_id);
                        cx.notify();
                    })),
            );
        }

        let kept_section_drag_hover_view = cx.weak_entity();
        div()
            .id("pinned-drop-zone")
            .w_full()
            .flex()
            .flex_col()
            .rounded_lg()
            .on_drop::<TabDrag>(cx.listener(|this, drag: &TabDrag, _window, cx| {
                this.kept_drag_hovered_tab = None;
                this.kept_drag_hover_after = false;
                this.kept_drag_source_id = None;
                this.kept_section_drag_hovered = false;
                this.pin_tab(&drag.tab_id, true);
                // Dropping on the section background (rather than a tile)
                // detaches the tab into its own full-width row.
                this.set_kept_group(&drag.tab_id, None);
                this.move_tab_to_end(&drag.tab_id);
                cx.notify();
            }))
            .drag_over::<TabDrag>(move |style, _, _, app| {
                kept_section_drag_hover_view
                    .update(app, |this, cx| {
                        if !this.kept_section_drag_hovered {
                            this.kept_section_drag_hovered = true;
                            cx.notify();
                        }
                    })
                    .ok();
                style
            })
            .on_mouse_move(cx.listener(|this, _, _, cx| {
                if cx.has_active_drag() && !this.kept_section_drag_hovered {
                    this.kept_section_drag_hovered = true;
                    cx.notify();
                }
            }))
            .on_mouse_exit(cx.listener(|this, _, _, cx| {
                let had_target = this.kept_drag_hovered_tab.take().is_some();
                let had_after = this.kept_drag_hover_after;
                let had_section_hover = this.kept_section_drag_hovered;
                this.kept_drag_hover_after = false;
                this.kept_section_drag_hovered = false;
                if had_target || had_after || had_section_hover {
                    cx.notify();
                }
            }))
            .child(kept_boxes)
    }
}

fn build_kept_rows(kept_tabs: &[SidebarTab], active_space: &Space) -> Vec<KeptTabRow> {
    let tab_group = |id: &str| -> Option<String> {
        active_space
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .and_then(|tab| tab.kept_group.clone())
    };
    let mut rows: Vec<KeptTabRow> = Vec::new();
    let mut seen_groups: Vec<String> = Vec::new();
    for (id, title, favicon) in kept_tabs {
        match tab_group(id) {
            None => rows.push((None, vec![(id.clone(), title.clone(), favicon.clone())])),
            Some(group) => {
                if !seen_groups.contains(&group) {
                    seen_groups.push(group.clone());
                    let members = kept_tabs
                        .iter()
                        .filter(|(member_id, _, _)| {
                            tab_group(member_id).as_deref() == Some(group.as_str())
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    rows.push((Some(group), members));
                }
            }
        }
    }
    rows
}
