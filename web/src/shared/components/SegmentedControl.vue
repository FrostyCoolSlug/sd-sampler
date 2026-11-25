<script setup>
defineProps({
  options: { type: Array, required: true }, // [{ value, label }]
  modelValue: [String, Number],
})
defineEmits(['update:modelValue'])
</script>

<template>
  <div class="seg">
    <button
      v-for="option in options"
      :key="option.value"
      :class="{ on: option.value === modelValue }"
      @click="$emit('update:modelValue', option.value)"
    >
      {{ option.label }}
    </button>
  </div>
</template>

<style scoped>
.seg {
  display: flex;
  border: 1px solid var(--line);
  border-radius: 5px;
  overflow: hidden;
}

.seg button {
  flex: 1;
  border: 0;
  border-radius: 0;
}

.seg button.on {
  background: var(--accent);
  color: #fff;
}
</style>
