<script setup>
import { computed, ref, watch } from 'vue'

const props = defineProps({
  min: { type: Number, default: 0 },
  max: { type: Number, default: 100 },
  step: { type: Number, default: 1 },
  modelValue: Number,
  format: { type: Function, default: String },
})
defineEmits(['commit'])

// Live local value so the readout tracks the drag; follows the store if it changes.
const current = ref(props.modelValue)
watch(() => props.modelValue, (value) => {
  if (value !== current.value) current.value = value
})

const display = computed(() => props.format(current.value))
</script>

<template>
  <div class="slider">
    <input
      type="range"
      :min="min"
      :max="max"
      :step="step"
      v-model.number="current"
      @change="$emit('commit', +current)"
    >
    <span class="val">{{ display }}</span>
  </div>
</template>

<style scoped>
.slider {
  display: flex;
  align-items: center;
  gap: 10px;
}

input[type=range] {
  width: 100%;
  height: 22px;
  margin: 0;
  accent-color: var(--accent);
}

.val {
  min-width: 58px;
  text-align: right;
  color: var(--dim);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
</style>
