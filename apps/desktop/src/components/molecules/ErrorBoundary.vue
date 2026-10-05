<script setup lang="ts">
import { onErrorCaptured, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import EmptyState from "./EmptyState.vue";

// Frontière d'erreur du CONTENU d'une page (jamais de la coquille) : une page dont le rendu plante est remplacée par un message avec « Réessayer »,
// la coquille autour reste utilisable (jamais d'écran blanc). L'erreur est notifiée
// discrètement et journalisée ; `resetKey` (la route) relance l'affichage au changement de page.
const props = withDefaults(defineProps<{ resetKey?: string }>(), { resetKey: undefined });

const failed = ref(false);

// Seule une erreur de RENDU (ou de cycle de vie) remplace la page par le repli. Une erreur de
// gestionnaire d'événement, de surveillant ou une promesse rejetée (ex. « Réessayer maintenant »)
// ne détruit rien : notification discrète et journal, la page reste affichée.
const RENDER_PHASES = /^(setup function|render function|component update|scheduler flush|.* hook)$/;

onErrorCaptured((error, _instance, info) => {
  reportUiError(error, `boundary:${info}`);
  if (RENDER_PHASES.test(info)) failed.value = true;
  return false;
});

watch(
  () => props.resetKey,
  () => {
    failed.value = false;
  },
);
</script>

<template>
  <div v-if="failed" class="boundary" role="alert">
    <EmptyState :title="t('errorBoundary.title')" :text="t('errorBoundary.text')" heading="h2">
      <template #action>
        <HButton variant="secondary" @click="failed = false">{{ t("errorBoundary.retry") }}</HButton>
      </template>
    </EmptyState>
  </div>
  <slot v-else />
</template>

<style scoped>
.boundary {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  padding: var(--page-pad);
}
</style>
