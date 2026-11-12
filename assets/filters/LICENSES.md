# Bundled filter list licenses

The shield compiles these lists into the binary (`src/shield.rs`, `BUNDLED_FILTER_LISTS`). They mirror the filter sets enabled in the user's uBlock Origin Lite install, which cannot itself run here because this build uses CEF's Alloy runtime (no extension support).

| File | Source | Retrieved | License |
| --- | --- | --- | --- |
| `easylist.txt` | <https://easylist.to/easylist/easylist.txt> | 2026-11-10 | GPL-3.0-or-later or CC BY-SA 3.0 (see `EASYLIST-LICENSE.md`) |
| `easyprivacy.txt` | <https://easylist.to/easylist/easyprivacy.txt> | 2026-11-11 | GPL-3.0-or-later or CC BY-SA 3.0 |
| `ublock-filters.txt` | <https://ublockorigin.github.io/uAssets/filters/filters.txt> | 2026-11-11 | GPL-3.0, © uBlock Origin contributors |
| `ublock-badware.txt` | <https://ublockorigin.github.io/uAssets/filters/badware.txt> | 2026-11-11 | GPL-3.0, © uBlock Origin contributors |
| `pgl.txt` | <https://pgl.yoyo.org/adservers/> | 2026-11-11 | Peter Lowe's list, free to use with attribution |
| `lumen.txt` | authored for Lumen | — | CC0-1.0 (see `LICENSE.md`) |

uBlock Origin Lite's `urlhaus-full` ruleset (about 108k malware URLs, 12 MB) is intentionally not bundled: it protects against malware downloads rather than ads, and embedding it would add more runtime cost than it saves for this browser.

These files are unmodified copies redistributed as data for the built-in shield. Filter lists are third-party works; update them by re-downloading from the sources above.
