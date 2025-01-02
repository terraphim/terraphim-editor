import init, { run } from './terraphim_editor.js';

export class TeraphimEditor {
    constructor(options) {
        this.container = options.container;
        this.config = options.config;
    }

    async initialize() {
        try {
            // Initialize WASM
            await init('../wasm/terraphim_editor_bg.wasm');
            
            // Create the editor structure
            this.container.innerHTML = `
                <div id="app">
                    <div class="terraphim-editor">
                        <div class="toolbar" id="formatting-toolbar"></div>
                        <div class="editor-area">
                            <textarea class="markdown-input"></textarea>
                            <div class="markdown-preview"></div>
                        </div>
                    </div>
                </div>
            `;

            // Initialize the WASM components
            run();

            // Initialize the editor components
            this.setupEditor();
        } catch (error) {
            console.error('Failed to initialize editor:', error);
            throw error;
        }
    }

    setupEditor() {
        // Add your editor setup code here
        console.log('Editor initialized with config:', this.config);
    }
}

// For UMD builds, expose the TeraphimEditor class globally
if (typeof window !== 'undefined') {
    window.TeraphimEditor = TeraphimEditor;
} 