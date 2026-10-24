<script setup lang="ts">
import { computed } from "vue";
import type { Level } from "@/link";

// Jauge linéaire (SVG pur) : occupation d'un disque. La couleur suit le niveau ; le libellé et
// le pictogramme sont portés par le composant qui l'emploie.
// `indeterminate` (HRT-69) : la progression n'est pas connue (`ratio` à `null`) ; une barre glisse au lieu d'une
// jauge vide, et reste immobile, rayée par sa seule position, si le mouvement est réduit.
const props = withDefaults(
  defineProps<{ ratio: number | null; level?: Level; label: string; indeterminate?: boolean }>(),
  {
    level: "normal",
    indeterminate: false,
  },
);

const width = computed(() =>
  props.ratio === null ? 0 : Math.min(100, Math.max(0, props.ratio * 100)),
);
</script>

<template>
  <svg class="meter" viewBox="0 0 100 6" preserveAspectRatio="none" role="img" :aria-label="label">
    <rect class="meter__track" x="0" y="0" width="100" height="6" rx="3" />
    <rect
      v-if="ratio === null && indeterminate"
      class="meter__slide"
      x="0"
      y="0"
      width="40"
      height="6"
      rx="3"
    />
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

.meter__slide {
  fill: var(--ac);
  transform-box: fill-box;
  animation: meter-slide var(--motion-pulse) linear infinite;
}

@keyframes meter-slide {
  from {
    transform: translateX(-100%);
  }

  to {
    transform: translateX(250%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .meter__slide {
    transform: translateX(75%);
    animation: none;
  }
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
