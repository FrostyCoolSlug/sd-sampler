// Reactive store + Stream Deck (OpenDeck) WebSocket bridge, as a factory.
//
// Each property inspector passes its own config:
//   { defaults, globalDefaults, listEvents, arrayFields }
// and gets the same API back. Components bind to `state`; Vue reactivity does the
// rest, so there is no manual render step.

import { reactive } from 'vue'
import { isSame } from '../utils/helpers'

// How often to re-read settings, since they can change from elsewhere
// (e.g. the plugin finishing a recording).
const SETTINGS_POLL_MS = 2000

export function useInspector({ defaults = {}, globalDefaults = {}, listEvents = [], arrayFields = [] }) {
  // Fill in defaults, then force fields that must always be arrays.
  const normaliseSettings = (settings) => {
    const result = { ...defaults, ...settings }
    for (const key of arrayFields) {
      if (!Array.isArray(result[key])) result[key] = []
    }
    return result
  }
  const normaliseGlobals = (settings) => ({ ...globalDefaults, ...settings })

  const state = reactive({
    settings: normaliseSettings(),
    globals: normaliseGlobals(),
    lists: Object.fromEntries(listEvents.map((event) => [event, []])),
    connected: false,
  })

  let socket = null
  let context = null // our inspector UUID, sent as `context` on every message
  let action = null
  let pollTimer = null

  // --- outgoing -----------------------------------------------------------

  const send = (message) => {
    if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(message))
  }

  // Ask the plugin to run one of its registered events (device lists, folder pick, ...).
  const ask = (event) => send({ event: 'sendToPlugin', action, context, payload: { event } })
  const save = () => send({ event: 'setSettings', context, payload: state.settings })
  const saveGlobals = () => send({ event: 'setGlobalSettings', context, payload: state.globals })

  /** Write one key on this action's settings and persist. */
  const set = (key, value) => {
    state.settings[key] = value
    save()
  }

  /** Write one key on the global settings and persist. */
  const setGlobal = (key, value) => {
    state.globals[key] = value
    saveGlobals()
  }

  /** Re-request the device lists from the plugin. */
  const refreshDevices = () => {
    ask('getSources')
    ask('getSinks')
  }

  // --- incoming -----------------------------------------------------------

  const handlers = {
    didReceiveSettings: (payload) => {
      const next = normaliseSettings(payload.settings)
      if (!isSame(next, state.settings)) Object.assign(state.settings, next)
    },
    didReceiveGlobalSettings: (payload) => {
      const next = normaliseGlobals(payload.settings)
      if (!isSame(next, state.globals)) Object.assign(state.globals, next)
    },
    sendToPropertyInspector: (payload) => {
      if (!listEvents.includes(payload.event)) return
      state.lists[payload.event] = Array.isArray(payload.items) ? payload.items : []
    },
  }

  const onMessage = ({ data }) => {
    const message = JSON.parse(data)
    handlers[message.event]?.(message.payload || {})
  }

  // --- lifecycle ----------------------------------------------------------

  /** Seed the store from the action info OpenDeck hands over before the socket is up. */
  const init = (initialSettings) => Object.assign(state.settings, normaliseSettings(initialSettings))

  function connect(port, uuid, registerEvent, actionUuid) {
    context = uuid
    action = actionUuid
    state.connected = true

    socket = new WebSocket(`ws://127.0.0.1:${port}`)
    socket.onmessage = onMessage
    socket.onclose = () => clearInterval(pollTimer)
    socket.onopen = () => {
      send({ event: registerEvent, uuid })
      send({ event: 'getSettings', context })
      send({ event: 'getGlobalSettings', context })
      listEvents.forEach(ask)
      pollTimer = setInterval(() => send({ event: 'getSettings', context }), SETTINGS_POLL_MS)
    }
  }

  return { state, set, setGlobal, ask, save, saveGlobals, refreshDevices, init, connect }
}
