import { createApp } from 'vue'
import './style.css'
// must run before App.vue's module-level Tauri calls; no-op inside the real app
import './demo-shim'
import App from './App.vue'

createApp(App).mount('#app')
