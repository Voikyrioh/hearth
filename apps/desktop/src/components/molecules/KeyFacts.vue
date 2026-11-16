<script setup lang="ts">
import HIcon from "@/components/atoms/HIcon.vue";
import type { KeyFact } from "@/fiche/facts";

// Les quatre faits clés d'une fiche (HRT-242, BR-FICHE-004) : mémoire, espace disque, ports à ouvrir, durée du premier
// démarrage. Une icône et une phrase chacun ; les phrases sont composées par `fiche/facts.ts`, jamais ici.
defineProps<{ facts: KeyFact[] }>();

const ICONS = { memory: "server", disk: "box", ports: "external", start: "clock" } as const;
</script>

<template>
  <ul class="facts" data-fiche-facts>
    <li v-for="fact in facts" :key="fact.id" class="facts__item" :data-fact="fact.id">
      <HIcon :name="ICONS[fact.id]" size="sm" />
      <span>{{ fact.text }}</span>
    </li>
  </ul>
</template>

<style scoped>
.facts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(var(--card-min), 1fr));
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.facts__item {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-control);
  background: var(--card);
  box-shadow: var(--card-edge);
}
</style>
