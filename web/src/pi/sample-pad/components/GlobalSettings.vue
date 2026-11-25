<script setup>
import { computed, inject } from 'vue'

import DeviceSelect from '@shared/components/DeviceSelect.vue'
import FieldRow from '@shared/components/FieldRow.vue'
import Slider from '@shared/components/Slider.vue'
import { useCommitOnChange } from '@shared/composables/useCommitOnChange'
import { formatMs } from '@shared/utils/helpers'

const PRE_BUFFER = { min: 0, max: 30000, step: 100 }

const { state, setGlobal, ask, refreshDevices } = inject('inspector')
const globals = state.globals

const sources = computed(() => state.lists.getSources)
const sinks = computed(() => state.lists.getSinks)

const { draft: samplesDir, onChange: commitSamplesDir } = useCommitOnChange(
  () => globals.samples_directory,
  (value) => setGlobal('samples_directory', value),
)
</script>

<template>
  <section>
    <h2>Global Settings</h2>

    <section>
      <h2>Audio</h2>
      <FieldRow label="Record from">
        <DeviceSelect :items="sources" :model-value="globals.capture_device" @update:model-value="setGlobal('capture_device', $event)" />
      </FieldRow>
      <FieldRow label="Play To">
        <DeviceSelect :items="sinks" :model-value="globals.playback_device" @update:model-value="setGlobal('playback_device', $event)" />
      </FieldRow>
      <p class="hint"><button class="link" @click="refreshDevices">↻ Refresh devices</button></p>
    </section>

    <section>
      <h2>Other</h2>
      <FieldRow label="Pre-buffer">
        <Slider
          v-bind="PRE_BUFFER"
          :model-value="globals.pre_buffer_ms"
          :format="formatMs"
          @commit="setGlobal('pre_buffer_ms', $event)"
        />
      </FieldRow>
      <p class="hint">Audio kept from just before you press a pad to record. Applies to every record source.</p>

      <FieldRow label="Samples Folder">
        <div class="add">
          <input type="text" v-model="samplesDir" placeholder="Default folder" @change="commitSamplesDir" />
          <button title="Rescan the samples folder" @click="ask('selectSampleDir')">📁</button>
        </div>
      </FieldRow>
    </section>
  </section>
</template>

<style scoped>
button.link {
  height: auto;
  padding: 0;
  background: none;
  border: 0;
  color: var(--accent);
}

.add {
  display: flex;
  gap: 6px;
  margin-top: 10px;
}

</style>
