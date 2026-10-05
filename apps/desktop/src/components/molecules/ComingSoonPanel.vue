<script setup lang="ts">
import { t } from "@/i18n";
import EmptyState from "./EmptyState.vue";

// Carte « Bientôt disponible » des vues dont le ticket suit. Les données périmées (BR-RESIL-007)
// sont gérées par le gabarit du serveur (`ServerLayout`), pas par la page.
</script>

<template>
  <section class="soon">
    <EmptyState :title="t('pages.soonTitle')" :text="t('pages.soonText')" heading="h2" />
  </section>
</template>

<style scoped>
.soon {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: var(--soon-min-height);
  padding: var(--space-6) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}
</style>
