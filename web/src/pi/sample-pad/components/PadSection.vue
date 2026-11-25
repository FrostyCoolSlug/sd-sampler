<script setup>
import { computed, inject } from 'vue'

import FieldRow from '@shared/components/FieldRow.vue'
import SegmentedControl from '@shared/components/SegmentedControl.vue'
import { useCommitOnChange } from '@shared/composables/useCommitOnChange'

const PAD_FUNCTIONS = [
  { value: 'PlayNext', label: 'Play / Next', hint: 'Plays the next sample each press. Pressing again restarts it.' },
  { value: 'PlayStop', label: 'Play / Stop', hint: 'Press to play, press again to stop.' },
  { value: 'PlayFade', label: 'Play / Fade', hint: 'Press to play, press again to fade out.' },
  { value: 'StopOnRelease', label: 'Hold to play', hint: 'Plays while held, stops when released.' },
  { value: 'FadeOnRelease', label: 'Hold to play, fade out', hint: 'Plays while held, fades out when released.' },
  { value: 'Loop', label: 'Loop', hint: 'Loops until pressed again.' },
]

const PAD_ORDERS = [
  { value: 'Sequential', label: 'In order' },
  { value: 'Random', label: 'Random' },
]

const { state, set } = inject('inspector')
const settings = state.settings

const currentFunction = computed(
  () => PAD_FUNCTIONS.find((f) => f.value === settings.function) ?? PAD_FUNCTIONS[0],
)

const { draft: name, onChange: commitName } = useCommitOnChange(
  () => settings.name,
  (value) => set('name', value),
)
</script>

<template>
  <section>
    <h2>Pad</h2>

    <FieldRow label="Name">
      <input type="text" v-model="name" placeholder="Shown on the key" maxlength="30" @change="commitName" />
    </FieldRow>

    <FieldRow label="Function">
      <select :value="settings.function" @change="set('function', $event.target.value)">
        <option v-for="f in PAD_FUNCTIONS" :key="f.value" :value="f.value">{{ f.label }}</option>
      </select>
      <p class="hint">{{ currentFunction.hint }}</p>
    </FieldRow>

    <FieldRow label="Order">
      <SegmentedControl :options="PAD_ORDERS" :model-value="settings.order" @update:model-value="set('order', $event)" />
    </FieldRow>
  </section>
</template>
