class MarkdownEditor {
  constructor(config) {
    this.config = config;
    this.shortcuts = config.shortcuts;
    this.commands = config.commands.map(cmd => ({
      ...cmd,
      action: () => this.wrapSelectedText(cmd.prefix, cmd.suffix)
    }));
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
    // Create inline command menu
    const commandMenu = document.createElement('div');
    commandMenu.classList.add('command-menu');
    commandMenu.style.display = 'none';
    commandMenu.setAttribute('tabindex', '0');
    
    const commandList = document.createElement('div');
    commandList.classList.add('command-list');
    
    commandMenu.appendChild(commandList);
    document.body.appendChild(commandMenu);

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
        if (slashPosition !== null) {
          const text = this.textarea.value;
          this.textarea.value = text.substring(0, slashPosition) + text.substring(slashPosition + 1);
          this.textarea.selectionStart = slashPosition;
          this.textarea.selectionEnd = slashPosition;
        }
        cmd.action();
        hideCommandMenu();
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

    const positionCommandMenu = () => {
      const caretPosition = getCaretCoordinates(this.textarea, this.textarea.selectionStart);
      const textareaRect = this.textarea.getBoundingClientRect();
      const menuRect = commandMenu.getBoundingClientRect();
      const viewportWidth = window.innerWidth;
      const viewportHeight = window.innerHeight;
      
      // Calculate initial position
      let left = textareaRect.left + caretPosition.left;
      let top = textareaRect.top + caretPosition.top + 20;
      
      // Adjust horizontal position if menu would go outside viewport
      if (left + menuRect.width > viewportWidth) {
        left = viewportWidth - menuRect.width - 10; // 10px padding from right edge
      }
      if (left < 0) {
        left = 10; // 10px padding from left edge
      }
      
      // Adjust vertical position if menu would go outside viewport
      if (top + menuRect.height > viewportHeight) {
        // Show menu above the caret if there's not enough space below
        top = textareaRect.top + caretPosition.top - menuRect.height - 10;
      }
      if (top < 0) {
        top = 10; // 10px padding from top edge
      }
      
      commandMenu.style.position = 'fixed';
      commandMenu.style.left = `${left}px`;
      commandMenu.style.top = `${top}px`;
    };

    const showCommandMenu = () => {
      commandMenu.style.display = 'block';
      selectedIndex = 0;
      updateSelection();
      positionCommandMenu();
      commandMenu.focus();
    };

    const hideCommandMenu = () => {
      commandMenu.style.display = 'none';
      selectedIndex = -1;
      slashPosition = null;
      this.textarea.focus();
    };

    // Keyboard navigation
    commandMenu.addEventListener('keydown', (e) => {
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
          hideCommandMenu();
          break;
      }
    });

    // Show command menu on forward slash
    this.textarea.addEventListener('keydown', (e) => {
      if (e.key === '/' && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        const start = this.textarea.selectionStart;
        const text = this.textarea.value;
        this.textarea.value = text.substring(0, start) + '/' + text.substring(this.textarea.selectionEnd);
        this.textarea.selectionStart = start + 1;
        this.textarea.selectionEnd = start + 1;
        
        slashPosition = start;
        showCommandMenu();
      }
    });

    // Hide menu when clicking outside
    document.addEventListener('click', (e) => {
      if (!commandMenu.contains(e.target) && e.target !== this.textarea) {
        hideCommandMenu();
      }
    });

    // Update menu position on scroll or resize
    window.addEventListener('scroll', positionCommandMenu);
    window.addEventListener('resize', positionCommandMenu);
    this.textarea.addEventListener('scroll', positionCommandMenu);
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

function getCaretCoordinates(element, position) {
  const div = document.createElement('div');
  const styles = getComputedStyle(element);
  const properties = [
    'direction', 'boxSizing', 'width', 'height', 'overflowX', 'overflowY',
    'borderTopWidth', 'borderRightWidth', 'borderBottomWidth', 'borderLeftWidth',
    'paddingTop', 'paddingRight', 'paddingBottom', 'paddingLeft',
    'fontStyle', 'fontVariant', 'fontWeight', 'fontStretch', 'fontSize',
    'fontSizeAdjust', 'lineHeight', 'fontFamily', 'textAlign', 'textTransform',
    'textIndent', 'textDecoration', 'letterSpacing', 'wordSpacing'
  ];

  div.style.position = 'absolute';
  div.style.visibility = 'hidden';
  div.style.whiteSpace = 'pre-wrap';

  properties.forEach(prop => {
    div.style[prop] = styles[prop];
  });

  div.textContent = element.value.substring(0, position);
  const span = document.createElement('span');
  span.textContent = element.value.substring(position) || '.';
  div.appendChild(span);
  
  document.body.appendChild(div);
  const coordinates = {
    top: span.offsetTop,
    left: span.offsetLeft
  };
  document.body.removeChild(div);
  
  return coordinates;
}

// Update the initEditor function
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
      // Pass the EditorConfig when initializing
      const editor = new MarkdownEditor(window.EditorConfig || {
        shortcuts: [],
        commands: [],
        styles: {}
      });
      editor.initialize();
    } else {
      // Check again in 100ms
      setTimeout(checkElements, 100);
    }
  };

  checkElements();
};

// Make sure config is loaded before initializing
document.addEventListener('DOMContentLoaded', () => {
  if (window.EditorConfig) {
    initEditor();
  } else {
    console.error('Editor configuration not found. Make sure config.js is loaded before editor.js');
  }
}); 