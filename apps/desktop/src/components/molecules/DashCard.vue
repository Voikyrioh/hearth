<script setup lang="ts">
import { useId } from "vue";

// Carte du tableau de bord : en-tête (titre de section), corps. Chaque section est une région
// nommée par son titre.
defineProps<{ title: string }>();

const titleId = useId();
</script>

<template>
  <section class="card" :aria-labelledby="titleId">
    <header class="card__head">
      <h2 :id="titleId" class="card__title">{{ title }}</h2>
      <slot name="aside" />
    </header>
    <div class="card__body"><slot /></div>
  </section>
</template>

<style scoped>
.card {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.card__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.card__title {
  /* FIX:01M4D1K6V5DS7DQVR6ZV7A3HGY */
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--tx2);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
  letter-spacing: var(--ls-label);
  text-transform: uppercase;
}

.card__body {
  display: flex;
  flex: 1 1 auto;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}
</style>
