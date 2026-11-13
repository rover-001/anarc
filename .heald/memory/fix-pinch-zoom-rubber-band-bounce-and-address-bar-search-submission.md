---
type: decision
title: "Fix pinch zoom rubber-band bounce and address bar search submission"
timestamp: 2026-11-12T13:27:32.247271991+00:00
---
1. Replaced wheel event injection during trackpad pinch zoom with instant programmatic window.scrollBy via Blink DevTools, eliminating overscroll spring bounce animations and inner element scroll hijacking while maintaining cursor anchoring. 2. Fixed address bar Enter key event handling so that entering queries navigates immediately regardless of whether autocomplete suggestions are open or selected. 3. Added pending browser URL resolution in CEF runtime to guarantee navigations on newly created or pending tabs load reliably.
