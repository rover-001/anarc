use super::super::state::PeekState;
use super::super::*;

/// Header strip height of the floating peek card.
pub(in crate::app) const PEEK_HEADER_HEIGHT: f32 = 40.0 * UI_SCALE;
/// Inset between the content card edge and the floating peek card.
pub(in crate::app) const PEEK_MARGIN: f32 = 12.0 * UI_SCALE;

/// Window-space geometry shared by the overlay renderer and the CEF viewport.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, allow(dead_code))]
pub(in crate::app) struct PeekLayout {
    pub(in crate::app) card_x: f32,
    pub(in crate::app) card_y: f32,
    pub(in crate::app) card_w: f32,
    pub(in crate::app) card_h: f32,
    pub(in crate::app) viewport_x: f32,
    pub(in crate::app) viewport_y: f32,
    pub(in crate::app) viewport_w: i32,
    pub(in crate::app) viewport_h: i32,
}

/// Arc anchors its peek window to the top-right of the content area. The width
/// tracks the window but stays in a readable range, and the card never leaves
/// the content column.
pub(in crate::app) fn peek_layout(
    window_width: f32,
    window_height: f32,
    sidebar_width: f32,
    sidebar_on_right: bool,
    topbar_height: f32,
) -> PeekLayout {
    let content_width = (window_width - sidebar_width - 2.0 * CONTENT_BEZEL_X).max(1.0);
    let content_height = (window_height - topbar_height - CONTENT_BEZEL_BOTTOM).max(1.0);
    let card_w = (content_width * 0.62)
        .clamp(340.0 * UI_SCALE, 820.0 * UI_SCALE)
        .min((content_width - 2.0 * PEEK_MARGIN).max(1.0));
    let card_h = (content_height - 2.0 * PEEK_MARGIN).max(1.0);
    let right_inset = if sidebar_on_right {
        sidebar_width + CONTENT_BEZEL_X + PEEK_MARGIN
    } else {
        CONTENT_BEZEL_X + PEEK_MARGIN
    };
    let card_x = (window_width - right_inset - card_w).max(0.0);
    let card_y = topbar_height + PEEK_MARGIN;
    let viewport_w = (card_w - 2.0).max(1.0) as i32;
    let viewport_h = (card_h - PEEK_HEADER_HEIGHT - 2.0).max(1.0) as i32;
    PeekLayout {
        card_x,
        card_y,
        card_w,
        card_h,
        viewport_x: card_x + 1.0,
        viewport_y: card_y + PEEK_HEADER_HEIGHT + 1.0,
        viewport_w,
        viewport_h,
    }
}

impl Lumen {
    /// Opens a user-requested link as a peek overlay instead of a new tab. A
    /// second link while the peek is open reuses the same preview window.
    pub(in crate::app) fn open_peek(&mut self, space_id: &str, url: String) {
        if let Some(existing) = self.peek.as_mut() {
            if existing.space_id == space_id {
                existing.url = url.clone();
                existing.title = peek_title_for(&url);
                self.engine.navigate(&existing.id, &url);
                return;
            }
            self.close_peek();
        }
        let id = self.next_peek_id(space_id);
        self.emit_plugin_text("peek.opened", url.clone());
        self.peek = Some(PeekState {
            id,
            space_id: space_id.to_owned(),
            title: peek_title_for(&url),
            url,
        });
        self.peek_focused = false;
    }

    fn next_peek_id(&mut self, space_id: &str) -> String {
        loop {
            self.peek_serial = self.peek_serial.wrapping_add(1);
            let candidate = format!("{space_id}-peek-{}", self.peek_serial);
            let taken = self
                .spaces
                .iter()
                .any(|space| space.tabs.iter().any(|tab| tab.id == candidate));
            if !taken {
                return candidate;
            }
        }
    }

    pub(in crate::app) fn close_peek(&mut self) {
        if let Some(peek) = self.peek.take() {
            self.engine.close_browser(&peek.id);
            self.emit_plugin_text("peek.closed", peek.id);
        }
        self.peek_focused = false;
    }

    /// Dismisses the preview after the page itself closed its window (OAuth
    /// flows finish with `window.close()`). CEF is already tearing the browser
    /// down, so the engine only drops its bookkeeping.
    pub(in crate::app) fn close_peek_from_page(&mut self) {
        if let Some(peek) = self.peek.take() {
            self.engine.forget_closed_browser(&peek.id);
            self.emit_plugin_text("peek.closed", peek.id);
        }
        self.peek_focused = false;
    }

    /// Promotes the preview to a regular tab. The tab reuses the preview's
    /// browser id, so the already-loaded page (history, scroll, form state)
    /// carries over instead of reloading.
    pub(in crate::app) fn open_peek_in_tab(&mut self) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        self.peek_focused = false;
        let Some(space) = self
            .spaces
            .iter_mut()
            .find(|space| space.id == peek.space_id)
        else {
            self.engine.close_browser(&peek.id);
            return;
        };
        let mut tab = Tab::new(&peek.id, peek.title.clone(), peek.url.clone());
        tab.state = TabState::Live;
        space.tabs.push(tab);
        space.active = Some(peek.id.clone());
        let space_id = peek.space_id.clone();
        let tab_id = peek.id.clone();
        self.activate_space(&space_id);
        self.emit_plugin_text("tab.created", tab_id);
        self.save_state();
    }

    pub(crate) fn focus_peek_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(peek_id) = self.peek.as_ref().map(|peek| peek.id.clone()) else {
            return;
        };
        self.browser_focused = true;
        self.peek_focused = true;
        self.show_suggestions = false;
        self.selected_suggestion = None;
        self.close_app_page();
        self.plugins_menu_open = false;
        self.tab_context_menu = None;
        self.space_context_menu = None;
        self.folder_context_menu = None;
        self.web_context_menu = None;
        self.theme_editor = None;
        window.focus(&self.browser_focus_handle, cx);
        let active = self.active_tab_id().to_owned();
        self.engine.set_focus(&active, false);
        if let Some(pair) = self.current_split_pair() {
            self.engine.set_focus(&pair.first, false);
            self.engine.set_focus(&pair.second, false);
        }
        self.engine.set_focus(&peek_id, true);
    }
}

pub(in crate::app) fn peek_title_for(url: &str) -> String {
    crate::shield::url_host(url).unwrap_or_else(|| url.to_owned())
}
