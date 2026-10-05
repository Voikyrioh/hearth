<script setup lang="ts" generic="T extends string">
import { ref } from "vue";

// Choix exclusif entre quelques options : groupe `radiogroup`, flèches pour passer
// d'une option à l'autre, un seul arrêt de tabulation (l'option cochée).
const props = defineProps<{
  modelValue: T;
  options: readonly { value: T; label: string }[];
  label: string;
}>();

const emit = defineEmits<{ "update:modelValue": [value: T] }>();

const buttons = ref<HTMLButtonElement[]>([]);

function move(delta: number) {
  const count = props.options.length;
  const current = props.options.findIndex((option) => option.value === props.modelValue);
  const next = props.options[(current + delta + count) % count];
  if (!next) return;
  emit("update:modelValue", next.value);
  buttons.value[props.options.indexOf(next)]?.focus();
}
</script>

<template>
  <div
    class="seg"
    role="radiogroup"
    :aria-label="label"
    @keydown.right.prevent="move(1)"
    @keydown.down.prevent="move(1)"
    @keydown.left.prevent="move(-1)"
    @keydown.up.prevent="move(-1)"
  >
    <button
      v-for="option in options"
      :key="option.value"
      ref="buttons"
      type="button"
      role="radio"
      :class="['seg__item', { 'seg__item--on': option.value === modelValue }]"
      :aria-checked="option.value === modelValue"
      :tabindex="option.value === modelValue ? 0 : -1"
      @click="emit('update:modelValue', option.value)"
    >
      {{ option.label }}
    </button>
  </div>
</template>

<style scoped>
.seg {
  display: inline-flex;
  padding: var(--space-1);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
}

.seg__item {
  min-height: var(--control-sm);
  padding: 0 var(--space-3);
  border: 0;
  border-radius: var(--space-2);
  background: transparent;
  color: var(--tx2);
  cursor: pointer;
  transition: background var(--motion-fast) var(--ease);
}

.seg__item:hover {
  color: var(--tx);
}

.seg__item--on {
  background: var(--card-2);
  color: var(--tx);
  font-weight: var(--fw-semibold);
}
</style>
