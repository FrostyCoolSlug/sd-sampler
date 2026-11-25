<script setup>
defineProps({ label: { type: String, required: true } })
</script>

<template>
  <div class="row">
    <label>{{ label }}</label>
    <div><slot /></div>
  </div>
</template>

<style scoped>
.row {
  display: grid;
  grid-template-columns: 92px 1fr;
  gap: 8px;
  align-items: center;
  margin: 8px 0;
}

.row > label {
  color: var(--dim);
}
</style>
