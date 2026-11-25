<script setup>
import { computed } from 'vue'
import { flatten } from '../utils/helpers'

const props = defineProps({
  items: { type: Array, default: () => [] },
  modelValue: String,
})
defineEmits(['update:modelValue'])

const isKnown = computed(() => flatten(props.items).some((item) => item.value === props.modelValue))
</script>

<template>
  <select :value="modelValue" @change="$emit('update:modelValue', $event.target.value)">
    <!-- Keep an unplugged current value selectable rather than silently dropping it. -->
    <option v-if="modelValue && !isKnown" :value="modelValue">{{ modelValue }} (not found)</option>

    <template v-for="(item, i) in items" :key="i">
      <optgroup v-if="item.children?.length" :label="item.label">
        <option v-for="child in item.children" :key="child.value" :value="child.value">{{ child.label }}</option>
      </optgroup>
      <option v-else-if="!item.children" :value="item.value">{{ item.label }}</option>
    </template>
  </select>
</template>
