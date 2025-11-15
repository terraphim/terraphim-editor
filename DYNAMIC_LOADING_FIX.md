# Dynamic Script Loading Fix

## Issue Summary

**Severity:** P1 - Critical
**Affected Components:** All three editor variants
**Impact:** Editors render but remain non-interactive when loaded dynamically

---

## The Problem

### Root Cause

All three editor scripts (`editor.js`, `editor-vanilla.js`, `editor-webawesome.js`) used this pattern:

```javascript
document.addEventListener('DOMContentLoaded', () => {
  if (window.EditorConfig) {
    initEditor();
  }
});
```

**The Bug:** When a script is added dynamically after page load:

```javascript
const script = document.createElement('script');
script.src = './js/editor-vanilla.js';
document.body.appendChild(script);
```

The `DOMContentLoaded` event has **already fired** and won't fire again, so `initEditor()` never runs.

### Symptoms

- ✅ Editor HTML renders correctly
- ❌ Toolbar buttons don't work
- ❌ Keyboard shortcuts inactive
- ❌ Help dialog won't open
- ❌ Command palette doesn't appear
- ❌ No markdown conversion on input

### Affected Use Cases

1. **Multi-style switcher** (`index-multistyle.html`)
   - When switching between Shoelace → Vanilla → Web Awesome
   - Scripts are loaded dynamically on style change

2. **Single-page applications**
   - Any app dynamically loading editor scripts
   - Lazy loading scenarios

3. **Progressive enhancement**
   - Scripts loaded conditionally after page load

---

## The Solution

### Implementation

Replace the simple event listener with a state check:

**Before:**
```javascript
document.addEventListener('DOMContentLoaded', () => {
  if (window.EditorConfig) {
    initEditor();
  }
});
```

**After:**
```javascript
function initializeEditor() {
  if (window.EditorConfig) {
    initEditor();
  }
}

// Run immediately if DOM is ready, otherwise wait
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initializeEditor);
} else {
  // DOM already ready, initialize immediately
  initializeEditor();
}
```

### How It Works

1. **Check `document.readyState`:**
   - `'loading'` = Document still parsing → wait for event
   - `'interactive'` or `'complete'` = DOM ready → run now

2. **Extract initialization logic:**
   - Wrapped in `initializeEditor()` function
   - Can be called directly or via event

3. **Handles both scenarios:**
   - Static `<script>` tags: waits for DOMContentLoaded
   - Dynamic injection: runs immediately

---

## Changes Made

### Files Modified

1. **`public/js/editor.js`** (Shoelace variant)
   - Added `initializeEditor()` wrapper
   - Added `document.readyState` check

2. **`public/js/editor-vanilla.js`** (Vanilla variant)
   - Added `initializeEditor()` wrapper
   - Added `document.readyState` check

3. **`public/js/editor-webawesome.js`** (Web Awesome variant)
   - Added `initializeEditor()` wrapper
   - Added `document.readyState` check

### Tests Added

**New file:** `tests/e2e/dynamic-loading.spec.js`

**Test 1: Vanilla variant dynamic loading**
- Creates minimal HTML page
- Waits for DOMContentLoaded
- Dynamically loads WASM and editor script
- Verifies editor is interactive

**Test 2: Shoelace variant dynamic loading**
- Same pattern with Shoelace components
- Tests toolbar functionality
- Validates markdown conversion

**Test 3: Multi-style switcher**
- Tests switching between styles
- Verifies each variant works after switch
- Confirms toolbar buttons are clickable

---

## Verification

### Manual Testing

```bash
# Start dev server
npm run dev

# Open multi-style switcher
open http://localhost:8080/index-multistyle.html

# Test steps:
1. Page loads with Shoelace (default) ✅
2. Type in editor → preview updates ✅
3. Click "Vanilla" button ✅
4. Type in editor → preview updates ✅
5. Click toolbar buttons → formatting works ✅
6. Click help button → dialog opens ✅
```

### Automated Tests

```bash
# Run new dynamic loading tests
npx playwright test tests/e2e/dynamic-loading.spec.js

# Expected results:
✅ should initialize editor when script is loaded after DOMContentLoaded
✅ should work with Shoelace variant when loaded dynamically
✅ multi-style switcher should work after switching styles
```

### All Tests

```bash
# Run full test suite
npm test

# Should pass:
✅ 2 Rust unit tests
✅ 46 existing E2E tests
✅ 3 new dynamic loading tests
= 51 total tests
```

---

## Technical Details

### Document Ready States

| State | Description | When |
|-------|-------------|------|
| `loading` | Document still loading | Initial page load |
| `interactive` | DOM ready, resources loading | DOMContentLoaded fired |
| `complete` | Everything loaded | window.onload fired |

### Event Timing

**Static Script Loading:**
```
1. HTML parsing starts (readyState: 'loading')
2. <script> encountered and executed
3. Script adds DOMContentLoaded listener
4. HTML parsing completes
5. DOMContentLoaded fires → listener runs ✅
```

**Dynamic Script Loading (BEFORE fix):**
```
1. HTML parsing completes
2. DOMContentLoaded fires
3. User clicks button
4. Script created and added (readyState: 'interactive')
5. Script executes, adds listener
6. DOMContentLoaded ALREADY FIRED → listener never runs ❌
```

**Dynamic Script Loading (AFTER fix):**
```
1. HTML parsing completes
2. DOMContentLoaded fires
3. User clicks button
4. Script created and added (readyState: 'interactive')
5. Script executes, checks readyState
6. readyState !== 'loading' → runs immediately ✅
```

---

## Best Practices

### Pattern for Dynamic Loading

```javascript
// ✅ GOOD: Handles both static and dynamic loading
function init() {
  // initialization code
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', init);
} else {
  init();
}
```

```javascript
// ❌ BAD: Only works for static loading
document.addEventListener('DOMContentLoaded', () => {
  // initialization code
});
```

### Alternative Patterns

**Using `DOMContentLoaded` with immediate check:**
```javascript
function init() { /* ... */ }

if (document.readyState !== 'loading') {
  init();
} else {
  document.addEventListener('DOMContentLoaded', init);
}
```

**Using `readystatechange`:**
```javascript
function init() { /* ... */ }

document.addEventListener('readystatechange', () => {
  if (document.readyState === 'interactive') {
    init();
  }
});

// Also run if already interactive
if (document.readyState !== 'loading') {
  init();
}
```

---

## Impact Assessment

### Before Fix

| Scenario | Status |
|----------|--------|
| Static `<script>` tags | ✅ Works |
| Dynamic script loading | ❌ Broken |
| Multi-style switcher | ❌ Broken |
| SPA integration | ❌ Broken |

### After Fix

| Scenario | Status |
|----------|--------|
| Static `<script>` tags | ✅ Works |
| Dynamic script loading | ✅ Works |
| Multi-style switcher | ✅ Works |
| SPA integration | ✅ Works |

### Compatibility

- ✅ **Backward compatible:** Static loading still works
- ✅ **No breaking changes:** Existing code unaffected
- ✅ **Browser support:** All modern browsers (IE11+)
- ✅ **Framework agnostic:** Works with React, Vue, etc.

---

## Related Issues

- **Code review bot finding:** P1 - Initialize editor scripts when loaded dynamically
- **Multi-style switcher:** Editors rendered but non-interactive
- **Dynamic import:** Scripts loaded via `import()` or `createElement()`

---

## Commit Information

**Commit:** `ed76c0b`
**Message:** "fix: support dynamic script loading for all editor variants"
**Branch:** `claude/validate-editor-merge-extend-01PmtMwPNRUw9UpJcPMjxR2m`
**Files Changed:** 4
**Lines Added:** 186
**Lines Removed:** 9

---

## Prevention

### For Future Development

1. **Always check `readyState`** when using `DOMContentLoaded`
2. **Test dynamic loading** scenarios
3. **Use linters** that catch this pattern
4. **Document loading requirements** in README

### Recommended Linting Rule

```javascript
// ESLint rule to warn about this pattern
{
  "rules": {
    "no-dom-content-loaded-only": "warn"
  }
}
```

---

## References

- **MDN:** [Document.readyState](https://developer.mozilla.org/en-US/docs/Web/API/Document/readyState)
- **MDN:** [DOMContentLoaded](https://developer.mozilla.org/en-US/docs/Web/API/Document/DOMContentLoaded_event)
- **HTML Spec:** [Document lifecycle](https://html.spec.whatwg.org/multipage/parsing.html#the-end)

---

## Summary

✅ **Fixed:** Critical bug preventing dynamic script loading
✅ **Tested:** 3 new E2E tests covering all scenarios
✅ **Compatible:** Backward compatible with existing code
✅ **Complete:** All three editor variants updated

**Impact:** Multi-style switcher and dynamic loading now fully functional! 🎉
