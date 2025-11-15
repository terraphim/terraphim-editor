# Testing Guide

This document describes the testing infrastructure for Terraphim Editor, including unit tests, integration tests, and end-to-end (E2E) tests.

## Table of Contents

- [Overview](#overview)
- [Test Types](#test-types)
- [Running Tests](#running-tests)
- [E2E Tests](#e2e-tests)
- [CI/CD Pipeline](#cicd-pipeline)
- [Writing Tests](#writing-tests)
- [Troubleshooting](#troubleshooting)

---

## Overview

Terraphim Editor uses a multi-layered testing strategy:

1. **Rust Unit Tests** - Test core markdown conversion logic
2. **WASM Integration Tests** - Test WASM bindings and browser integration
3. **E2E Tests** - Test all three UI variants (Shoelace, Vanilla, Web Awesome)

### Test Coverage

- ✅ Markdown rendering
- ✅ Live preview updates
- ✅ Toolbar functionality
- ✅ Keyboard shortcuts
- ✅ Dialog interactions
- ✅ Split panel resizing
- ✅ Multi-style switching
- ✅ Cross-browser compatibility

---

## Test Types

### 1. Rust Unit Tests

Located in `src/lib.rs` under `#[cfg(test)]` modules.

**What they test:**
- Markdown to HTML conversion
- Template rendering
- Error handling

**Technologies:**
- Rust's built-in test framework
- `cargo test`

### 2. WASM Integration Tests

Located in `tests/web.rs` and `benches/markdown_bench.rs`.

**What they test:**
- WASM module loading
- Browser API integration
- Performance benchmarks

**Technologies:**
- `wasm-bindgen-test`
- Chrome/Firefox headless browsers

### 3. End-to-End Tests

Located in `tests/e2e/*.spec.js`.

**What they test:**
- Complete user workflows
- UI interactions across all three styles
- Cross-browser compatibility
- Responsive design

**Technologies:**
- Playwright
- Chromium, Firefox, WebKit

---

## Running Tests

### Prerequisites

```bash
# Install Rust and WASM target
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Install Node.js dependencies
npm install

# Install Playwright browsers
npx playwright install
```

### Quick Test Commands

```bash
# Run all tests (Rust + E2E)
npm test

# Run only Rust tests
npm run test:rust
# or
cargo test

# Run only E2E tests
npm run test:e2e

# Run E2E tests with UI (interactive mode)
npm run test:e2e:ui

# Run E2E tests in headed mode (see browser)
npm run test:e2e:headed

# Run E2E tests in specific browser
npm run test:e2e:chromium
npm run test:e2e:firefox
npm run test:e2e:webkit
```

### Detailed Test Commands

#### Rust Tests

```bash
# Run all Rust tests
cargo test --all-targets

# Run specific test
cargo test test_markdown_conversion

# Run tests with output
cargo test -- --nocapture

# Run tests with specific features
cargo test --features "feature-name"
```

#### WASM Tests

```bash
# Run WASM tests in Chrome
wasm-pack test --chrome

# Run WASM tests in Firefox
wasm-pack test --firefox

# Run WASM tests in headless mode
wasm-pack test --headless --chrome
```

#### E2E Tests

```bash
# Run all E2E tests
npx playwright test

# Run specific test file
npx playwright test tests/e2e/shoelace-variant.spec.js

# Run specific test by name
npx playwright test -g "should render markdown"

# Debug tests
npx playwright test --debug

# Generate test report
npx playwright show-report
```

---

## E2E Tests

### Test Structure

```
tests/e2e/
├── shoelace-variant.spec.js      # Tests for Shoelace style
├── vanilla-variant.spec.js       # Tests for Vanilla style
├── webawesome-variant.spec.js    # Tests for Web Awesome style
└── multi-style-switcher.spec.js  # Tests for style switching
```

### Test Coverage by Variant

#### Shoelace Variant Tests (14 tests)

- ✅ Initial content loading
- ✅ Markdown preview rendering
- ✅ Live preview updates
- ✅ Toolbar button interactions
- ✅ Help dialog opening
- ✅ Bold formatting with toolbar
- ✅ Split panel layout
- ✅ Code block rendering
- ✅ List rendering
- ✅ Special character handling

**File:** `tests/e2e/shoelace-variant.spec.js`

#### Vanilla Variant Tests (10 tests)

- ✅ Zero external dependencies verification
- ✅ Vanilla UI rendering
- ✅ Markdown rendering
- ✅ Resizable split panel
- ✅ Vanilla dialog functionality
- ✅ Dialog close button
- ✅ Text-based toolbar icons
- ✅ Tooltip display
- ✅ Formatting with vanilla buttons
- ✅ Responsive layout

**File:** `tests/e2e/vanilla-variant.spec.js`

#### Web Awesome Variant Tests (9 tests)

- ✅ Web Awesome component loading
- ✅ Markdown preview rendering
- ✅ Live preview updates
- ✅ Toolbar with wa- components
- ✅ Help dialog
- ✅ Split panel layout
- ✅ Formatting functionality
- ✅ Complex markdown rendering
- ✅ Setup note display

**File:** `tests/e2e/webawesome-variant.spec.js`

#### Multi-Style Switcher Tests (11 tests)

- ✅ Default Shoelace style loading
- ✅ Switch to Vanilla style
- ✅ Switch to Web Awesome style
- ✅ Info panel updates on switch
- ✅ Editor functionality after switching
- ✅ Header styling
- ✅ Gradient background
- ✅ All style options display
- ✅ Feature display for each style
- ✅ Rapid style switching
- ✅ Responsive mobile layout

**File:** `tests/e2e/multi-style-switcher.spec.js`

### Total Test Count

- **44 E2E tests** across 4 test suites
- **3 browsers** (Chromium, Firefox, WebKit)
- **132 total test runs** in full CI pipeline

---

## CI/CD Pipeline

### GitHub Actions Workflow

Location: `.github/workflows/ci.yml`

### Pipeline Stages

```
┌─────────────────┐
│  Rust Tests     │  ← Run first
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Build WASM     │  ← Upload artifacts
└────────┬────────┘
         │
         ├──────────────┬──────────────┐
         ▼              ▼              ▼
    ┌────────┐    ┌────────┐    ┌────────┐
    │Chromium│    │Firefox │    │ WebKit │  ← E2E tests in parallel
    └────────┘    └────────┘    └────────┘
         │              │              │
         └──────┬───────┴──────────────┘
                ▼
      ┌──────────────────┐
      │ Build Package    │
      └──────────────────┘
                │
                ▼
      ┌──────────────────┐
      │ All Tests Pass ✓ │
      └──────────────────┘
```

### CI Jobs

1. **rust-tests**
   - Check code formatting (`cargo fmt`)
   - Run linter (`cargo clippy`)
   - Run unit tests (`cargo test`)

2. **build-wasm**
   - Install wasm-pack
   - Build WASM package
   - Upload artifacts for subsequent jobs

3. **e2e-tests** (Matrix: chromium, firefox, webkit)
   - Download WASM artifacts
   - Install Playwright browsers
   - Run E2E tests
   - Upload test reports

4. **build-package**
   - Build distribution package
   - Verify all formats (ESM, UMD, IIFE)
   - Upload dist artifacts

5. **all-tests-passed**
   - Final check that all jobs succeeded

### Artifacts

The CI pipeline uploads:
- WASM build artifacts
- Playwright test reports (for each browser)
- Distribution package

**Retention:** 30 days

### Triggering CI

CI runs on:
- Push to `main` branch
- Push to any `claude/**` branch
- Pull requests to `main`

---

## Writing Tests

### E2E Test Template

```javascript
import { test, expect } from '@playwright/test';

test.describe('Feature Name', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/example-page.html');
    await page.waitForSelector('.markdown-input', { timeout: 10000 });
  });

  test('should do something', async ({ page }) => {
    const element = page.locator('.some-element');
    await expect(element).toBeVisible();

    // Interact with element
    await element.click();

    // Assert result
    const result = await element.textContent();
    expect(result).toContain('expected text');
  });
});
```

### Best Practices

1. **Wait for WASM initialization**
   ```javascript
   await page.waitForSelector('.markdown-input', { timeout: 10000 });
   ```

2. **Use specific selectors**
   ```javascript
   // Good
   page.locator('#editor-container .markdown-input')

   // Avoid
   page.locator('textarea')
   ```

3. **Add appropriate timeouts**
   ```javascript
   await page.waitForTimeout(100); // For debounced updates
   ```

4. **Test user workflows, not implementation**
   ```javascript
   // Good: Test what user sees/does
   await page.locator('#show-help').click();
   await expect(page.locator('.shortcuts-dialog')).toBeVisible();

   // Avoid: Testing internal state
   await page.evaluate(() => window.internalState);
   ```

5. **Clean up state between tests**
   ```javascript
   test.beforeEach(async ({ page }) => {
     // Reset to clean state
   });
   ```

### Common Patterns

#### Testing Markdown Rendering

```javascript
test('should render markdown', async ({ page }) => {
  const textarea = page.locator('.markdown-input');
  const preview = page.locator('.markdown-preview');

  await textarea.clear();
  await textarea.fill('# Heading\n\n**bold**');
  await page.waitForTimeout(100);

  const html = await preview.innerHTML();
  expect(html).toContain('<h1>Heading</h1>');
  expect(html).toContain('<strong>bold</strong>');
});
```

#### Testing Button Clicks

```javascript
test('should apply formatting', async ({ page }) => {
  const textarea = page.locator('.markdown-input');

  await textarea.fill('text');
  await textarea.evaluate(el => el.setSelectionRange(0, 4));

  await page.locator('#bold-button').click();

  const value = await textarea.inputValue();
  expect(value).toBe('**text**');
});
```

#### Testing Dialogs

```javascript
test('should open and close dialog', async ({ page }) => {
  await page.locator('#open-dialog').click();

  const dialog = page.locator('.dialog');
  await expect(dialog).toBeVisible();

  await page.locator('#close-dialog').click();
  await expect(dialog).not.toBeVisible();
});
```

---

## Troubleshooting

### Common Issues

#### 1. WASM not loading in tests

**Symptom:** Timeout waiting for `.markdown-input`

**Solution:**
```bash
# Rebuild WASM
wasm-pack build --target web --out-dir pkg

# Copy to public directory
cp pkg/terraphim_editor_bg.wasm public/wasm/
cp pkg/terraphim_editor.js public/js/
```

#### 2. Tests fail in CI but pass locally

**Symptom:** Tests pass on `npm run test:e2e` but fail in GitHub Actions

**Possible causes:**
- Missing WASM files in artifacts
- Different Node.js versions
- Race conditions (increase timeouts)

**Solution:**
```yaml
# Check CI logs for artifact download
# Increase timeouts in tests
await page.waitForTimeout(500); // Instead of 100
```

#### 3. Browser not installed

**Symptom:** `browserType.launch: Executable doesn't exist`

**Solution:**
```bash
npx playwright install chromium firefox webkit
```

#### 4. Port already in use

**Symptom:** `Error: Port 8080 is already in use`

**Solution:**
```bash
# Kill process on port 8080
lsof -ti:8080 | xargs kill -9

# Or change port in playwright.config.js
baseURL: 'http://127.0.0.1:3000'
```

#### 5. Tests are flaky

**Symptom:** Tests sometimes pass, sometimes fail

**Solutions:**
- Add explicit waits: `await page.waitForSelector()`
- Increase timeouts for async operations
- Use `waitForLoadState`: `await page.waitForLoadState('networkidle')`
- Disable parallelism: `workers: 1` in config

### Debug Mode

Run tests in debug mode to step through them:

```bash
# Open Playwright Inspector
npx playwright test --debug

# Debug specific test
npx playwright test --debug tests/e2e/shoelace-variant.spec.js

# Open in UI mode (recommended)
npx playwright test --ui
```

### Viewing Test Reports

```bash
# Generate and open HTML report
npx playwright show-report

# Reports are in: playwright-report/index.html
```

### Test Artifacts

Failed test artifacts (screenshots, videos, traces):
- Location: `test-results/`
- Screenshots: Automatically captured on failure
- Videos: Only in CI (to save space locally)
- Traces: Captured on retry

---

## Performance

### Test Execution Times (Approximate)

- Rust tests: ~5 seconds
- WASM build: ~10 seconds
- E2E tests (all browsers): ~2-3 minutes
- Full CI pipeline: ~5-7 minutes

### Optimization Tips

1. **Run specific browsers during development**
   ```bash
   npm run test:e2e:chromium
   ```

2. **Use test filtering**
   ```bash
   npx playwright test -g "should render"
   ```

3. **Parallel execution**
   ```javascript
   // In playwright.config.js
   workers: 4 // Run 4 tests in parallel
   ```

---

## Contributing

When adding new features:

1. ✅ Add Rust unit tests for core logic
2. ✅ Add E2E tests for UI interactions
3. ✅ Test across all three style variants if applicable
4. ✅ Ensure CI passes before submitting PR
5. ✅ Update this documentation if adding new test patterns

---

## Resources

- [Playwright Documentation](https://playwright.dev)
- [Rust Testing Documentation](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [wasm-bindgen Testing](https://rustwasm.github.io/wasm-bindgen/wasm-bindgen-test/index.html)
- [GitHub Actions Documentation](https://docs.github.com/en/actions)

---

## Test Metrics

Current test coverage:

| Component | Tests | Status |
|-----------|-------|--------|
| Rust Core | 2 | ✅ |
| Shoelace Variant | 14 | ✅ |
| Vanilla Variant | 10 | ✅ |
| Web Awesome Variant | 9 | ✅ |
| Multi-Style Switcher | 11 | ✅ |
| **Total** | **46** | **✅** |

Last updated: 2025-01-15
