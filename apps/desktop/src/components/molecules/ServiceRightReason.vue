<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";
import type { RightsGate } from "@/services/rights";
import CommandBlock from "./CommandBlock.vue";

// La raison écrite SOUS un bouton d'action de service désactivé faute du droit « gérer les services »
// (HRT-78, BR-GRANT-014). UN seul composant pour tous les boutons d'action de service : ils le consomment sans
// rien recalculer. Le bouton qu'il explique porte `describedBy = id` (`HButton`) : la raison est lue avec lui,
// jamais seulement en infobulle. Le texte décrit (`id`) est la phrase de raison et, selon le profil, la ligne
// qui suit ; la commande à copier n'en fait pas partie (elle se lit à part, un bloc à recopier n'est pas une
// description).
//
// - administrateur sans le droit : la raison, la phrase d'introduction et la commande à copier ;
// - compte en lecture seule : la raison et « Demandez à la personne qui administre ce serveur. » ;
// - agent ancien : « Ce serveur doit être mis à jour… », aucune action ;
// - lecture en échec : « Impossible de lire votre droit… » et « Vérifier à nouveau » ;
// - lecture en cours, droit accordé, pilotage désactivé (l'état du moteur parle) : rien.
// Lecture seule : aucune case, aucun commutateur, rien qui laisse croire que le droit se donne d'ici (BR-GRANT-005).
const props = defineProps<{
  gate: RightsGate;
  /** L'`id` du texte décrit, à passer au `describedBy` des boutons que ce composant explique. */
  id: string;
}>();
const emit = defineEmits<{ retry: [] }>();

const sentence = computed(() => {
  switch (props.gate.kind) {
    case "denied":
      return t("rights.reason");
    case "old-agent":
      return t("rights.oldAgent");
    case "failed":
      return t("rights.readFailed");
    default:
      return null;
  }
});

/** La ligne qui suit la raison d'un compte sans le droit : la commande annoncée, ou à qui s'adresser. */
const follow = computed(() => {
  const gate = props.gate;
  if (gate.kind !== "denied") return null;
  return gate.command ? t("rights.adminIntro") : t("rights.readonlyAsk");
});

const command = computed(() => (props.gate.kind === "denied" ? props.gate.command : null));
</script>

<template>
  <div v-if="sentence" class="reason" data-rights-reason :data-rights-gate="gate.kind">
    <div :id="id" class="reason__text">
      <p class="reason__sentence" data-rights-sentence>{{ sentence }}</p>
      <p v-if="follow" class="reason__follow" data-rights-follow>{{ follow }}</p>
    </div>
    <CommandBlock
      v-if="command"
      :text="command"
      :label="t('rights.commandLabel')"
      :copy-label="t('rights.copyCommand')"
      :copied-label="t('rights.copied')"
      :failed-label="t('rights.copyFailed')"
      data-rights-command
    />
    <HButton
      v-if="gate.kind === 'failed'"
      variant="secondary"
      size="sm"
      data-rights-retry
      @click="emit('retry')"
    >
      {{ t("engine.recheck") }}
    </HButton>
  </div>
</template>

<style scoped>
/* C5 : la largeur est bornée par la page qui l'accueille (`.svc__rights`, `--panel-max`) ; ici le composant
   garde en plus son propre plafond pour ne jamais s'étirer ailleurs (design grand écran, 2026-10-08). */
.reason {
  box-sizing: border-box;
  width: 100%;
  max-width: var(--panel-max);
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
  min-width: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.reason :deep(.block) {
  align-self: stretch;
}

.reason__text {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.reason__sentence,
.reason__follow {
  margin: 0;
  overflow-wrap: anywhere;
}

.reason__sentence {
  color: var(--tx);
}
</style>
