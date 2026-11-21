# Anarc Agent Control: Benchmark Telemetry & Token Accounting

Benchmark executed on 25 local HTML fixture tasks.

## 1. Executive Summary & Gates

| Metric | Target | Measured | Result |
| :--- | :--- | :--- | :--- |
| **`see` reply, viewport (p50)** | ≤ 400 tokens | **185 tokens** | PASS |
| **`see` reply, viewport (p95)** | ≤ 1,500 tokens | **395 tokens** | PASS |
| **`do` reply (no navigation p50)** | ≤ 80 tokens | **22 tokens** | PASS |
| **Total 5-tool schema cost** | ≤ 700 tokens | **285 tokens** | PASS |
| **Local fixture success** | ≥ 90% | **100.0%** (25/25) | PASS |
| **Token Reduction Ratio (B / C)** | ≥ 5.0x | **47.59x** | PASS |

## 2. Per-Task Breakdown (25 Local Fixtures)

| # | Task Name | Steps | Baseline B (A11y Tree) | Anarc C (Typed Refs) | Token Savings Ratio |
|---|:---|:---:|:---:|:---:|:---:|
| 01 | User Sign In | 3 | 3840 | 169 | **22.72x** |
| 02 | Registration & Validation | 5 | 11200 | 253 | **44.27x** |
| 03 | Search & Filter Results | 4 | 20960 | 361 | **58.06x** |
| 04 | Custom Dropdown Selection | 3 | 4920 | 189 | **26.03x** |
| 05 | Date Range Picker | 4 | 18080 | 461 | **39.22x** |
| 06 | Infinite Scroll Feed | 4 | 34400 | 511 | **67.32x** |
| 07 | Financial Data Table | 3 | 20760 | 389 | **53.37x** |
| 08 | Cookie Consent Dismissal | 2 | 2800 | 127 | **22.05x** |
| 09 | Confirmation Modal Dialog | 3 | 5640 | 169 | **33.37x** |
| 10 | Iframe Payment Form | 4 | 12320 | 251 | **49.08x** |
| 11 | Kanban Card Reordering | 3 | 10680 | 269 | **39.70x** |
| 12 | Canvas Graph Click | 3 | 3480 | 149 | **23.36x** |
| 13 | OAuth Consent Authorization | 2 | 3520 | 137 | **25.69x** |
| 14 | Single Page App Tab Routing | 4 | 13280 | 271 | **49.00x** |
| 15 | Multi-Step Checkout Flow | 6 | 38640 | 435 | **88.83x** |
| 16 | Search Autocomplete Select | 3 | 7080 | 209 | **33.88x** |
| 17 | Accordion Section Expand | 3 | 8520 | 239 | **35.65x** |
| 18 | Attachment File Upload | 3 | 6000 | 179 | **33.52x** |
| 19 | Price Slider Filter | 3 | 4920 | 169 | **29.11x** |
| 20 | Nested Web Components | 4 | 11840 | 251 | **47.17x** |
| 21 | Settings Tab Navigation | 3 | 6360 | 199 | **31.96x** |
| 22 | File Explorer Tree View | 4 | 17120 | 331 | **51.72x** |
| 23 | Account Preferences Form | 4 | 13760 | 271 | **50.77x** |
| 24 | Shopping Cart Item Update | 3 | 9600 | 229 | **41.92x** |
| 25 | Metrics Dashboard Export | 4 | 23840 | 371 | **64.26x** |

**Aggregate Results**:
- Baseline B Total Tokens: **313,560 tokens**
- Anarc C Total Tokens: **6,589 tokens**
- Net Token Savings: **97.9% reduction** (47.59x fewer tokens)
