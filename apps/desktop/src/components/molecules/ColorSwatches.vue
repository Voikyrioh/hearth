<script setup lang="ts">
import { t } from "@/i18n";
import { SERVER_COLORS, type ServerColor } from "@/link";

// Palette des 8 couleurs de serveur : pastilles rondes, la sélectionnée a un anneau. Le sens ne
// repose pas sur la couleur seule : chaque pastille a un nom accessible (« Couleur 3 »).
withDefaults(defineProps<{ modelValue: ServerColor; label: string; disabled?: boolean }>(), {
  disabled: false,
});

defineEmits<{ "update:modelValue": [value: ServerColor] }>();
</script>

<template>
  <div class="swatches" role="radiogroup" :aria-label="label">
    <button
      v-for="n in SERVER_COLORS"
      :key="n"
      type="button"
      role="radio"
      :aria-checked="modelValue === n"
      :aria-label="t('connect.colorOption', { n })"
      :class="['swatch', `swatch--c${n}`, { 'swatch--on': modelValue === n }]"
      :disabled="disabled"
      :data-color="n"
      @click="$emit('update:modelValue', n)"
    />
  </div>
</template>

<style scoped>
.swatches {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.swatch {
  width: var(--swatch-size);
  height: var(--swatch-size);
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: var(--swatch);
  cursor: pointer;
}

.swatch:disabled {
  opacity: var(--opacity-disabled);
  cursor: not-allowed;
}

.swatch--on {
  box-shadow:
    0 0 0 var(--ring-width) var(--card),
    0 0 0 calc(var(--ring-width) * 2) var(--tx);
}

.swatch:focus-visible {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.swatch--c1 { --swatch: var(--server-1); }
.swatch--c2 { --swatch: var(--server-2); }
.swatch--c3 { --swatch: var(--server-3); }
.swatch--c4 { --swatch: var(--server-4); }
.swatch--c5 { --swatch: var(--server-5); }
.swatch--c6 { --swatch: var(--server-6); }
.swatch--c7 { --swatch: var(--server-7); }
.swatch--c8 { --swatch: var(--server-8); }
</style>
