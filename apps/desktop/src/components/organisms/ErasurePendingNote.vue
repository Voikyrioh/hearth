<script setup lang="ts">
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// Note de la page Sécurité (HRT-32 suites, HRT-18) : l'agent dit que l'effacement physique des anciennes
// empreintes de requêtes est en attente (point de contrôle retenu, disque plein au démarrage). Rien n'est
// à faire : il est repris au prochain démarrage de l'agent. Montrée aux administrateurs seulement (l'agent
// ne le dit qu'à eux) ; rien de sensible, un fait.
</script>

<template>
  <section class="note" role="status" aria-labelledby="erasure-note-title" data-erasure-pending>
    <HIcon name="info" size="sm" class="note__icon" />
    <div>
      <h2 id="erasure-note-title" class="note__title">{{ t("security.erasureTitle") }}</h2>
      <p class="note__text">{{ t("security.erasureText") }}</p>
    </div>
  </section>
</template>

<style scoped>
.note {
  display: flex;
  gap: var(--space-3);
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.note__icon {
  flex: none;
  margin-top: var(--space-1);
  color: var(--tx2);
}

.note__title {
  font-weight: var(--fw-semibold);
}

.note__text {
  color: var(--tx2);
}
</style>
