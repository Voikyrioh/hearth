<script setup lang="ts">
import { computed, ref, watch } from "vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import { formatClock } from "@/composables/format";
import { useOperationActions } from "@/composables/useOperationActions";
import { t } from "@/i18n";
import type { OperationChoice } from "@/link";
import { CHOICE_KEYS, operationTitle, refusalText } from "@/serviceOps/messages";
import { decisionReason, isStopOperation } from "@/services/game";
import type { TrackedOperation } from "@/stores/serviceOps";

// La fenêtre de décision d'une opération de service (HRT-84, BR-SVC-029) : les choix que l'agent offre, une seule
// réponse à la fois. C'est un acte d'administration confirmé : le mot de passe est redemandé à CHAQUE réponse
// (jamais couvert par l'élévation) et le choix est signé avec l'opération. Fermer la fenêtre n'envoie rien. Le
// choix que ce client ne connaît pas n'est jamais proposé. Une réponse refusée se dit dans la fenêtre ; un
// résultat inconnu (lien coupé avant la réponse) ferme la fenêtre, sans jamais rejouer.
//
// L'arrêt (ou le redémarrage) d'un serveur de jeu dont la confirmation d'enregistrement manque (HRT-104, BR-JEU-006,
// BR-JEU-007) a sa propre mise en page : Hearth n'a rien arrêté, le service tourne toujours, et le choix le plus sûr,
// « Annuler », est présélectionné, listé en premier et reçoit le curseur ; « Arrêter quand même » est le choix qui coûte
// (bouton d'envoi en rouge). Hearth ne renvoie jamais l'ordre seul. Le bouton qui quitte sans répondre dit qu'on
// décidera plus tard : sans réponse en 15 minutes, l'arrêt est annulé.
const props = defineProps<{
  open: boolean;
  serverId: string;
  tracked: TrackedOperation | null;
  /** Le nom du jeu, quand le geste vient de cette fenêtre (sinon le nom du service tient lieu de nom de jeu). */
  gameName?: string | null;
}>();
const emit = defineEmits<{ close: [] }>();

const actions = useOperationActions(() => props.serverId);
const chosen = ref<string | null>(null);

const operation = computed(() => props.tracked?.operation ?? null);
/** L'arrêt d'un serveur de jeu qui attend un choix (BR-JEU-006) : le seul cas de décision d'un arrêt ou d'un redémarrage. */
const game = computed(
  () => operation.value !== null && isStopOperation(operation.value) && props.tracked !== null,
);
const restart = computed(() => operation.value?.kind === "restart");
const offered = computed(() => operation.value?.decision?.choices ?? []);
/** Pour un arrêt de jeu, « Annuler » d'abord (le choix sûr) ; sinon l'ordre de l'agent. */
const choices = computed(() =>
  game.value
    ? [...offered.value].sort((a, b) => Number(b === "cancel") - Number(a === "cancel"))
    : offered.value,
);
const title = computed(() => {
  const name = props.tracked?.name ?? t("serviceOps.unknownService");
  if (game.value)
    return t(restart.value ? "game.sequence.titleRestart" : "game.sequence.titleStop", { name });
  return t("serviceOps.dialogTitle", { name });
});
const gameLines = computed(() => {
  const op = operation.value;
  if (!game.value || !op) return [];
  const label = props.gameName ?? props.tracked?.name ?? "";
  if (decisionReason(op) === "not-confirmed") return [t("game.decision.notConfirmed")];
  return [t("game.decision.cannotAsk", { game: label }), t("game.decision.cannotAskMore")];
});
function choiceLabel(choice: OperationChoice): string {
  if (game.value && choice === "continue") {
    return t(restart.value ? "game.decision.continueRestart" : "game.decision.continueStop");
  }
  if (game.value && choice === "cancel") return t("game.decision.cancel");
  return t(CHOICE_KEYS[choice]);
}
const submitLabel = computed(() => {
  if (!game.value) return t("serviceOps.submit");
  if (chosen.value === "cancel") return t("game.decision.submitCancel");
  return t(restart.value ? "game.decision.submitRestart" : "game.decision.submitStop");
});
const intro = computed(() =>
  t(
    operation.value?.status === "needs_attention"
      ? "serviceOps.dialogAttention"
      : "serviceOps.dialogAwaiting",
  ),
);
const heading = computed(() =>
  props.tracked ? operationTitle(props.tracked.operation, props.tracked.name, true) : "",
);
const deadline = computed(() => {
  const at = operation.value?.decision?.expiresAt ?? null;
  return at === null
    ? t("serviceOps.noDeadline")
    : t("serviceOps.deadline", { time: formatClock(at) });
});

// Aucune réponse n'est préchoisie : un acte confirmé ne part jamais sur un choix que l'on n'a pas fait. Seule exception,
// l'arrêt d'un jeu : « Annuler » (le service reste en marche) est le choix sûr, présélectionné (BR-JEU-006).
watch(
  () => props.open,
  () => {
    chosen.value = game.value && choices.value.includes("cancel") ? "cancel" : null;
  },
);

function perform(adminPassword: string | null) {
  const op = operation.value;
  const choice = choices.value.find((candidate) => candidate === chosen.value);
  if (!op || !choice) return Promise.resolve({ kind: "failed" as const });
  return actions.decide(op.id, choice, adminPassword);
}

function refusal(refused: { kind: string }): string {
  return refusalText(refused);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    kind="operation_decide"
    :title="title"
    :submit-label="submitLabel"
    :cancel-label="game ? t('game.decision.later') : undefined"
    :destructive="game && chosen === 'continue'"
    :can-submit="chosen !== null && actions.busy.value === false"
    :perform="perform"
    :refusal-text="refusal"
    @close="emit('close')"
  >
    <template v-if="game">
      <Callout tone="warn" data-decision-game>
        {{ gameLines.join(" ") }}
      </Callout>
      <p class="decide__intro" data-decision-nothing>{{ t("game.decision.nothingYet") }}</p>
    </template>
    <template v-else>
      <p class="decide__heading" data-decision-heading>{{ heading }}</p>
      <p class="decide__intro">{{ intro }}</p>
    </template>
    <p class="decide__deadline" data-decision-deadline>{{ deadline }}</p>
    <fieldset class="decide__choices">
      <legend class="decide__legend">{{ t("serviceOps.choicesLegend") }}</legend>
      <label v-for="choice in choices" :key="choice" class="decide__choice">
        <input v-model="chosen" type="radio" name="operation-choice" :value="choice" :data-decision-choice="choice" />
        <span>{{ choiceLabel(choice) }}</span>
      </label>
    </fieldset>
  </AdminActDialog>
</template>

<style scoped>
.decide__heading {
  margin: 0;
  color: var(--tx);
  font-weight: var(--fw-semibold);
}

.decide__intro,
.decide__deadline {
  margin: 0;
  color: var(--tx-secondary);
}

.decide__choices {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  border: 0;
}

.decide__legend {
  padding: 0;
  margin-bottom: var(--space-2);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
}

.decide__choice {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--tx);
  cursor: pointer;
}

.decide__choice input {
  accent-color: var(--ac);
}
</style>
