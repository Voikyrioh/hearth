<script setup lang="ts">
import { onErrorCaptured, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import EmptyState from "./EmptyState.vue";

// Frontière d'erreur : une page qui plante est remplacée par un message avec « Réessayer »,
// la coquille autour reste utilisable (jamais d'écran blanc). L'erreur est notifiée
// discrètement et journalisée ; `resetKey` (la route) relance l'affichage au changement de page.
const props = withDefaults(defineProps<{ resetKey?: string }>(), { resetKey: undefined });

const failed = ref(false);

onErrorCaptured((error, _instance, info) => {
  failed.value = true;
  reportUiError(error, `boundary:${info}`);
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
