import init, { render_markdown } from './terraphim_editor.js';

class TeraphimEditor {
    constructor(options) {
        console.log('TeraphimEditor constructor called with options:', options);
        
        if (!options || !options.container) {
            throw new Error('Container element is required');
        }

        this.container = options.container;
        this.config = options.config || {};
        this.wasmUrl = options.wasmUrl || '/wasm/terraphim_editor_bg.wasm';
        
        console.log('Container element:', this.container);
        console.log('Configuration:', this.config);
    }

    async initialize() {
        try {
            console.log('Initializing editor...');

            // Insert the placeholder element *before* loading WASM so the
            // Rust `#[wasm_bindgen(start)]` function can find it.
            this.container.innerHTML = `<div id="editor-container"></div>`;

            // Initialize WASM; the start function will render the template.
            console.log('Initializing WASM from:', this.wasmUrl);
            await init(this.wasmUrl);

            console.log('WASM rendered editor template');

            // Apply user-provided initial content & commands
            await this.postSetup();
            console.log('Editor initialized successfully');
        } catch (error) {
            console.error('Editor initialization failed:', error);
            throw error;
        }
    }

    async postSetup() {
        const input = document.querySelector('.markdown-input');
        const preview = document.querySelector('.markdown-preview');
        const toolbar = document.querySelector('#formatting-toolbar');

        if (!input || !preview || !toolbar) {
            throw new Error('Required editor elements not found after WASM render');
        }

        // Populate toolbar with extra commands if provided
        if (Array.isArray(this.config.commands)) {
            this.config.commands.forEach(command => {
                const button = document.createElement('sl-button');
                button.innerHTML = `<sl-icon name="${command.icon}"></sl-icon>`;
                button.setAttribute('size', 'small');
                button.setAttribute('title', command.name);
                button.addEventListener('click', () => {
                    // Insert formatting
                    const start = input.selectionStart;
                    const end = input.selectionEnd;
                    const before = input.value.substring(0, start);
                    const selection = input.value.substring(start, end);
                    const after = input.value.substring(end);
                    input.value = before + command.prefix + (selection || 'text') + command.suffix + after;
                    input.focus();
                    input.selectionStart = input.selectionEnd = start + command.prefix.length;
                    // Update preview using WASM helper
                    try {
                        preview.innerHTML = render_markdown(input.value);
                    } catch (err) {
                        console.error('Markdown render error:', err);
                    }
                });
                toolbar.appendChild(button);
            });
        }

        // Inject user-provided initial markdown, if any
        if (this.config.initialContent) {
            input.value = this.config.initialContent;
            try {
                preview.innerHTML = render_markdown(input.value);
            } catch (err) {
                console.error('Markdown render error:', err);
            }
        }

        // Setup live preview using render_markdown directly
        input.addEventListener('input', () => {
            try {
                preview.innerHTML = render_markdown(input.value);
            } catch (err) {
                console.error('Markdown render error:', err);
            }
        });
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