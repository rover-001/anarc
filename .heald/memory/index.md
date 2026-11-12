---
type: summary
title: "Memory Index"
---
## Project Memory Manifest

| Title | Type | Date | Tags | Summary |
| :--- | :--- | :--- | :--- | :--- |
| **Fix pinned tab restoration, trackpad visual zoom, and clipboard selection** (`fix-pinned-tab-restoration-trackpad-visual-zoom-and-clipboard-selection.md`) | `decision` | 2026-11-12 | - | 1. Resolved pinned/favorite tab URL persistence: added pi... |
| **Fix trackpad zoom and web copy-paste synchronization** (`fix-trackpad-zoom-and-web-copy-paste-synchronization.md`) | `decision` | 2026-11-12 | - | Resolved trackpad zooming by tuning pinch sensitivity fro... |
| **Multi-instance launches get CEF profile slots** (`multi-instance-launches-get-cef-profile-slots.md`) | `decision` | 2026-11-11 | - | Anarc allows up to 9 concurrent windows. The installed la... |
| **Page popups are CEF-native and render in the peek overlay** (`page-popups-are-cef-native-and-render-in-the-peek-overlay.md`) | `decision` | 2026-11-11 | - | window.open/sign-in popups are no longer cancelled and re... |
| **Cap CEF OSR frame rate at 60 and pass button flags on mouse moves** (`cap-cef-osr-frame-rate-at-60-and-pass-button-flags-on-mouse-moves.md`) | `decision` | 2026-11-11 | - | OSR paints cost a full-window BGRA memcpy plus a new Rend... |
| **Implement file upload dialog handler and drag-and-drop file ingestion** (`implement-file-upload-dialog-handler-and-drag-and-drop-file-ingestion.md`) | `decision` | 2026-11-11 | - | Implemented CEF DialogHandler in LumenDialogHandler spawn... |
| **Shield mirrors uBlock Origin Lite; no WebExtensions in Alloy** (`shield-mirrors-ublock-origin-lite-no-webextensions-in-alloy.md`) | `decision` | 2026-11-11 | - | The built-in shield compiles the same filter sets the use... |
| **Fix external Chromium launch on install and uncloseable popup tabs** (`fix-external-chromium-launch-on-install-and-uncloseable-popup-tabs.md`) | `decision` | 2026-11-11 | - | 1. Updated install.sh to export PATH with bin/CEF dirs, l... |
| **Download handler fix and uBlock Origin integration with YouTube ad shield** (`download-handler-fix-and-ublock-origin-integration-with-youtube-ad-shield.md`) | `decision` | 2026-11-10 | - | Implemented can_download=1 and on_before_download=1 in Lu... |
| **Sidebar right-click context menu and blank tab close animation fix** (`sidebar-right-click-context-menu-and-blank-tab-close-animation-fix.md`) | `decision` | 2026-11-10 | - | Fixed sidebar canvas right-click context menu by attachin... |
| **Eliminate heavy site scroll and network stutter on hianime** (`eliminate-heavy-site-scroll-and-network-stutter-on-hianime.md`) | `decision` | 2026-11-10 | - | Diagnosed lag on heavy media/ad-heavy sites (such as hian... |
| **Fix Chromium subprocess launch and transition frame flash, complete Anarc rename** (`fix-chromium-subprocess-launch-and-transition-frame-flash-complete-anarc-rename.md`) | `decision` | 2026-11-10 | - | Renamed package to anarc in Cargo.toml and install script... |
| **Configure release profiles and native cpu rustflags** (`configure-release-profiles-and-native-cpu-rustflags.md`) | `decision` | 2026-11-10 | - | Added release profile with opt-level=3, thin LTO, 1 codeg... |
| **Comprehensive performance audit and roadmap (performance.md)** (`comprehensive-performance-audit-and-roadmap-performancemd.md`) | `decision` | 2026-11-10 | - | Audited entire Anarc codebase across graphics, state pers... |
| **Create private repo anarc and publish installation guide** (`create-private-repo-anarc-and-publish-installation-guide.md`) | `decision` | 2026-11-10 | - | Created private GitHub repository rover-001/anarc, harden... |
| **Rebrand browser to Anarc and polish button responsiveness** (`rebrand-browser-to-anarc-and-polish-button-responsiveness.md`) | `decision` | 2026-11-10 | - | Renamed browser from Lumen to Anarc across window title, ... |
| **Plugin platform architecture (agent_plan.md)** (`plugin-platform-architecture-agentplanmd.md`) | `decision` | 2026-11-10 | - | Cordis-style Rust kernel in src/plugin (services by key, ... |
| **Use Brave adblock matcher on an isolated worker** (`use-brave-adblock-matcher-on-an-isolated-worker.md`) | `decision` | 2026-11-09 | - | Phase 2 shield uses adblock 0.13.5 for EasyList-compatibl... |
