# Summary: tests/web_bench.rs

## Purpose
Typing-latency benchmark in the browser (5,000-word document); asserts the debounced surface is no slower than the textarea baseline.

## Key points
- Async and yielding so it does not starve the webdriver poll.
