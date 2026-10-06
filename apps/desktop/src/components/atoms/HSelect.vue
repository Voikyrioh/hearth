<script setup lang="ts" generic="V extends string">
import { useId } from "vue";

// Liste déroulante native (clavier et lecteurs d'écran gratuits) : libellé toujours présent, masqué
// à l'écran avec `hide-label` quand le contexte (une cellule de tableau) le porte déjà.
withDefaults(
  defineProps<{
    modelValue: V;
    label: string;
    options: ReadonlyArray<{ value: V; label: string }>;
    hideLabel?: boolean;
    disabled?: boolean;
  }>(),
  { hideLabel: false, disabled: false },
);

defineEmits<{ "update:modelValue": [value: V] }>();

const id = useId();
</script>

<template>
  <div class="select">
    <label :for="id" :class="hideLabel ? 'sr-only' : 'select__label'">{{ label }}</label>
    <select
      :id="id"
      class="select__control"
      :value="modelValue"
      :disabled="disabled"
      @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value as V)"
    >
      <option v-for="option in options" :key="option.value" :value="option.value">
        {{ option.label }}
      </option>
    </select>
  </div>
</template>

<style scoped>
.select {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.select__label {
  color: var(--tx2);
  font-size: var(--fs-small);
}

.select__control {
  min-height: var(--target-size);
  padding: var(--field-pad-y) var(--field-pad-x);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  color: var(--tx);
  font: inherit;
}

.select__control:focus {
  border-color: var(--ac);
  box-shadow: 0 0 0 var(--halo-width) var(--focus-halo);
  outline: none;
}

.select__control:disabled {
  opacity: var(--opacity-disabled);
}
</style>
