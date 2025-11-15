import { test, expect } from '@playwright/test';

test.describe('Dynamic Script Loading', () => {
  test('should initialize editor when script is loaded after DOMContentLoaded', async ({ page }) => {
    // Navigate to a minimal page
    await page.setContent(`
      <!DOCTYPE html>
      <html>
      <head>
        <meta charset="UTF-8">
        <title>Dynamic Loading Test</title>
        <script src="/js/config.js"></script>
      </head>
      <body>
        <div id="editor-container"></div>
      </body>
      </html>
    `);

    // Wait for page to fully load
    await page.waitForLoadState('domcontentloaded');

    // Inject WASM initialization
    await page.evaluate(async () => {
      const init = await import('/js/terraphim_editor.js');
      const wasm = await init.default('/wasm/terraphim_editor_bg.wasm');
      wasm.run_with_style(wasm.EditorStyle.Vanilla);
    });

    // Dynamically load editor script AFTER DOMContentLoaded has fired
    await page.evaluate(() => {
      const script = document.createElement('script');
      script.src = '/js/editor-vanilla.js';
      document.body.appendChild(script);
    });

    // Wait for editor to initialize
    await page.waitForSelector('.markdown-input', { timeout: 5000 });

    // Verify editor is interactive
    const textarea = page.locator('.markdown-input');
    await expect(textarea).toBeVisible();

    // Test that toolbar exists
    const toolbar = page.locator('.toolbar');
    await expect(toolbar).toBeVisible();

    // Test that editor is functional
    await textarea.clear();
    await textarea.fill('# Dynamic Test');
    await page.waitForTimeout(100);

    const preview = page.locator('.markdown-preview');
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Dynamic Test</h1>');
  });

  test('should work with Shoelace variant when loaded dynamically', async ({ page }) => {
    await page.setContent(`
      <!DOCTYPE html>
      <html>
      <head>
        <meta charset="UTF-8">
        <title>Dynamic Shoelace Test</title>
        <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/themes/light.css" />
        <script type="module">
          import { setBasePath } from 'https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/utilities/base-path.js';
          setBasePath('https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/');
        </script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/split-panel/split-panel.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/button/button.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/button-group/button-group.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/icon/icon.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/tooltip/tooltip.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/dialog/dialog.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/badge/badge.js"></script>
        <script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/divider/divider.js"></script>
        <script src="/js/config.js"></script>
      </head>
      <body>
        <div id="editor-container"></div>
      </body>
      </html>
    `);

    await page.waitForLoadState('domcontentloaded');

    // Inject WASM initialization
    await page.evaluate(async () => {
      const init = await import('/js/terraphim_editor.js');
      const wasm = await init.default('/wasm/terraphim_editor_bg.wasm');
      wasm.run_with_style(wasm.EditorStyle.Shoelace);
    });

    // Dynamically load editor script
    await page.evaluate(() => {
      const script = document.createElement('script');
      script.src = '/js/editor.js';
      document.body.appendChild(script);
    });

    // Wait for editor to initialize
    await page.waitForSelector('.markdown-input', { timeout: 5000 });

    const textarea = page.locator('.markdown-input');
    await expect(textarea).toBeVisible();

    // Test functionality
    await textarea.clear();
    await textarea.fill('**Dynamically loaded**');
    await page.waitForTimeout(100);

    const preview = page.locator('.markdown-preview');
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<strong>Dynamically loaded</strong>');
  });

  test('multi-style switcher should work after switching styles', async ({ page }) => {
    await page.goto('/index-multistyle.html');
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Test initial Shoelace style
    let textarea = page.locator('.markdown-input');
    await textarea.clear();
    await textarea.fill('# Initial');
    await page.waitForTimeout(100);

    let preview = page.locator('.markdown-preview');
    let html = await preview.innerHTML();
    expect(html).toContain('<h1>Initial</h1>');

    // Switch to Vanilla
    await page.locator('#btn-vanilla').click();
    await page.waitForTimeout(1000);

    // Verify vanilla editor is interactive
    textarea = page.locator('.markdown-input');
    await textarea.clear();
    await textarea.fill('# After Switch');
    await page.waitForTimeout(100);

    preview = page.locator('.markdown-preview');
    html = await preview.innerHTML();
    expect(html).toContain('<h1>After Switch</h1>');

    // Verify toolbar works
    const toolbar = page.locator('.toolbar');
    await expect(toolbar).toBeVisible();

    const firstButton = toolbar.locator('.btn').first();
    await expect(firstButton).toBeVisible();
  });
});
