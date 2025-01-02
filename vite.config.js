import { defineConfig } from 'vite'
import wasm from 'vite-plugin-wasm'
import topLevelAwait from 'vite-plugin-top-level-await'
import { resolve } from 'path'
import { copyFileSync, mkdirSync } from 'fs'

// Copy WASM files from pkg to public
try {
  mkdirSync('public/wasm', { recursive: true });
  mkdirSync('public/js', { recursive: true });
  copyFileSync('pkg/terraphim_editor_bg.wasm', 'public/wasm/terraphim_editor_bg.wasm');
  copyFileSync('pkg/terraphim_editor.js', 'public/js/terraphim_editor.js');
  copyFileSync('public/js/umd-example.js', 'package/js/umd-example.js');
} catch (error) {
  console.error('Error copying files:', error);
}

export default defineConfig({
  plugins: [
    wasm(),
    topLevelAwait()
  ],
  build: {
    outDir: 'package',
    target: 'esnext',
    lib: {
      entry: resolve(__dirname, 'public/js/terraphim-editor.js'),
      name: 'TeraphimEditor',
      formats: ['es', 'umd'],
      fileName: (format) => format === 'umd' ? 'terraphim-editor.umd.cjs' : 'terraphim-editor.js'
    },
    rollupOptions: {
      output: {
        dir: 'package',
        assetFileNames: (assetInfo) => {
          if (assetInfo.name === 'style.css') return 'css/terraphim-editor.css';
          if (assetInfo.name.endsWith('.wasm')) return 'wasm/[name][extname]';
          return 'js/[name][extname]';
        },
        entryFileNames: 'js/[name].[format].js'
      }
    },
    sourcemap: true,
    minify: 'esbuild'
  },
  preview: {
    port: 5173,
    open: 'example.html',
    root: resolve(__dirname, 'package')
  },
  server: {
    fs: {
      strict: false,
      allow: ['..']
    }
  }
})
