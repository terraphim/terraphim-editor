class MarkdownEditor {
  constructor() {
    if (typeof shortcuts === 'undefined') {
      console.error('Shortcuts not loaded');
      this.shortcuts = [];
    } else {
      this.shortcuts = shortcuts;
    }
  }

  initialize() {
    // Get DOM elements after template is rendered
    this.textarea = document.querySelector('.markdown-input');
    this.toolbar = document.querySelector('#formatting-toolbar');
    this.shortcutsList = document.querySelector('#shortcuts-list');
    this.dialog = document.querySelector('.shortcuts-dialog');
    this.helpButton = document.querySelector('#show-help');

    // Check if elements exist
    if (!this.textarea || !this.toolbar || !this.shortcutsList || !this.dialog || !this.helpButton) {
      console.error('Required DOM elements not found');
      return;
    }

    if (this.shortcuts.length === 0) {
      console.error('No shortcuts available');
      return;
    }

    this.setupShortcuts();
    this.setupHelpDialog();
  }

  wrapSelectedText(prefix, suffix) {
    const start = this.textarea.selectionStart;
    const end = this.textarea.selectionEnd;
    const text = this.textarea.value;
    const before = text.substring(0, start);
    const selection = text.substring(start, end);
    const after = text.substring(end);
    
    const wrappedText = selection ? selection : 'text';
    this.textarea.value = before + prefix + wrappedText + suffix + after;
    
    this.textarea.focus();
    this.textarea.selectionStart = selection ? start + prefix.length : start + prefix.length;
    this.textarea.selectionEnd = selection ? end + prefix.length : start + prefix.length + 4;
    
    this.textarea.dispatchEvent(new Event('input'));
  }

  setupShortcuts() {
    // Create toolbar buttons
    this.shortcuts.forEach(shortcut => {
      const button = document.createElement('sl-tooltip');
      button.setAttribute('content', shortcut.key);
      
      button.innerHTML = `
        <sl-button size="small" variant="default">
          <sl-icon name="${shortcut.name}"></sl-icon>
        </sl-button>
      `;
      
      button.querySelector('sl-button').addEventListener('click', () => {
        this.wrapSelectedText(shortcut.prefix, shortcut.suffix);
      });
      
      this.toolbar.appendChild(button);
    });

    // Setup keyboard shortcuts
    this.textarea.addEventListener('keydown', (e) => {
      const key = `${e.ctrlKey ? 'ctrl+' : ''}${e.key.toLowerCase()}`;
      const shortcut = this.shortcuts.find(s => s.key === key);
      
      if (shortcut) {
        e.preventDefault();
        this.wrapSelectedText(shortcut.prefix, shortcut.suffix);
      }
    });
  }

  setupHelpDialog() {
    // Create shortcut list items
    this.shortcuts.forEach(shortcut => {
      const item = document.createElement('div');
      item.className = 'shortcut-item';
      item.innerHTML = `
        <sl-icon name="${shortcut.name}"></sl-icon>
        <span class="shortcut-desc">${shortcut.desc}</span>
        <sl-badge variant="neutral">${shortcut.key}</sl-badge>
      `;
      this.shortcutsList.appendChild(item);
    });

    this.helpButton.addEventListener('click', () => this.dialog.show());
  }
}

// Wait for both DOM content and WASM initialization
const initEditor = () => {
  const checkElements = () => {
    const required = [
      '.markdown-input',
      '#formatting-toolbar',
      '#shortcuts-list',
      '.shortcuts-dialog',
      '#show-help'
    ];

    if (required.every(selector => document.querySelector(selector))) {
      const editor = new MarkdownEditor();
      editor.initialize();
    } else {
      // Check again in 100ms
      setTimeout(checkElements, 100);
    }
  };

  checkElements();
};

document.addEventListener('DOMContentLoaded', initEditor); 