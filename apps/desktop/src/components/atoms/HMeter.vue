<script setup lang="ts">
import { computed } from "vue";
import type { Level } from "@/link";

// Jauge linéaire (SVG pur) : occupation d'un disque. La couleur suit le niveau ; le libellé et
// le pictogramme sont portés par le composant qui l'emploie.
const props = withDefaults(defineProps<{ ratio: number | null; level?: Level; label: string }>(), {
  level: "normal",
});

const width = computed(() =>
  props.ratio === null ? 0 : Math.min(100, Math.max(0, props.ratio * 100)),
);
</script>

<template>
  <svg class="meter" viewBox="0 0 100 6" preserveAspectRatio="none" role="img" :aria-label="label">
    <rect class="meter__track" x="0" y="0" width="100" height="6" rx="3" />
    <rect
      v-if="ratio !== null"
      :class="['meter__value', `meter__value--${level}`]"
      x="0"
      y="0"
      :width="width"
      height="6"
      rx="3"
    />
  </svg>
</template>

<style scoped>
.meter {
  display: block;
  width: 100%;
  height: var(--space-2);
}

.meter__track {
  fill: var(--bd);
}

.meter__value {
  transition: width var(--motion-base) var(--ease);
}

.meter__value--normal {
  fill: var(--ac);
}

.meter__value--attention {
  fill: var(--warn);
}

.meter__value--critical {
  fill: var(--crit);
}
</style>
