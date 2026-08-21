import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  cacheDir: './.vite',
  clearScreen: false,
  server: { strictPort: true, host: '127.0.0.1', port: 1421 },
  envPrefix: ['VITE_', 'TAURI_']
});
