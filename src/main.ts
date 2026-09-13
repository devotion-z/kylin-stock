import { createApp } from 'vue'
import ElementPlus from 'element-plus'
import zhCn from 'element-plus/es/locale/lang/zh-cn'
import 'element-plus/dist/index.css'
import './styles/global.css'
import App from './App.vue'
import router from './router'
import { initializeDatabase } from './services/database'

async function bootstrap() {
  await initializeDatabase()

  createApp(App)
    .use(router)
    .use(ElementPlus, { locale: zhCn })
    .mount('#app')
}

bootstrap().catch((error) => {
  console.error('KylinStock bootstrap failed:', error)
  document.body.innerHTML = '<div style="padding:32px;font-family:sans-serif">系统初始化失败，请联系技术人员。</div>'
})
