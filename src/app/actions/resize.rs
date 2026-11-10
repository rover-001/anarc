use super::super::*;

impl Lumen {
    pub(in crate::app) fn animate_sidebar_toggle(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        let _ = window;
        let from = self.sidebar_width_transition.map_or_else(
            || self.effective_sidebar_width(),
            |(from, to, started, _)| {
                let progress = started.elapsed().as_secs_f32()
                    / ui::motion::MOTION_SLOW.as_secs_f32().max(f32::EPSILON);
                from + (to - from) * ui::motion::ease_out_quint(progress)
            },
        );
        let collapsing = !self.is_sidebar_collapsed();
        let target = if collapsing {
            SIDEBAR_COLLAPSED_WIDTH
        } else {
            self.sidebar_width
        };
        #[cfg(all(target_os = "linux", not(test)))]
        self.begin_browser_resize_drag(window, false);
        #[cfg(all(target_os = "linux", not(test)))]
        let final_sizes = self.browser_sizes_for_sidebar(window, target);
        self.sidebar_transition_serial = self.sidebar_transition_serial.wrapping_add(1);
        let serial = self.sidebar_transition_serial;
        self.sidebar_width_transition = Some((from, target, Instant::now(), serial));
        self.sidebar_collapsed = collapsing;
        self.save_state();

        #[cfg(all(target_os = "linux", not(test)))]
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(ui::motion::duration(ui::motion::MOTION_SLOW))
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
    #[cfg(all(target_os = "linux", not(test)))]
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
