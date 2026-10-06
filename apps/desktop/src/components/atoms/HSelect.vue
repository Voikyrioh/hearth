<script setup lang="ts" generic="T extends string">
import { useId } from "vue";
import HIcon from "./HIcon.vue";

// Liste déroulante native (clavier, lecteurs d'écran) habillée aux couleurs Braise : un choix
// parmi quelques options.
defineProps<{
  modelValue: T;
  label: string;
  options: readonly { value: T; label: string }[];
}>();

defineEmits<{ "update:modelValue": [value: T] }>();

const id = useId();
</script>

<template>
  <div class="select">
    <label class="select__label" :for="id">{{ label }}</label>
    <div class="select__box">
      <select
        :id="id"
        class="select__input"
        :value="modelValue"
        @change="$emit('update:modelValue', ($event.target as HTMLSelectElement).value as T)"
      >
        <option v-for="option in options" :key="option.value" :value="option.value">
          {{ option.label }}
        </option>
      </select>
      <HIcon class="select__chevron" name="chevron-down" size="sm" />
    </div>
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

.select__box {
  position: relative;
  display: flex;
  align-items: center;
}

.select__input {
  width: 100%;
  padding: var(--field-pad-y) var(--field-pad-x);
  padding-right: var(--space-6);
  appearance: none;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  color: var(--tx);
  font: inherit;
  cursor: pointer;
}

.select__input:focus-visible {
  border-color: var(--ac);
  box-shadow: 0 0 0 var(--halo-width) var(--focus-halo);
  outline: none;
}

.select__chevron {
  position: absolute;
  right: var(--space-3);
  color: var(--tx2);
  pointer-events: none;
}
</style>
