<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import type { Level } from "@/link";

// FIX:01M7GQRC6QSEZH39F7M4DD04B4
// Jauge linéaire (HTML, jamais un SVG étiré : un `rx` sous `preserveAspectRatio="none"` donne des bouts en fuseau, HRT-220) : occupation d'un disque. La couleur suit le niveau ; le libellé et
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

// Le remplissage passe par une propriété CSS posée depuis le script (jamais `style=` dans le gabarit).
const root = ref<HTMLElement | null>(null);
function fill() {
  root.value?.style.setProperty("--meter-fill", `${width.value}%`);
  // Un remplissage non nul ne devient jamais plus étroit que la barre n'est haute : toujours un bout arrondi lisible.
  root.value?.style.setProperty("--meter-min", width.value > 0 ? "var(--space-2)" : "0");
}
onMounted(fill);
watch(width, fill, { flush: "post" });
</script>

<template>
  <div ref="root" class="meter" role="img" :aria-label="label">
    <div v-if="ratio === null && indeterminate" class="meter__slide" />
    <div v-if="ratio !== null" :class="['meter__value', `meter__value--${level}`]" />
  </div>
</template>

<style scoped>
.meter {
  display: block;
  position: relative;
  width: 100%;
  height: var(--space-2);
  overflow: hidden;
  border-radius: var(--radius-pill);
  background: var(--bd);
}

.meter__value {
  width: var(--meter-fill, 0%);
  min-width: var(--meter-min, 0);
  height: 100%;
  border-radius: var(--radius-pill);
  transition: width var(--motion-base) var(--ease);
}

.meter__slide {
  position: absolute;
  top: 0;
  left: 0;
  width: 40%;
  height: 100%;
  border-radius: var(--radius-pill);
  background: var(--ac);
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
  background: var(--ac);
}

.meter__value--attention {
  background: var(--warn);
}

.meter__value--critical {
  background: var(--crit);
}
</style>
