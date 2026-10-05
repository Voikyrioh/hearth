<script setup lang="ts">
import { computed } from "vue";

// Empreinte d'un serveur : 8 groupes de 4 caractères, en police mono, sur 2 lignes de 4, dans un
// cadre. Le texte est celui à comparer avec l'empreinte affichée à l'installation de l'agent.
// `tone="crit"` : l'empreinte reçue de l'alerte d'identité changée.
const props = withDefaults(
  defineProps<{ value: string; label?: string; tone?: "normal" | "crit" }>(),
  {
    label: undefined,
    tone: "normal",
  },
);

const groups = computed(() => props.value.split(/\s+/).filter(Boolean));
</script>

<template>
  <figure :class="['fp', { 'fp--crit': tone === 'crit' }]">
    <figcaption v-if="label" class="fp__label">{{ label }}</figcaption>
    <p class="fp__groups" data-fingerprint>
      <span v-for="(group, index) in groups" :key="index" class="fp__group">{{ group }}</span>
    </p>
  </figure>
</template>

<style scoped>
.fp {
  margin: 0;
}

.fp__label {
  margin-bottom: var(--space-2);
  color: var(--tx2);
  font-size: var(--fs-small);
}

.fp__groups {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: var(--space-2) var(--space-3);
  padding: var(--space-4);
  margin: 0;
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--bg);
  font-family: var(--font-mono);
  font-size: var(--fs-fingerprint);
  text-align: center;
}

.fp--crit .fp__groups {
  border-color: var(--crit);
}
</style>
