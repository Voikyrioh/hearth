<script setup lang="ts">
import { computed, ref, watch } from "vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import { formatClock } from "@/composables/format";
import { useOperationActions } from "@/composables/useOperationActions";
import { t } from "@/i18n";
import type { DecisionRefusal } from "@/link";
import { CHOICE_KEYS, operationTitle, refusalText } from "@/serviceOps/messages";
import type { TrackedOperation } from "@/stores/serviceOps";

// La fenêtre de décision d'une opération de service (HRT-84, BR-SVC-029) : les choix que l'agent offre, une seule
// réponse à la fois. C'est un acte d'administration confirmé : le mot de passe est redemandé à CHAQUE réponse
// (jamais couvert par l'élévation) et le choix est signé avec l'opération. Fermer la fenêtre n'envoie rien. Le
// choix que ce client ne connaît pas n'est jamais proposé. Une réponse refusée se dit dans la fenêtre ; un
// résultat inconnu (lien coupé avant la réponse) ferme la fenêtre, sans jamais rejouer.
const props = defineProps<{ open: boolean; serverId: string; tracked: TrackedOperation | null }>();
const emit = defineEmits<{ close: [] }>();

const actions = useOperationActions(() => props.serverId);
const chosen = ref<string | null>(null);

const operation = computed(() => props.tracked?.operation ?? null);
const choices = computed(() => operation.value?.decision?.choices ?? []);
const title = computed(() =>
  t("serviceOps.dialogTitle", { name: props.tracked?.name ?? t("serviceOps.unknownService") }),
);
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

// Aucune réponse n'est préchoisie : un acte confirmé ne part jamais sur un choix que l'on n'a pas fait.
watch(
  () => props.open,
  () => {
    chosen.value = null;
  },
);

function perform(adminPassword: string | null) {
  const op = operation.value;
  const choice = choices.value.find((candidate) => candidate === chosen.value);
  if (!op || !choice) return Promise.resolve({ kind: "failed" as const });
  return actions.decide(op.id, choice, adminPassword);
}

function refusal(refused: { kind: string; retry_after_s?: number }): string {
  return refusalText(refused as DecisionRefusal);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    kind="operation_decide"
    :title="title"
    :submit-label="t('serviceOps.submit')"
    :can-submit="chosen !== null && actions.busy.value === false"
    :perform="perform"
    :refusal-text="refusal"
    @close="emit('close')"
  >
    <p class="decide__heading" data-decision-heading>{{ heading }}</p>
    <p class="decide__intro">{{ intro }}</p>
    <p class="decide__deadline" data-decision-deadline>{{ deadline }}</p>
    <fieldset class="decide__choices">
      <legend class="decide__legend">{{ t("serviceOps.choicesLegend") }}</legend>
      <label v-for="choice in choices" :key="choice" class="decide__choice">
        <input v-model="chosen" type="radio" name="operation-choice" :value="choice" :data-decision-choice="choice" />
        <span>{{ t(CHOICE_KEYS[choice]) }}</span>
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
