<script setup lang="ts">
import StaleStamp from "./StaleStamp.vue";

// Enveloppe commune des données périmées (BR-RESIL-007) : quand le lien n'est pas
// « Connecté », le contenu est désaturé et daté ; il reste lisible et jamais retiré.
withDefaults(
  defineProps<{
    stale: boolean;
    lastContactAt: number | null;
    /** Faux : la page n'a encore rien chargé, il n'y a donc rien à dater (HRT-38, C46). */
    stamped?: boolean;
  }>(),
  { stamped: true },
);
</script>

<template>
  <div :class="['surface', { 'surface--stale': stale }]" :data-stale="stale ? 'true' : undefined">
    <div class="surface__body"><slot /></div>
    <StaleStamp v-if="stale && stamped" class="surface__stamp" :last-contact-at="lastContactAt" />
  </div>
</template>

<style scoped>
.surface {
  position: relative;
  display: flex;
  flex: 1 0 auto;
  flex-direction: column;
}

.surface__body {
  display: flex;
  flex: 1 0 auto;
  flex-direction: column;
}

.surface--stale .surface__body {
  filter: grayscale(var(--grayscale-stale));
  opacity: var(--opacity-stale);
  transition:
    filter var(--motion-base) var(--ease),
    opacity var(--motion-base) var(--ease);
}

.surface__stamp {
  position: absolute;
  top: var(--space-3);
  right: var(--space-4);
}
</style>
