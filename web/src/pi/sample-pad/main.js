import { createApp } from 'vue'

import { useInspector } from '@shared/composables/useInspector'
import '@shared/styles/base.css'
import App from './App.vue'
import config from './pi.config.json'

const inspector = useInspector(config)

createApp(App).provide('inspector', inspector).mount('#app')

// The Stream Deck / OpenDeck entry point. Must stay global and named exactly this.
window.connectElgatoStreamDeckSocket = (port, uuid, registerEvent, _info, actionInfo) => {
  const { action, payload } = JSON.parse(actionInfo)
  inspector.init(payload?.settings)
  inspector.connect(port, uuid, registerEvent, action)
}

