use super::super::*;

impl Anarc {
    pub(in crate::app) fn animate_sidebar_toggle(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let collapsing = !self.is_sidebar_collapsed();
        let target = if collapsing {
            match self.preferences.sidebar_mode {
                crate::settings::SidebarMode::Hidden => 0.0,
                crate::settings::SidebarMode::Icons => SIDEBAR_COLLAPSED_WIDTH,
                crate::settings::SidebarMode::Hover => 0.0,
            }
        } else {
            self.sidebar_width
        };
        self.animate_sidebar_width_to(target, collapsing, window, cx);
    }

    pub(in crate::app) fn animate_sidebar_hover(
        &mut self,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar_hovered == hovered {
            return;
        }
        let target = if hovered { self.sidebar_width } else { 0.0 };
        self.animate_sidebar_width_to(target, true, window, cx);
        self.sidebar_hovered = hovered;
        cx.notify();
    }

    pub(in crate::app) fn animate_sidebar_width_to(
        &mut self,
        target: f32,
        collapsed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        let _ = window;
        let from = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = ui::motion::sidebar().progress(started.elapsed());
                from + (to - from) * progress
            },
        );
        #[cfg(not(test))]
        self.begin_browser_resize_drag(window, false, true);
        #[cfg(not(test))]
        let final_sizes = self.browser_sizes_for_sidebar(window, target);
        self.sidebar_transition_serial = self.sidebar_transition_serial.wrapping_add(1);
        let serial = self.sidebar_transition_serial;
        self.sidebar_width_transition = Some((from, target, Instant::now(), serial));
        let collapsed_changed = self.sidebar_collapsed != collapsed;
        self.sidebar_collapsed = collapsed;
        if collapsed_changed {
            self.save_state();
        }

        #[cfg(not(test))]
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(ui::motion::sidebar().duration())
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.sidebar_transition_serial == serial {
                    this.sidebar_width_transition = None;
                    this.finish_browser_resize_drag(final_sizes, "sidebar-toggle");
                    cx.notify();
                }
            });
        })
        .detach();

        cx.notify();
    }
    #[cfg(not(test))]
    pub(in crate::app) fn finish_browser_resize_drag(
        &mut self,
        sizes: Vec<split_view::BrowserSize>,
        kind: &str,
    ) {
        for size in self.resize_drag.finish(sizes) {
            resize_debug(format_args!(
                "drag_end kind={kind} tab={} size={}x{}",
                size.tab_id, size.width, size.height
            ));
            self.engine
                .flush_resize_browser(&size.tab_id, size.width, size.height);
        }
    }
}
