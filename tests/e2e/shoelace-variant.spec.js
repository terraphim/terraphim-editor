import { test, expect } from '@playwright/test';

test.describe('Shoelace Variant', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/example-shoelace.html');
    // Wait for WASM to initialize
    await page.waitForSelector('.markdown-input', { timeout: 10000 });
  });

  test('should load editor with initial content', async ({ page }) => {
    // Check that the editor container is present
    const container = page.locator('#editor-container');
    await expect(container).toBeVisible();

    // Check initial markdown content
    const textarea = page.locator('.markdown-input');
    const content = await textarea.inputValue();
    expect(content).toContain('Welcome to Markdown Editor');
    expect(content).toContain('Rust');
    expect(content).toContain('WebAssembly');
  });

  test('should render markdown preview', async ({ page }) => {
    // Check that preview is rendered
    const preview = page.locator('.markdown-preview');
    await expect(preview).toBeVisible();

    // Verify initial preview contains rendered HTML
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Welcome to Markdown Editor!</h1>');
    expect(previewHTML).toContain('<li>Rust</li>');
    expect(previewHTML).toContain('<li>WebAssembly</li>');
  });

  test('should update preview on input', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    // Clear and type new content
    await textarea.clear();
    await textarea.fill('# Test Heading\n\nThis is **bold** text.');

    // Wait a bit for update
    await page.waitForTimeout(100);

    // Check preview updated
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Test Heading</h1>');
    expect(previewHTML).toContain('<strong>bold</strong>');
  });

  test('should have toolbar buttons', async ({ page }) => {
    // Check toolbar is present
    const toolbar = page.locator('.toolbar');
    await expect(toolbar).toBeVisible();

    // Check formatting buttons exist
    const buttons = page.locator('#formatting-toolbar sl-button');
    const count = await buttons.count();
    expect(count).toBeGreaterThan(0);
  });

  test('should open help dialog', async ({ page }) => {
    const helpButton = page.locator('#show-help');
    await helpButton.click();

    // Wait for dialog to open
    await page.waitForTimeout(300);

    // Check dialog is visible
    const dialog = page.locator('.shortcuts-dialog');
    const isOpen = await dialog.evaluate((el) => el.hasAttribute('open'));
    expect(isOpen).toBe(true);

    // Check shortcuts are listed
    const shortcuts = page.locator('#shortcuts-list .shortcut-item');
    const shortcutCount = await shortcuts.count();
    expect(shortcutCount).toBeGreaterThan(0);
  });

  test('should apply bold formatting with toolbar button', async ({ page }) => {
    const textarea = page.locator('.markdown-input');

    // Clear and set content
    await textarea.clear();
    await textarea.fill('selected text');

    // Select the text
    await textarea.evaluate((el) => {
      el.setSelectionRange(0, 13);
    });

    // Click bold button (first button)
    const boldButton = page.locator('#formatting-toolbar sl-button').first();
    await boldButton.click();

    // Check text is wrapped
    const value = await textarea.inputValue();
    expect(value).toContain('**selected text**');
  });

  test('should have split panel for layout', async ({ page }) => {
    // Check split panel component exists
    const splitPanel = page.locator('sl-split-panel');
    await expect(splitPanel).toBeVisible();

    // Check both panels are present
    const startPanel = page.locator('sl-split-panel [slot="start"]');
    const endPanel = page.locator('sl-split-panel [slot="end"]');

    await expect(startPanel).toBeVisible();
    await expect(endPanel).toBeVisible();
  });

  test('should render code blocks correctly', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    // Type code block
    await textarea.clear();
    await textarea.fill('```javascript\nconst x = 42;\n```');

    await page.waitForTimeout(100);

    // Check code block is rendered
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<pre>');
    expect(previewHTML).toContain('<code');
    expect(previewHTML).toContain('const x = 42');
  });

  test('should render lists correctly', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    await textarea.clear();
    await textarea.fill('- Item 1\n- Item 2\n- Item 3');

    await page.waitForTimeout(100);

    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<ul>');
    expect(previewHTML).toContain('<li>Item 1</li>');
    expect(previewHTML).toContain('<li>Item 2</li>');
  });

  test('should handle special characters', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    await textarea.clear();
    await textarea.fill('Test & < > " \' characters');

    await page.waitForTimeout(100);

    // HTML should be properly escaped
    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('&amp;');
    expect(previewHTML).toContain('&lt;');
    expect(previewHTML).toContain('&gt;');
  });
});
