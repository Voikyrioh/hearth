<script setup lang="ts">
import { t } from "@/i18n";
import EmptyState from "./EmptyState.vue";
import StaleSurface from "./StaleSurface.vue";

// Carte « Bientôt disponible » des vues dont le ticket suit. Elle passe par `StaleSurface` :
// c'est la vitrine du mécanisme de données périmées (BR-RESIL-007) en attendant les vraies cartes.
defineProps<{ stale: boolean; lastContactAt: number | null }>();
</script>

<template>
  <StaleSurface :stale="stale" :last-contact-at="lastContactAt">
    <section class="soon">
      <EmptyState :title="t('pages.soonTitle')" :text="t('pages.soonText')" heading="h2" />
    </section>
  </StaleSurface>
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
