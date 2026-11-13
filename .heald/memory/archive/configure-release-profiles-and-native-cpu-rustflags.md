---
type: decision
title: "Configure release profiles and native cpu rustflags"
timestamp: 2026-11-10T12:05:42.601221204+00:00
---
Added release profile with opt-level=3, thin LTO, 1 codegen-unit, panic=abort, strip=symbols; pruned image crate default features to png,jpeg,gif,webp,ico; configured .cargo/config.toml with target-cpu=native.
