class TeraphimEditor {
  constructor(targetElement, config = {}) {
    this.config = Object.assign({}, EditorConfig, config);
    this.targetElement = targetElement;
    this.editor = null;
  }

  async initialize() {
    // Load required Shoelace components
    await this.loadShoelaceComponents();
    
    // Create editor container
    const container = document.createElement('div');
    container.className = 'terraphim-editor-container';
    this.targetElement.appendChild(container);

    // Initialize editor
    this.editor = new MarkdownEditor(this.config);
    await this.editor.initialize(container);

    return this;
  }

  async loadShoelaceComponents() {
    const shoelaceBase = 'https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/';
    
    // Load Shoelace theme
    if (!document.querySelector('link[href*="shoelace"]')) {
      const link = document.createElement('link');
      link.rel = 'stylesheet';
      link.href = `${shoelaceBase}themes/light.css`;
      document.head.appendChild(link);
    }

    // Load required components
    const components = [
      'input/input.js',
      'icon/icon.js',
      'button/button.js',
      'button-group/button-group.js',
      'tooltip/tooltip.js',
      'split-panel/split-panel.js',
      'dialog/dialog.js',
      'divider/divider.js',
      'badge/badge.js'
    ];

    await Promise.all(components.map(async (component) => {
      if (!customElements.get(`sl-${component.split('/')[0]}`)) {
        await import(`${shoelaceBase}components/${component}`);
      }
    }));
  }

  getValue() {
    return this.editor.textarea.value;
  }

  setValue(markdown) {
    this.editor.textarea.value = markdown;
    this.editor.textarea.dispatchEvent(new Event('input'));
  }
}

// Make it available globally
window.TeraphimEditor = TeraphimEditor; 