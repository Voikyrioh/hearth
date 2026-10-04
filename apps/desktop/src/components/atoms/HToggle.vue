<script setup lang="ts">
defineProps<{ modelValue: boolean; disabled?: boolean }>();

const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>

<template>
  <button
    type="button"
    role="switch"
    :class="['toggle', { 'toggle--on': modelValue }]"
    :aria-checked="modelValue"
    :disabled="disabled"
    @click="emit('update:modelValue', !modelValue)"
  >
    <span class="toggle__knob" />
  </button>
</template>

<style scoped>
.toggle {
  position: relative;
  flex: none;
  width: 44px;
  height: 26px;
  padding: 0;
  border: 1px solid var(--bd);
  border-radius: var(--radius-pill);
  background: var(--bg);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease);
}

.toggle--on {
  border-color: var(--ac);
  background: var(--ac);
}

.toggle:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.toggle__knob {
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  border-radius: var(--radius-pill);
  background: var(--tx);
  transition: transform var(--motion-fast) var(--ease);
}

.toggle--on .toggle__knob {
  background: var(--on-ac);
  transform: translateX(18px);
}
</style>
