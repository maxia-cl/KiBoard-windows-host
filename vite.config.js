import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Matches src-tauri/tauri.conf.json's build.devUrl (Tauri's own convention: fixed port,
// fail instead of picking a random one, ignore src-tauri/ so a `cargo build` doesn't
// trigger a frontend reload).
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
})
