class MarkdownEditor {
  constructor() {
    if (typeof shortcuts === 'undefined') {
      console.error('Shortcuts not loaded');
      this.shortcuts = [];
    } else {
      this.shortcuts = shortcuts;
    }
    this.commandPalette = null;
    this.commands = [
      { name: 'Heading 1', icon: 'type-h1', action: () => this.wrapSelectedText('# ', '') },
      { name: 'Heading 2', icon: 'type-h2', action: () => this.wrapSelectedText('## ', '') },
      { name: 'Heading 3', icon: 'type-h3', action: () => this.wrapSelectedText('### ', '') },
      { name: 'Bold', icon: 'type-bold', action: () => this.wrapSelectedText('**', '**') },
      { name: 'Italic', icon: 'type-italic', action: () => this.wrapSelectedText('_', '_') },
      { name: 'Underline', icon: 'type-underline', action: () => this.wrapSelectedText('<u>', '</u>') },
      { name: 'Custom', icon: 'gear', action: () => this.showCustomDialog() }
    ];
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
    this.setupCommandPalette();
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

  setupCommandPalette() {
    // Create command palette dialog
    this.commandPalette = document.createElement('sl-dialog');
    this.commandPalette.label = 'Commands';
    this.commandPalette.classList.add('command-palette');
    
    const searchInput = document.createElement('sl-input');
    searchInput.placeholder = 'Search commands...';
    searchInput.classList.add('command-search');
    
    const commandList = document.createElement('div');
    commandList.classList.add('command-list');
    
    this.commandPalette.appendChild(searchInput);
    this.commandPalette.appendChild(commandList);
    document.body.appendChild(this.commandPalette);

    // Add commands to the list
    this.commands.forEach(cmd => {
      const item = document.createElement('div');
      item.classList.add('command-item');
      item.innerHTML = `
        <sl-icon name="${cmd.icon}"></sl-icon>
        <span>${cmd.name}</span>
      `;
      
      item.addEventListener('click', () => {
        cmd.action();
        this.commandPalette.hide();
      });
      
      commandList.appendChild(item);
    });

    // Setup search functionality
    searchInput.addEventListener('input', (e) => {
      const query = e.target.value.toLowerCase();
      const items = commandList.querySelectorAll('.command-item');
      
      items.forEach(item => {
        const text = item.textContent.toLowerCase();
        item.style.display = text.includes(query) ? 'flex' : 'none';
      });
    });

    // Show command palette on forward slash
    this.textarea.addEventListener('keydown', (e) => {
      if (e.key === '/' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        this.commandPalette.show();
        searchInput.value = '';
        searchInput.focus();
      }
    });
  }

  showCustomDialog() {
    const dialog = document.createElement('sl-dialog');
    dialog.label = 'Custom Formatting';
    
    dialog.innerHTML = `
      <sl-input label="Prefix" id="prefix-input"></sl-input>
      <sl-input label="Suffix" id="suffix-input"></sl-input>
      <sl-button slot="footer" variant="primary">Apply</sl-button>
      <sl-button slot="footer" variant="default">Cancel</sl-button>
    `;
    
    document.body.appendChild(dialog);
    
    const [applyBtn, cancelBtn] = dialog.querySelectorAll('sl-button');
    const prefixInput = dialog.querySelector('#prefix-input');
    const suffixInput = dialog.querySelector('#suffix-input');
    
    applyBtn.addEventListener('click', () => {
      this.wrapSelectedText(prefixInput.value, suffixInput.value);
      dialog.hide();
    });
    
    cancelBtn.addEventListener('click', () => dialog.hide());
    
    dialog.addEventListener('sl-after-hide', () => dialog.remove());
    
    dialog.show();
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