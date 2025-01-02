import init, { Editor } from './terraphim_editor.js';

class TeraphimEditor {
    constructor(options) {
        console.log('TeraphimEditor constructor called with options:', options);
        
        if (!options || !options.container) {
            throw new Error('Container element is required');
        }
        
        this.container = options.container;
        this.config = options.config || {};
        
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
            
            // Initialize WASM
            console.log('Initializing WASM...');
            await init();
            
            // Create WASM editor instance
            this.editor = new Editor(this.container.id);
            await this.editor.init();
            
            console.log('WASM initialized successfully');

            console.log('Creating editor structure...');
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
            this.editor.render_markdown(input.value);
        });

        // Initial content
        if (this.config.initialContent) {
            input.value = this.config.initialContent;
            this.editor.render_markdown(this.config.initialContent);
        }
    }

    destroy() {
        if (this.editor) {
            // Add any necessary WASM cleanup here
        }
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