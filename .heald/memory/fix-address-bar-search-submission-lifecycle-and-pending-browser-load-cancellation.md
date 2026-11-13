---
type: decision
title: "Fix address bar search submission lifecycle and pending browser load cancellation"
timestamp: 2026-11-12T16:49:47.756708852+00:00
---
1. Stored initial_url on BrowserSharedState during browser creation so that take_created_browsers only triggers frame.load_url if pending_url diverged from initial_url while pending; this eliminates premature ERR_ABORTED cancellation on newly opened or discarded tabs. 2. Updated navigate to accept window: Option<&mut Window> and immediately transfer focus to browser_focus_handle and the CEF tab via focus_browser on Enter keydown or autocomplete suggestion click, ensuring address_focused resets cleanly and user keystrokes route to the page.
