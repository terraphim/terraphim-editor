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
      formats: ['es', 'umd', 'iife'],
      fileName: (format) => {
        if (format === 'umd') return 'js/terraphim-editor.umd.cjs'
        if (format === 'iife') return 'js/terraphim-editor.iife.js'
        return 'js/terraphim-editor.js'
      }
    },
    rollupOptions: {
      output: {
        extend: true,
        name: 'TeraphimEditor',
        format: 'iife',
        exports: 'named',
        globals: {
          TeraphimEditor: 'TeraphimEditor'
        },
        assetFileNames: (assetInfo) => {
          if (assetInfo.name === 'style.css') return 'css/terraphim-editor.css'
          if (assetInfo.name.endsWith('.wasm')) return 'wasm/[name][extname]'
          return 'js/[name][extname]'
        }
      }
    },
    sourcemap: true,
    minify: false // Disable minification for debugging
  },
  preview: {
    port: 5173,
    open: 'example-iife.html',
    root: resolve(__dirname, 'package')
  }
})
