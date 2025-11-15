# CI Troubleshooting Guide

## E2E Test Timeout Issues

### Problem: Playwright Timeout Waiting for Dev Server

**Symptom:**
```
Error: Timed out waiting 120000ms from config.webServer.
```

**Root Cause:**
The Vite dev server was binding to `localhost` by default, while Playwright was configured to connect to `127.0.0.1`. In some CI environments (particularly containerized ones), these addresses may not resolve to the same location, causing connection timeouts.

**Solution:**
Explicitly set `host: '127.0.0.1'` in `vite.config.js`:

```javascript
server: {
  host: '127.0.0.1',  // Match Playwright config
  port: 8080,
  strictPort: true,
  fs: {
    allow: ['..']
  }
}
```

This ensures the server binds to the exact address Playwright is checking in `playwright.config.js`:

```javascript
webServer: {
  command: 'npm run dev',
  url: 'http://127.0.0.1:8080',
  // ...
}
```

### Verification

After the fix, the Vite startup log should show:
```
➜  Local:   http://127.0.0.1:8080/
```

Instead of:
```
➜  Local:   http://localhost:8080/
```

## Other Common CI Issues

### WASM Files Not Found

**Symptom:**
Tests fail because WASM module can't be loaded.

**Solution:**
Verify WASM files are being copied correctly:
```bash
test -f public/wasm/terraphim_editor_bg.wasm
test -f public/js/terraphim_editor.js
```

The build script in `vite.config.js` handles this automatically when `pkg/` directory exists.

### Browser-Specific Failures

Use `fail-fast: false` in GitHub Actions matrix to see all browser failures:

```yaml
strategy:
  fail-fast: false
  matrix:
    browser: [chromium, firefox, webkit]
```

### Debugging Tips

1. **Enable verbose logging** in `playwright.config.js`:
   ```javascript
   webServer: {
     stdout: 'pipe',
     stderr: 'pipe',
   }
   ```

2. **Increase timeout** for slow CI environments:
   ```javascript
   webServer: {
     timeout: 120 * 1000,  // 2 minutes
   }
   ```

3. **Run locally in CI mode**:
   ```bash
   CI=true npm run test:e2e
   ```

4. **Check specific browser**:
   ```bash
   npm run test:e2e:chromium
   npm run test:e2e:firefox
   npm run test:e2e:webkit
   ```

## Test Coverage

Current test suite:
- **49 total tests** (47 E2E + 2 Rust)
- **3 browser engines** (Chromium, Firefox, WebKit)
- **3 UI variants** (Shoelace, Vanilla, Web Awesome)
- **Dynamic loading** scenarios tested

## Related Files

- `.github/workflows/ci.yml` - CI pipeline configuration
- `playwright.config.js` - E2E test configuration
- `vite.config.js` - Dev server and build configuration
- `tests/e2e/` - E2E test suites
