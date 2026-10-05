<script setup lang="ts">
import { computed } from "vue";

// Barres verticales (SVG pur) : une barre par valeur de 0 à 100, avec une infobulle native par
// barre (`titles`, même ordre). Sert aux cœurs du processeur.
const props = defineProps<{
  values: readonly number[];
  titles: readonly string[];
  label: string;
}>();

const BAR = 8;
const GAP = 3;
const HEIGHT = 40;

const bars = computed(() =>
  props.values.map((value, index) => {
    const height = Math.max(1, (Math.min(100, Math.max(0, value)) / 100) * HEIGHT);
    return {
      x: index * (BAR + GAP),
      y: HEIGHT - height,
      height,
      title: props.titles[index] ?? "",
    };
  }),
);
const width = computed(() => Math.max(1, props.values.length * (BAR + GAP) - GAP));
</script>

<template>
  <svg
    class="bars"
    :viewBox="`0 0 ${width} ${HEIGHT}`"
    preserveAspectRatio="none"
    role="img"
    :aria-label="label"
  >
    <rect
      v-for="(bar, index) in bars"
      :key="index"
      class="bars__bar"
      :x="bar.x"
      :y="bar.y"
      :width="BAR"
      :height="bar.height"
      rx="2"
    >
      <title>{{ bar.title }}</title>
    </rect>
  </svg>
</template>

<style scoped>
.bars {
  display: block;
  width: 100%;
  height: var(--bars-height);
}

.bars__bar {
  fill: var(--ac);
  transition:
    y var(--motion-base) var(--ease),
    height var(--motion-base) var(--ease);
}
</style>
