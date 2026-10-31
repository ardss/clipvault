import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vitest/config'

// Same vue plugin as the build; jsdom for localStorage/document in store tests.
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: 'jsdom',
    include: ['src/modules/__tests__/**/*.spec.js'],
  },
})
