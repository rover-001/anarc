# Browser performance benchmark

Single-tab, 40-second benchmark comparing Anarc against Brave, Chromium and Firefox on the same local page. Each browser runs one window with one tab: the benchmark page. There are no background worker tabs, so results are not skewed by hidden-tab timers, extra renderer processes, or process-tree RAM attribution.

## Run

```sh
cd tools/browser-bench
python3 run_bench.py             # all four browsers, sequentially
python3 run_bench.py anarc       # one browser
python3 run_bench.py brave chromium
```

Requirements: `python3`, `node`, `gpu-screen-recorder`, and the browsers installed at `/usr/bin/chromium`, `/usr/bin/brave`, `/usr/bin/firefox`, plus `anarc` and `anarc-ctl` on `PATH`. The harness starts its own HTTP server on `127.0.0.1:8877`, launches each browser with a clean temporary profile, records the display to `recordings/<browser>.mp4`, and writes the report to `results/<browser>.json`.

## Workload

Five 8-second stages, repeated rAF-driven work:

1. WebGL 3D particle cloud (12,000 points)
2. Canvas 2D particle storm (32,000 points)
3. CSS 3D engine (450 rotating cards)
4. DOM mutation and layout thrash (160 nodes rebuilt per frame)
5. JIT compute (84x84 matrix multiply + 120,000-sample Monte Carlo)

All browsers run uncapped: Chromium/Brave get `--disable-gpu-vsync --disable-frame-rate-limit`, Firefox gets `layout.frame_rate = 0`, and Anarc gets native CEF OSR with the same GPU/frame-limit switches plus ANGLE/Vulkan on GPUs that support it (`ANARC_NO_VULKAN=1` opts out). The harness also sets `ANARC_OSR_HEADROOM_PERCENT=0` and `ANARC_CEF_INIT_DELAY_MS=0` for Anarc, matching the zero-headroom/low-latency setup given to the other browsers. A dropped frame is any rAF interval above `1000/144 * 1.5 = 10.42 ms`.

Anarc paints are throttled to one full-frame copy per display refresh by default; `ANARC_OSR_MIN_PAINT_INTERVAL_MS` overrides it (0 disables throttling). Page rAF is unaffected because the timer is not coupled to CEF's paint acknowledgements.

## Scoring

```
fpsScore      = meanFps / 144 * 500
lowScore      = p1LowFps / 144 * 300
stability     = max(0, 100 - droppedPct * 2)
penalties     = min(50, coldStartupMs / 40) + min(50, peakRssMb / 40)
composite     = fpsScore + lowScore + stability + (100 - penalties)
```

Mean FPS carries the most weight (500), followed by 1% low FPS (300). Startup and RAM penalties are capped at 50 points each, so beyond roughly 2000 MB RSS there is no further score loss.

## Files

- `run_bench.py` — launcher, profile cleanup, recording, report collection
- `server.mjs` — static page server, RAM sampler, report endpoint
- `index.html` — benchmark stages, frame timing, scoring, summary UI

`results/` and `recordings/` are generated at runtime and are not committed.
