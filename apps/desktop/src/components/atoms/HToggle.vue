<script setup lang="ts">
import { computed } from "vue";

// Même convention que HButton : désactivé = `aria-disabled` (focus conservé, raison dans
// `hint`). `busy` bloque un second clic pendant qu'une commande tourne.
const props = withDefaults(
  defineProps<{ modelValue: boolean; disabled?: boolean; busy?: boolean; hint?: string }>(),
  { disabled: false, busy: false, hint: undefined },
);

const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();

const inert = computed(() => props.disabled || props.busy);

function onClick() {
  if (inert.value) return;
  emit("update:modelValue", !props.modelValue);
}
</script>

<template>
  <button
    type="button"
    role="switch"
    :class="['toggle', { 'toggle--on': modelValue, 'toggle--disabled': disabled, 'toggle--busy': busy }]"
    :aria-checked="modelValue"
    :aria-disabled="inert ? 'true' : undefined"
    :aria-busy="busy ? 'true' : undefined"
    :title="hint"
    @click="onClick"
  >
    <span class="toggle__knob" />
  </button>
</template>

<style scoped>
.toggle {
  position: relative;
  flex: none;
  width: var(--toggle-width);
  height: var(--toggle-height);
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

.toggle--disabled {
  opacity: var(--opacity-disabled);
  cursor: not-allowed;
}

.toggle--busy {
  cursor: progress;
}

.toggle__knob {
  position: absolute;
  top: var(--toggle-inset);
  left: var(--toggle-inset);
  width: var(--toggle-knob);
  height: var(--toggle-knob);
  border-radius: var(--radius-pill);
  background: var(--tx);
  transition: transform var(--motion-fast) var(--ease);
}

.toggle--on .toggle__knob {
  background: var(--on-ac);
  transform: translateX(var(--toggle-travel));
}
</style>
