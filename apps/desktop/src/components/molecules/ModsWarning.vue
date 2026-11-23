<script setup lang="ts">
import HIcon from "@/components/atoms/HIcon.vue";
import { t } from "@/i18n";

// L'avertissement posé sous chaque écran des mods (HRT-262, maquettes Main, Depot, Controle, Confirmer) : « Un mod est un
// programme écrit par quelqu'un d'autre. » avec son pictogramme d'alerte, jamais un simple paragraphe de plus.
</script>

<template>
  <p class="mw" data-mods-warning>
    <HIcon name="alert" size="sm" />
    <span>{{ t("mods.warning") }}</span>
  </p>
</template>

<style scoped>
.mw {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.mw :deep(svg) {
  flex: none;
  color: var(--warn);
}
</style>
