<script setup lang="ts">
import { t } from "@/i18n";
import HButton from "../atoms/HButton.vue";
import Callout from "./Callout.vue";

// « L'arrêt propre de ce jeu n'est plus garanti » (HRT-109, BR-JEU-039) : un réglage dont Hearth a besoin pour arrêter
// le jeu proprement, posé par la fiche, a été défait dans le fichier (par le jeu ou par une main). Hearth le SIGNALE et
// ne le réécrit jamais de lui-même. Ce bandeau passe avant tous les autres. Les réglages concernés sont nommés par le
// libellé de la fiche. « Remettre les réglages imposés » est présent mais inactif, avec sa phrase : cette remise est
// livrée avec l'arrêt par étapes (HRT-104), pas avec cet onglet.
defineProps<{ labels: readonly string[] }>();
</script>

<template>
  <Callout tone="warn" role="alert" data-imposed-broken>
    <p class="ib__lead">{{ t("gameSettings.imposedBroken") }}</p>
    <p v-if="labels.length > 0" class="ib__list" data-imposed-broken-list>
      {{ t("gameSettings.imposedBrokenList", { labels: labels.join(", ") }) }}
    </p>
    <template #actions>
      <div class="ib__fix">
        <HButton variant="secondary" size="sm" disabled data-imposed-broken-fix>
          {{ t("gameSettings.imposedBrokenFix") }}
        </HButton>
        <p class="ib__soon">{{ t("gameSettings.imposedBrokenSoon") }}</p>
      </div>
    </template>
  </Callout>
</template>

<style scoped>
.ib__lead,
.ib__list {
  margin: 0;
}

.ib__list {
  margin-top: var(--space-1);
  color: var(--tx-secondary);
}

.ib__fix {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
}

.ib__soon {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
