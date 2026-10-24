<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import HTag from "@/components/atoms/HTag.vue";
import Callout from "@/components/molecules/Callout.vue";
import type { PortVerdict } from "@/deploy/messages";
import { t } from "@/i18n";

// Les notes sous un port du serveur (HRT-69, HRT-102, BR-DEPLOY-007 à 009) : l'avertissement du serveur (un port réservé
// par un service arrêté se confirme au lancement), le port sous 1024, et le port libre le plus proche que l'AGENT a
// calculé quand le port saisi est refusé. Partagé par la rangée d'un déploiement et le champ d'une fiche : rien n'est
// recalculé ici, les textes arrivent déjà en phrases (`verdict`). Les avertissements ne se montrent que s'il n'y a
// aucune erreur (`clean`) : l'erreur d'abord.
defineProps<{
  verdict: PortVerdict;
  /** Le port est sous 1024 : avertissement, même sans réponse du serveur. */
  privileged: boolean;
  /** Aucune erreur sous le champ (forme ou serveur). */
  clean: boolean;
}>();

defineEmits<{ suggest: [port: number] }>();
</script>

<template>
  <div v-if="clean && (verdict.warning || privileged)" class="notes">
    <Callout v-if="verdict.warning" tone="warn" inline data-port-warning>
      {{ verdict.warning }}
      <HTag v-if="verdict.reserved" tone="warn">{{ t("deploy.portToConfirm") }}</HTag>
    </Callout>
    <Callout v-if="privileged" tone="warn" inline data-port-privileged>
      {{ t("deploy.portPrivileged") }}
    </Callout>
  </div>
  <div v-if="verdict.error && verdict.suggested !== null" class="notes">
    <Callout tone="info" inline data-port-suggested>
      {{ t("deploy.portSuggested", { port: verdict.suggested }) }}
    </Callout>
    <!-- Hors de l'encart : sur son fond teinté le bord du bouton disparaissait (revue UX 2026-10-24, constat 12). -->
    <div class="notes__use">
      <HButton
        variant="secondary"
        size="sm"
        data-port-use
        @click="$emit('suggest', verdict.suggested as number)"
      >
        {{ t("deploy.portUseSuggested", { port: verdict.suggested }) }}
      </HButton>
    </div>
  </div>
</template>

<style scoped>
.notes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
</style>
