import { createApp } from 'vue'
import App from './App.vue'
import './style.css'

// 主题初始化（index.html 默认 dark，避免首帧闪烁）
const saved = localStorage.getItem('avtool-theme')
if (saved === 'light' || saved === 'dark') {
  document.documentElement.dataset.theme = saved
}

createApp(App).mount('#app')
