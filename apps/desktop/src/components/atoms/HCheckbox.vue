<script setup lang="ts">
import HIcon from "./HIcon.vue";

// Case à cocher native (clavier, lecteurs d'écran) habillée aux couleurs Braise.
withDefaults(defineProps<{ modelValue: boolean; label: string; disabled?: boolean }>(), {
  disabled: false,
});

defineEmits<{ "update:modelValue": [value: boolean] }>();
</script>

<template>
  <label :class="['check', { 'check--disabled': disabled }]">
    <span class="check__control">
      <input
        class="check__input"
        type="checkbox"
        :checked="modelValue"
        :disabled="disabled"
        @change="$emit('update:modelValue', ($event.target as HTMLInputElement).checked)"
      />
      <HIcon class="check__mark" name="check" size="sm" />
    </span>
    <span class="check__label">{{ label }}</span>
  </label>
</template>

<style scoped>
.check {
  display: inline-flex;
  align-items: center;
  gap: var(--space-3);
  cursor: pointer;
}

.check--disabled {
  opacity: var(--opacity-disabled);
  cursor: not-allowed;
}

.check__control {
  position: relative;
  display: inline-grid;
  place-items: center;
  width: var(--check-size);
  height: var(--check-size);
  flex: none;
}

.check__input {
  appearance: none;
  grid-area: 1 / 1;
  width: 100%;
  height: 100%;
  margin: 0;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--space-1);
  background: var(--bg);
  cursor: inherit;
  transition: background var(--motion-fast) var(--ease);
}

.check__input:checked {
  border-color: var(--ac);
  background: var(--ac);
}

.check__mark {
  position: absolute;
  color: var(--on-ac);
  opacity: 0;
  pointer-events: none;
}

.check__input:checked + .check__mark {
  opacity: 1;
}
</style>
