import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// In dev, Vite serves the UI with hot reload on :5173 and forwards /api to the
// Rust server (`cargo run`) on :8080. In production the Rust binary serves both.
export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': 'http://localhost:8080',
    },
  },
});
