async function initializeUMDEditor() {
    if (!window.EditorConfig) {
        console.error('Configuration not loaded! Please check if config.js is loaded properly.');
        return;
    }

    try {
        const editor = new window.TeraphimEditor({
            container: document.getElementById('editor-container-umd'),
            config: window.EditorConfig
        });
        await editor.initialize();
    } catch (error) {
        console.error('Failed to initialize UMD editor:', error);
    }
}

window.addEventListener('load', initializeUMDEditor); 