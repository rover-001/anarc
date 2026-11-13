---
type: decision
title: "Page popups are CEF-native and render in the peek overlay"
timestamp: 2026-11-11T16:18:55.339014023+00:00
---
window.open/sign-in popups are no longer cancelled and re-created by the app. on_before_popup returns 0 with a windowless WindowInfo + full client so CEF creates the browser with a live window.opener, and the app only binds the peek overlay to that browser id (PopupRequest.browser_id). Pending state is queued in EngineEvents.pending_popup_states so ensure_browser cannot duplicate the id. window.close() routes through close_peek_from_page -> Engine::forget_closed_browser (no double close). Narrative: auth popup navigated the tab destroyed window.opener, so the callback postMessage failed and window.close() closed the main tab.
