import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  // relative asset paths so a build copied into docs/demo/ works from a subpath
  base: './',
})
