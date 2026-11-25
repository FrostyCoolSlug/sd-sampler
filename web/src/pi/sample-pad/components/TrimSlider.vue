<script setup>
import { computed, inject, ref, watch, onMounted, onBeforeUnmount } from 'vue'

const props = defineProps({
  // The live settings object — the thumbs write start/end straight into it.
  track: { type: Object, required: true },
  // Peaks are served async by the plugin and aren't part of the settings,
  // so they come in separately (never written back).
  peaks: { type: Array, required: false },
})
const { save } = inject('inspector')

const start = ref(props.track.start)
const end = ref(props.track.end)
const dragging = ref(false)

watch(
  () => [props.track.start, props.track.end],
  ([s, e]) => {
    if (!dragging.value) {
      start.value = s
      end.value = e
    }
  },
)

const beginDrag = () => {
  dragging.value = true
}

// Keep the two thumbs from crossing while dragging.
const clampStart = () => {
  if (start.value > end.value) start.value = end.value
}
const clampEnd = () => {
  if (end.value < start.value) end.value = start.value
}

function commit() {
  props.track.start = start.value
  props.track.end = end.value
  dragging.value = false
  save()
}

// The start thumb sits on top past the midpoint so it stays grabbable when both overlap at 100%.
const startZIndex = computed(() => (start.value >= 50 ? 2 : 1))
const readout = computed(() => `${(+start.value).toFixed(1)} – ${(+end.value).toFixed(1)}%`)

// Waveform: draw the track's peaks behind the thumbs, dimming the region outside the trim.
// Peaks arrive asynchronously from the plugin (computed off-thread), so this redraws whenever
// they show up or the trim moves.
const wave = ref(null)
let observer

function draw () {
  const el = wave.value
  if (!el) return
  const W = el.clientWidth, H = el.clientHeight
  if (!W || !H) return

  // Keep the backing store in sync with device pixels for a crisp render.
  const dpr = window.devicePixelRatio || 1
  const bw = Math.round(W * dpr), bh = Math.round(H * dpr)
  if (el.width !== bw || el.height !== bh) { el.width = bw; el.height = bh }

  const ctx = el.getContext('2d')
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, W, H)

  const peaks = props.peaks || []
  if (!peaks.length) return

  const accent = getComputedStyle(el).getPropertyValue('--accent').trim() || '#7aa2f7'
  const n = peaks.length
  const slot = W / n
  const barW = Math.max(1, slot * 0.8)
  const a = start.value / 100, b = end.value / 100

  ctx.fillStyle = accent
  for (let i = 0; i < n; i++) {
    const amp = Math.pow(Math.min(1, Math.max(0, peaks[i])), 0.6)
    const h = Math.max(2, amp * H)
    const pos = (i + 0.5) / n
    ctx.globalAlpha = (pos >= a && pos <= b) ? 1 : 0.24
    ctx.fillRect(i * slot + (slot - barW) / 2, (H - h) / 2, barW, h)
  }
  ctx.globalAlpha = 1
}

onMounted(() => {
  const el = wave.value
  if (el) {
    observer = new ResizeObserver(draw)
    observer.observe(el)
  }
  requestAnimationFrame(draw)
})
onBeforeUnmount(() => observer?.disconnect())
watch([() => props.peaks, start, end], draw)
</script>

<template>
  <div class="slider">
    <!-- Two thumbs on one rail: start and end of the part of the sample that plays. -->
    <div class="dual" :style="{ '--a': start, '--b': end }">
      <canvas ref="wave" class="wave"></canvas>
      <div class="rail"></div>
      <div class="fill"></div>
      <input
        type="range" min="0" max="100" step="0.1"
        v-model.number="start"
        :style="{ zIndex: startZIndex }"
        @pointerdown="beginDrag"
        @input="clampStart"
        @change="commit"
      />
      <input
        type="range" min="0" max="100" step="0.1"
        v-model.number="end"
        @pointerdown="beginDrag"
        @input="clampEnd"
        @change="commit"
      />
    </div>
    <span class="val">{{ readout }}</span>
  </div>
</template>

<style scoped>
.slider {
  display: flex;
  align-items: center;
  gap: 10px;
}

/* Two-thumb range for the trim region */
.dual {
  position: relative;
  flex: 1;
  height: 28px;
  --a: 0;
  --b: 100;
}

.dual .wave {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  pointer-events: none;
}

.dual .rail, .dual .fill {
  position: absolute;
  top: 12px;
  height: 4px;
  border-radius: 2px;
}

.dual .rail {
  left: 0;
  right: 0;
  background: var(--line);
}

.dual .fill {
  left: calc(var(--a) * 1%);
  right: calc((100 - var(--b)) * 1%);
  background: var(--accent);
}

.dual input {
  position: absolute;
  left: 0;
  top: 0;
  width: 100%;
  height: 28px;
  margin: 0;
  background: none;
  pointer-events: none;
  -webkit-appearance: none;
  appearance: none;
}

.dual input::-webkit-slider-runnable-track {
  height: 28px;
  background: none;
}

.dual input::-webkit-slider-thumb {
  -webkit-appearance: none;
  pointer-events: auto;
  width: 14px;
  height: 14px;
  margin-top: 7px;
  border-radius: 50%;
  background: #fff;
  border: 3px solid var(--accent);
  cursor: grab;
}

.dual input::-moz-range-track {
  background: none;
}

.dual input::-moz-range-thumb {
  pointer-events: auto;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #fff;
  border: 3px solid var(--accent);
  cursor: grab;
}

.val {
  min-width: 58px;
  text-align: right;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

</style>
