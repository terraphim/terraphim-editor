import { defineConfig } from 'vite'
import wasm from 'vite-plugin-wasm'
import topLevelAwait from 'vite-plugin-top-level-await'
import { resolve } from 'path'
import { copyFileSync, mkdirSync, existsSync } from 'fs'

// Copy WASM files from pkg to public (only if pkg exists)
try {
  if (existsSync('pkg/terraphim_editor_bg.wasm')) {
    mkdirSync('public/wasm', { recursive: true });
    mkdirSync('public/js', { recursive: true });
    copyFileSync('pkg/terraphim_editor_bg.wasm', 'public/wasm/terraphim_editor_bg.wasm');
    copyFileSync('pkg/terraphim_editor.js', 'public/js/terraphim_editor.js');
    console.log('✓ WASM files copied to public/');
  }
} catch (error) {
  console.error('Warning: Could not copy WASM files:', error.message);
}

export default defineConfig({
  plugins: [
    wasm(),
    topLevelAwait()
  ],
  // Use public as the root for dev server (for E2E tests)
  root: 'public',
  server: {
    port: 8080,
    strictPort: true,
    fs: {
      // Allow serving files from project root
      allow: ['..']
    }
  },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: 'esnext',
    lib: {
      entry: resolve(__dirname, 'public/js/terraphim-editor.js'),
      name: 'TeraphimEditor',
      formats: ['es', 'umd', 'iife'],
      fileName: (format) => {
        switch (format) {
          case 'es':
            return 'js/terraphim-editor.mjs'
          case 'umd':
            return 'js/terraphim-editor.umd.js'
          case 'iife':
            return 'js/terraphim-editor.iife.js'
          default:
            return 'js/terraphim-editor.js'
        }
      }
    },
    rollupOptions: {
      external: ['./config.js'],
      output: {
        globals: {
          './config.js': 'TeraphimConfig'
        },
        assetFileNames: (assetInfo) => {
          if (assetInfo.name === 'style.css') return 'css/terraphim-editor.css'
          if (assetInfo.name.endsWith('.wasm')) return 'wasm/[name][extname]'
          return '[ext]/[name][extname]'
        }
      }
    },
    sourcemap: true,
    minify: 'esbuild'
  },
  optimizeDeps: {
    exclude: ['./config.js']
  }
})
