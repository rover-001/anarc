---
type: decision
title: "Unlock full display refresh rate up to 240Hz for hardware accelerated rendering"
timestamp: 2026-11-12T13:37:46.584227121+00:00
---
Raised OSR frame rate cap from 60 to 240 fps on systems with hardware GPU acceleration, allowing Anarc to automatically match 144Hz and high-refresh displays while keeping software-only rendering clamped to 60 fps to preserve CPU on headless or non-GPU environments.
