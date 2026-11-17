// SPDX-License-Identifier: AGPL-3.0-or-later

import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

// tauri sets TAURI_ENV_PLATFORM while it drives the build
const platform = process.env.TAURI_ENV_PLATFORM

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    // tauri expects a fixed port, failing beats silently picking another one
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  base: './',
  build: {
    // WebView2 on Windows, WebKitGTK on Linux: no extra transpilation ships
    target: platform === 'windows' ? 'chrome120' : 'safari16',
    outDir: 'dist',
    emptyOutDir: true,
  },
})
