<script setup>
import { computed, inject } from 'vue'

import FieldRow from '@shared/components/FieldRow.vue'
import { basename } from '@shared/utils/helpers'
import TrimSlider from './TrimSlider.vue'

const props = defineProps({
  track: { type: Object, required: true },
  index: { type: Number, required: true },
})

const { state, save } = inject('inspector')

const gainLabel = computed(() => `×${Number(props.track.gain).toFixed(2)}`)

// Peaks aren't stored in the settings; the plugin serves them per file on request.
const trackPeaks = computed(
  () => (state.lists.getPeaks ?? []).find((p) => p.file === props.track.file)?.peaks,
)

const remove = () => {
  state.settings.tracks.splice(props.index, 1)
  save()
}
</script>

<template>
  <div class="track">
    <div class="track-head">
      <span class="track-n">{{ index + 1 }}</span>
      <span class="track-name" :title="track.file">{{ basename(track.file) }}</span>
      <span class="track-gain" title="Loudness normalisation (EBU R128, -23 LUFS)">{{ gainLabel }}</span>
      <button class="x" title="Remove from this pad (the file is kept)" @click="remove">✕</button>
    </div>

    <FieldRow label="Trim">
      <!-- Pass the live settings track so the thumbs write start/end into it,
           plus the async peaks separately (they're never persisted). -->
      <TrimSlider :track="track" :peaks="trackPeaks" />
    </FieldRow>
  </div>
</template>

<style scoped>
.track {
  margin: 8px 0;
  padding: 9px 11px 10px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: 8px;
}

.track-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.track-n {
  color: var(--dim);
  font-variant-numeric: tabular-nums;
}

.track-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
}

.track-gain {
  color: var(--dim);
  font-size: 11px;
}

.x {
  width: 24px;
  height: 24px;
  padding: 0;
  border-radius: 50%;
  color: var(--dim);
  line-height: 1;
}

.x:hover:not(:disabled) {
  color: var(--danger);
  border-color: var(--danger);
}

/* Tighter label column and spacing for the FieldRow inside a track card. */
.track :deep(.row) {
  grid-template-columns: 52px 1fr;
  margin: 5px 0;
}

</style>
