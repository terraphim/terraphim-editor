import { test, expect } from '@playwright/test';

test.describe('Web Awesome Variant', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/example-webawesome.html');
    // Wait for WASM to initialize
    await page.waitForSelector('.markdown-input', { timeout: 10000 });
  });

  test('should load editor with Web Awesome components', async ({ page }) => {
    const container = page.locator('#editor-container');
    await expect(container).toBeVisible();

    // Note: Currently using Shoelace as fallback for Web Awesome
    // This will work with actual Web Awesome CDN when configured
  });

  test('should render markdown preview', async ({ page }) => {
    const preview = page.locator('.markdown-preview');
    await expect(preview).toBeVisible();

    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Welcome to Markdown Editor!</h1>');
  });

  test('should update preview on input', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    await textarea.clear();
    await textarea.fill('# Web Awesome Test\n\n**Bold** and *italic* text.');

    await page.waitForTimeout(100);

    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Web Awesome Test</h1>');
    expect(previewHTML).toContain('<strong>Bold</strong>');
    expect(previewHTML).toContain('<em>italic</em>');
  });

  test('should have toolbar with components', async ({ page }) => {
    const toolbar = page.locator('.toolbar');
    await expect(toolbar).toBeVisible();

    // Web Awesome uses wa- prefix (or sl- as fallback)
    const buttons = page.locator('#formatting-toolbar button, #formatting-toolbar sl-button, #formatting-toolbar wa-button');
    const count = await buttons.count();
    expect(count).toBeGreaterThan(0);
  });

  test('should open help dialog', async ({ page }) => {
    const helpButton = page.locator('#show-help');
    await helpButton.click();

    await page.waitForTimeout(300);

    const dialog = page.locator('.shortcuts-dialog');

    // Check if using Web Awesome or Shoelace fallback
    const isOpen = await dialog.evaluate((el) => {
      return el.hasAttribute && el.hasAttribute('open');
    });

    expect(isOpen).toBe(true);
  });

  test('should have split panel layout', async ({ page }) => {
    // Check for split panel (wa-split-panel or sl-split-panel)
    const splitPanel = page.locator('wa-split-panel, sl-split-panel');
    await expect(splitPanel).toBeVisible();

    const startPanel = page.locator('[slot="start"]');
    const endPanel = page.locator('[slot="end"]');

    await expect(startPanel).toBeVisible();
    await expect(endPanel).toBeVisible();
  });

  test('should apply formatting', async ({ page }) => {
    const textarea = page.locator('.markdown-input');

    await textarea.clear();
    await textarea.fill('format this');

    await textarea.evaluate((el) => {
      el.setSelectionRange(0, 11);
    });

    const firstButton = page.locator('#formatting-toolbar button, #formatting-toolbar sl-button').first();
    await firstButton.click();

    const value = await textarea.inputValue();
    expect(value).toContain('**format this**');
  });

  test('should render complex markdown', async ({ page }) => {
    const textarea = page.locator('.markdown-input');
    const preview = page.locator('.markdown-preview');

    const markdown = `# Heading 1
## Heading 2

- List item 1
- List item 2

\`\`\`javascript
const x = 42;
\`\`\`

> Blockquote
`;

    await textarea.clear();
    await textarea.fill(markdown);

    await page.waitForTimeout(100);

    const previewHTML = await preview.innerHTML();
    expect(previewHTML).toContain('<h1>Heading 1</h1>');
    expect(previewHTML).toContain('<h2>Heading 2</h2>');
    expect(previewHTML).toContain('<ul>');
    expect(previewHTML).toContain('<pre>');
    expect(previewHTML).toContain('<blockquote>');
  });

  test('should show note about Web Awesome setup', async ({ page }) => {
    // Check if the note about Web Awesome project is present
    const note = page.locator('.note');
    const noteText = await note.textContent();
    expect(noteText).toContain('webawesome.com');
  });
});
