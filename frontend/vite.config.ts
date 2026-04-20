import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// Single-origin dev setup: `npm run dev` proxies /api/* to a local Rust
// backend on port 3100 (started e.g. via `cargo run` or by syncing to EVO
// with `./scripts/sync-evo.sh run`). Keeps dev behaviour identical to
// production where Axum itself serves both the frontend and the API.
const BACKEND_DEV_URL = process.env.VITE_BACKEND_URL ?? 'http://127.0.0.1:3100';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  server: {
    proxy: {
      '/api': {
        target: BACKEND_DEV_URL,
        changeOrigin: true,
      },
    },
  },
});
