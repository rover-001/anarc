---
type: decision
title: "Use Brave adblock matcher on an isolated worker"
timestamp: 2026-11-09T12:46:21.009193998+00:00
branch: "p2-shield"
---
Phase 2 shield uses adblock 0.13.5 for EasyList-compatible matching. Its Engine is non-Send/non-Sync with default features, so the engine is compiled and owned on a dedicated worker thread, while CEF IO callbacks submit synchronous match queries over a bounded channel. This avoids unsafe cross-thread access and keeps list parsing off UI startup; request callbacks pay channel handoff cost.
