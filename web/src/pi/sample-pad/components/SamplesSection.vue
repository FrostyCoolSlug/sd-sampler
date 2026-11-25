<script setup>
import { computed, inject, ref, watch } from 'vue'

import { basename } from '@shared/utils/helpers'
import TrackCard from './TrackCard.vue'

const { state, set, ask } = inject('inspector')
const settings = state.settings
const globals = state.globals

const samples = computed(() => state.lists.getSamples)
const selectedFile = ref('')

// Peaks aren't stored in the settings; the plugin serves them on request. Fetch them
// whenever a track's file isn't in the last batch we were sent (e.g. after a new sample).
watch(
  () => settings.tracks.map((t) => t.file),
  (files) => {
    const known = new Set((state.lists.getPeaks ?? []).map((p) => p.file))
    if (files.some((f) => !known.has(f))) ask('getPeaks')
  },
  { immediate: true },
)

// Writing `add_file` is what tells the plugin to import it; the plugin clears it
// again once it has worked out the file's loudness.
const addSelected = () => {
  if (!selectedFile.value) return
  set('add_file', selectedFile.value)
  selectedFile.value = ''
}
</script>

<template>
  <section>
    <h2>Samples</h2>

    <template v-if="settings.tracks.length">
      <TrackCard v-for="(track, i) in settings.tracks" :key="i" :track="track" :index="i" />
    </template>
    <div v-else class="empty">No samples yet.<br>Hold this pad to record, or add a file below.</div>

    <!-- While the plugin works out loudness for a freshly-added file. -->
    <p v-if="settings.add_file" class="busy">Adding {{ basename(settings.add_file) }}… working out its loudness</p>

    <template v-else>
      <div class="add">
        <select v-model="selectedFile">
          <template v-if="samples.length">
            <option v-for="sample in samples" :key="sample.value" :value="sample.value">{{ sample.label }}</option>
          </template>
          <option v-else value="">No audio files found</option>
        </select>
        <button class="primary" :disabled="!samples.length" @click="addSelected">Add</button>
        <button title="Rescan the samples folder" @click="ask('getSamples')">↻</button>
      </div>
      <p class="hint">From {{ globals.samples_directory || 'the default samples folder' }}: wav, ogg, flac or mp3.</p>
    </template>
  </section>
</template>

<style scoped>
.empty {
  padding: 14px;
  text-align: center;
  color: var(--dim);
  border: 1px dashed var(--line);
  border-radius: 8px;
}

.busy {
  margin: 10px 0 0;
  color: var(--accent);
}

.add {
  display: flex;
  gap: 6px;
  margin-top: 10px;
}

.add select {
  flex: 1;
  min-width: 0;
}

button.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

</style>
