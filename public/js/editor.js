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
    
    const commandList = document.createElement('div');
    commandList.classList.add('command-list');
    commandList.setAttribute('tabindex', '0');
    
    this.commandPalette.appendChild(commandList);
    document.body.appendChild(this.commandPalette);

    let selectedIndex = -1;
    let visibleItems = [];
    let slashPosition = null;

    // Add commands to the list
    this.commands.forEach(cmd => {
      const item = document.createElement('div');
      item.classList.add('command-item');
      item.innerHTML = `
        <sl-icon name="${cmd.icon}"></sl-icon>
        <span>${cmd.name}</span>
      `;
      
      item.addEventListener('click', () => {
        // Remove the slash when selecting a command
        if (slashPosition !== null) {
          const text = this.textarea.value;
          this.textarea.value = text.substring(0, slashPosition) + text.substring(slashPosition + 1);
          this.textarea.selectionStart = slashPosition;
          this.textarea.selectionEnd = slashPosition;
        }
        cmd.action();
        this.commandPalette.hide();
      });
      
      commandList.appendChild(item);
    });

    const updateSelection = () => {
      visibleItems = Array.from(commandList.querySelectorAll('.command-item'));
      visibleItems.forEach((item, index) => {
        if (index === selectedIndex) {
          item.classList.add('selected');
          item.scrollIntoView({ block: 'nearest' });
        } else {
          item.classList.remove('selected');
        }
      });
    };

    // Setup keyboard navigation
    commandList.addEventListener('keydown', (e) => {
      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          selectedIndex = Math.min(selectedIndex + 1, visibleItems.length - 1);
          if (selectedIndex === -1 && visibleItems.length > 0) selectedIndex = 0;
          updateSelection();
          break;
          
        case 'ArrowUp':
          e.preventDefault();
          selectedIndex = Math.max(selectedIndex - 1, 0);
          updateSelection();
          break;
          
        case 'Enter':
          e.preventDefault();
          if (selectedIndex >= 0 && selectedIndex < visibleItems.length) {
            visibleItems[selectedIndex].click();
          }
          break;
          
        case 'Escape':
          e.preventDefault();
          this.commandPalette.hide();
          break;
      }
    });

    // Show command palette on forward slash
    this.textarea.addEventListener('keydown', (e) => {
      if (e.key === '/' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        // Insert the slash character
        const start = this.textarea.selectionStart;
        const text = this.textarea.value;
        this.textarea.value = text.substring(0, start) + '/' + text.substring(this.textarea.selectionEnd);
        this.textarea.selectionStart = start + 1;
        this.textarea.selectionEnd = start + 1;
        
        // Store the position of the slash
        slashPosition = start;
        
        this.commandPalette.show();
      }
    });

    // Focus management when dialog opens
    this.commandPalette.addEventListener('sl-after-show', () => {
      selectedIndex = 0;
      updateSelection();
      setTimeout(() => {
        commandList.focus();
      }, 100);
    });

    // Reset selection when dialog is hidden
    this.commandPalette.addEventListener('sl-after-hide', () => {
      selectedIndex = -1;
      updateSelection();
      this.textarea.focus();
      // Reset slash position when dialog is closed with Escape
      if (e.key === 'Escape') {
        slashPosition = null;
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