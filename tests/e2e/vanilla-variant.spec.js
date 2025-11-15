import { test, expect } from '@playwright/test';

test.describe('Vanilla Variant', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/example-vanilla.html');
    // Wait for WASM to initialize
    await page.waitForSelector('.markdown-input', { timeout: 10000 });
  });

  test('should load editor with no external dependencies', async ({ page }) => {
    // Check that no Shoelace/Web Awesome scripts are loaded
    const scripts = await page.evaluate(() => {
      return Array.from(document.querySelectorAll('script'))
        .map(s => s.src)
        .filter(src => src.includes('shoelace') || src.includes('webawesome'));
    });
    expect(scripts.length).toBe(0);
  });

  test('should have fully functional vanilla UI', async ({ page }) => {
    const container = page.locator('#editor-container');
    await expect(container).toBeVisible();

    // Check vanilla-specific classes
    const splitPanel = page.locator('.split-panel');
    await expect(splitPanel).toBeVisible();
  });

  test('should render markdown correctly', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    await textarea.clear();
    await textarea.fill('# Vanilla Test\n\nThis is **bold** and *italic*.');

    await page.waitForTimeout(100);

    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Vanilla Test</h1>');
    expect(previewHTML).toContain('<strong>bold</strong>');
    expect(previewHTML).toContain('<em>italic</em>');
  });

  test('should have resizable split panel', async ({ page }) => {
    const divider = page.locator('#panel-divider');
    await expect(divider).toBeVisible();

    // Check cursor style
    const cursor = await divider.evaluate((el) => {
      return window.getComputedStyle(el).cursor;
    });
    expect(cursor).toBe('col-resize');
  });

  test('should open vanilla dialog on help button', async ({ page }) => {
    const helpButton = page.locator('#show-help');
    await helpButton.click();

    await page.waitForTimeout(100);

    // Check vanilla dialog is visible
    const dialog = page.locator('.shortcuts-dialog');
    const isVisible = await dialog.evaluate((el) => {
      return el.classList.contains('open');
    });
    expect(isVisible).toBe(true);
  });

  test('should close dialog on close button', async ({ page }) => {
    const helpButton = page.locator('#show-help');
    await helpButton.click();

    await page.waitForTimeout(100);

    const closeButton = page.locator('#close-help');
    await closeButton.click();

    await page.waitForTimeout(100);

    const dialog = page.locator('.shortcuts-dialog');
    const isVisible = await dialog.evaluate((el) => {
      return el.classList.contains('open');
    });
    expect(isVisible).toBe(false);
  });

  test('should have toolbar buttons with text icons', async ({ page }) => {
    const toolbar = page.locator('#formatting-toolbar');
    await expect(toolbar).toBeVisible();

    // Vanilla buttons should have text-based icons
    const firstButton = toolbar.locator('.btn').first();
    const iconText = await firstButton.locator('.icon').textContent();
    expect(iconText).toBeTruthy();
    expect(iconText.length).toBeGreaterThan(0);
  });

  test('should show tooltips on hover', async ({ page }) => {
    const firstButton = page.locator('#formatting-toolbar .tooltip').first();

    // Hover over button
    await firstButton.hover();

    await page.waitForTimeout(100);

    // Check tooltip is visible
    const tooltip = firstButton.locator('.tooltip-text');
    const isVisible = await tooltip.isVisible();
    expect(isVisible).toBe(true);
  });

  test('should apply formatting with vanilla buttons', async ({ page }) => {
    const textarea = page.locator('.markdown-input');

    await textarea.clear();
    await textarea.fill('text to format');

    await textarea.evaluate((el) => {
      el.setSelectionRange(0, 14);
    });

    // Click first formatting button
    const firstButton = page.locator('#formatting-toolbar .btn').first();
    await firstButton.click();

    const value = await textarea.inputValue();
    expect(value).toContain('**text to format**');
  });

  test('should have proper styling without external libraries', async ({ page }) => {
    // Check that styles are applied
    const toolbar = page.locator('.toolbar');
    const bgColor = await toolbar.evaluate((el) => {
      return window.getComputedStyle(el).backgroundColor;
    });

    // Should have a background color set
    expect(bgColor).not.toBe('rgba(0, 0, 0, 0)');
    expect(bgColor).not.toBe('transparent');
  });

  test('should handle responsive layout', async ({ page }) => {
    // Set mobile viewport
    await page.setViewportSize({ width: 375, height: 667 });

    await page.waitForTimeout(100);

    // Editor should still be visible and functional
    const container = page.locator('#editor-container');
    await expect(container).toBeVisible();

    const textarea = page.locator('.markdown-input');
    await expect(textarea).toBeVisible();
  });
});
