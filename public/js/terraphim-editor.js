import init, { run } from './terraphim_editor.js';

class TeraphimEditor {
    constructor(options) {
        console.log('TeraphimEditor constructor called with options:', options);
        
        if (!options || !options.container) {
            throw new Error('Container element is required');
        }
        
        this.container = options.container;
        this.config = options.config || {};
        this.wasmUrl = options.wasmUrl || '/wasm/terraphim_editor_bg.wasm';
        
        // Ensure the container has an ID for WASM to reference
        if (!this.container.id) {
            this.container.id = 'editor-' + Math.random().toString(36).substr(2, 9);
        }
        
        console.log('Container element:', this.container);
        console.log('Configuration:', this.config);
    }

    async initialize() {
        try {
            console.log('Initializing editor...');
            
            // Initialize WASM with the correct URL
            console.log('Initializing WASM from:', this.wasmUrl);
            await init(this.wasmUrl);
            
            // Create editor structure directly in the provided container
            this.container.innerHTML = `
                <div class="terraphim-editor">
                    <div class="editor-toolbar">
                        <div id="formatting-toolbar"></div>
                    </div>
                    <div class="editor-content">
                        <div class="editor-input">
                            <textarea class="markdown-input">${this.config.initialContent || ''}</textarea>
                        </div>
                        <div class="editor-preview markdown-preview"></div>
                    </div>
                </div>
            `;

            // Initialize WASM editor
            run();
            
            console.log('Setting up editor components...');
            await this.setupEditor();
            console.log('Editor initialized successfully');
        } catch (error) {
            console.error('Editor initialization failed:', error);
            throw error;
        }
    }

    async setupEditor() {
        const input = this.container.querySelector('.markdown-input');
        const preview = this.container.querySelector('.markdown-preview');
        const toolbar = this.container.querySelector('#formatting-toolbar');

        if (!input || !preview || !toolbar) {
            throw new Error('Required editor elements not found');
        }

        // Setup toolbar
        if (this.config.commands) {
            this.config.commands.forEach(command => {
                const button = document.createElement('sl-button');
                button.innerHTML = `<sl-icon name="${command.icon}"></sl-icon>`;
                button.setAttribute('size', 'small');
                button.setAttribute('title', command.name);
                toolbar.appendChild(button);
            });
        }

        // Setup input handlers
        input.addEventListener('input', () => {
            // Use WASM to render markdown
            run();
        });

        // Initial content
        if (this.config.initialContent) {
            input.value = this.config.initialContent;
            run();
        }
    }

    destroy() {
        if (this.container) {
            this.container.innerHTML = '';
        }
    }
}

// Export for ESM
export { TeraphimEditor };

// For IIFE and UMD builds
if (typeof window !== 'undefined') {
    window.TeraphimEditor = TeraphimEditor;
} 