import { createApp } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import ElementPlus from 'element-plus'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import 'element-plus/dist/index.css'
import './styles/global.css'
import App from './App.vue'
import router, { preloadRouteViews } from './router'
import { initializeDatabase } from './services/database'

async function bootstrap() {
  const app = createApp(App)
    .use(router)
    .use(ElementPlus, { locale: zhCn })
  // Mount the shell immediately so the app never shows a blank white window
  // while SQLite performs its first-run migration. Views await the same
  // initialization promise through getDatabase().
  app.mount('#app')
  // The native window starts hidden so neither first launch nor a rejected
  // second instance can flash an empty white webview. Show it only after the
  // inline startup animation and Vue shell have both reached the DOM.
  await invoke('show_main_window')
  await initializeDatabase()
  const startup = document.getElementById('startup-screen')
  startup?.classList.add('startup-done')
  window.setTimeout(() => startup?.remove(), 320)
  // Warm the small route chunks once the first screen is usable. Exporting
  // Excel is loaded separately only when requested, so module switches no
  // longer wait for the large spreadsheet library.
  window.setTimeout(() => { void preloadRouteViews() }, 0)
}

bootstrap().catch((error) => {
  console.error('KylinStock bootstrap failed:', error)
  document.body.innerHTML = '<div style="padding:32px;font-family:sans-serif">系统初始化失败，请联系技术人员。</div>'
})
