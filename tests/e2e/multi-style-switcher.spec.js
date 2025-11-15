import { test, expect } from '@playwright/test';

test.describe('Multi-Style Switcher', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/index-multistyle.html');
    // Wait for WASM to initialize
    await page.waitForSelector('#editor-container', { timeout: 10000 });
  });

  test('should load with Shoelace style by default', async ({ page }) => {
    // Wait for editor to load
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Check that Shoelace button is active
    const shoelaceBtn = page.locator('#btn-shoelace');
    const variant = await shoelaceBtn.getAttribute('variant');
    expect(variant).toBe('primary');

    // Check Shoelace components are loaded
    const splitPanel = page.locator('sl-split-panel');
    await expect(splitPanel).toBeVisible();
  });

  test('should switch to Vanilla style', async ({ page }) => {
    // Wait for initial load
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Click Vanilla button
    const vanillaBtn = page.locator('#btn-vanilla');
    await vanillaBtn.click();

    // Wait for switch
    await page.waitForTimeout(500);

    // Check Vanilla button is active
    const variant = await vanillaBtn.getAttribute('variant');
    expect(variant).toBe('primary');

    // Check vanilla split panel is loaded
    const splitPanel = page.locator('.split-panel');
    await expect(splitPanel).toBeVisible();

    // Check editor still works
    const textarea = page.locator('.markdown-input');
    await expect(textarea).toBeVisible();
  });

  test('should switch to Web Awesome style', async ({ page }) => {
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Click Web Awesome button
    const webAwesomeBtn = page.locator('#btn-webawesome');
    await webAwesomeBtn.click();

    await page.waitForTimeout(500);

    // Check Web Awesome button is active
    const variant = await webAwesomeBtn.getAttribute('variant');
    expect(variant).toBe('primary');

    // Check editor loaded
    const textarea = page.locator('.markdown-input');
    await expect(textarea).toBeVisible();
  });

  test('should update info panel when switching styles', async ({ page }) => {
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    const infoPanel = page.locator('#style-info');

    // Check initial info
    let infoText = await infoPanel.textContent();
    expect(infoText).toContain('Shoelace');

    // Switch to Vanilla
    await page.locator('#btn-vanilla').click();
    await page.waitForTimeout(300);

    infoText = await infoPanel.textContent();
    expect(infoText).toContain('Pure HTML/CSS');
    expect(infoText).toContain('Zero dependencies');

    // Switch to Web Awesome
    await page.locator('#btn-webawesome').click();
    await page.waitForTimeout(300);

    infoText = await infoPanel.textContent();
    expect(infoText).toContain('Web Awesome');
  });

  test('should maintain editor functionality after switching', async ({ page }) => {
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    // Test with Shoelace
    await textarea.clear();
    await textarea.fill('# Shoelace Test');
    await page.waitForTimeout(100);
    let previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Shoelace Test</h1>');

    // Switch to Vanilla
    await page.locator('#btn-vanilla').click();
    await page.waitForTimeout(500);

    // Test with Vanilla
    await textarea.clear();
    await textarea.fill('# Vanilla Test');
    await page.waitForTimeout(100);
    previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Vanilla Test</h1>');

    // Switch to Web Awesome
    await page.locator('#btn-webawesome').click();
    await page.waitForTimeout(500);

    // Test with Web Awesome
    await textarea.clear();
    await textarea.fill('# Web Awesome Test');
    await page.waitForTimeout(100);
    previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Web Awesome Test</h1>');
  });

  test('should have proper styling for header', async ({ page }) => {
    const header = page.locator('.header');
    await expect(header).toBeVisible();

    const bgColor = await header.evaluate((el) => {
      return window.getComputedStyle(el).backgroundColor;
    });

    // Should not be transparent
    expect(bgColor).not.toBe('rgba(0, 0, 0, 0)');
  });

  test('should have gradient background on body', async ({ page }) => {
    const bodyBg = await page.evaluate(() => {
      return window.getComputedStyle(document.body).background;
    });

    // Should have gradient
    expect(bodyBg).toContain('gradient');
  });

  test('should display all three style options', async ({ page }) => {
    const shoelaceBtn = page.locator('#btn-shoelace');
    const vanillaBtn = page.locator('#btn-vanilla');
    const webAwesomeBtn = page.locator('#btn-webawesome');

    await expect(shoelaceBtn).toBeVisible();
    await expect(vanillaBtn).toBeVisible();
    await expect(webAwesomeBtn).toBeVisible();

    // Check button text
    expect(await shoelaceBtn.textContent()).toContain('Shoelace');
    expect(await vanillaBtn.textContent()).toContain('Vanilla');
    expect(await webAwesomeBtn.textContent()).toContain('Web Awesome');
  });

  test('should show features for each style', async ({ page }) => {
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Each style should show features
    const features = page.locator('.feature');
    const count = await features.count();
    expect(count).toBe(3); // Default Shoelace shows 3 features
  });

  test('should handle rapid style switching', async ({ page }) => {
    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Rapidly switch between styles
    await page.locator('#btn-vanilla').click();
    await page.waitForTimeout(100);
    await page.locator('#btn-webawesome').click();
    await page.waitForTimeout(100);
    await page.locator('#btn-shoelace').click();
    await page.waitForTimeout(500);

    // Editor should still work
    const textarea = page.locator('.markdown-input');
    await expect(textarea).toBeVisible();

    await textarea.clear();
    await textarea.fill('# Rapid Switch Test');
    await page.waitForTimeout(100);

    const preview = page.locator('.markdown-preview');
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Rapid Switch Test</h1>');
  });

  test('should be responsive on mobile', async ({ page }) => {
    // Set mobile viewport
    await page.setViewportSize({ width: 375, height: 667 });

    await page.waitForSelector('.markdown-input', { timeout: 10000 });

    // Everything should still be visible
    const header = page.locator('.header');
    const styleSelector = page.locator('.style-selector');
    const editorContainer = page.locator('#editor-container');

    await expect(header).toBeVisible();
    await expect(styleSelector).toBeVisible();
    await expect(editorContainer).toBeVisible();
  });
});
