<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import { t } from "@/i18n";
import type { ServiceOperation } from "@/link";
import { operationTitle, progressOf, statusText, stepText } from "@/serviceOps/messages";

// La carte d'une opération de service (HRT-84, BR-SVC-029) : le titre, l'étape, la progression, qui l'a lancée et,
// quand l'opération attend une réponse, le bouton de cette réponse. Elle se reconstruit depuis l'état du serveur :
// elle ne dépend d'aucune fenêtre ouverte et reprend sans message après une coupure du lien. Aucun bouton
// d'annulation ici (aucune opération réelle n'existe encore). Le bouton de réponse est réservé au droit
// « gérer les services » : sans lui il est désactivé, avec sa raison écrite dessous.
const props = defineProps<{
  operation: ServiceOperation;
  /** Le nom du service touché (de la liste des services). */
  name: string;
  /** Le compte voit le détail (chemins et progression de l'effacement) : il a le droit de gérer les services. */
  detailed: boolean;
  /** « Lancée par {compte}. » déjà mis en phrase, ou `null` quand l'agent ne dit pas qui. */
  by: string | null;
  /** Le compte peut répondre (droit « gérer les services » accordé). */
  canDecide: boolean;
}>();
const emit = defineEmits<{ decide: [operation: ServiceOperation] }>();

const title = computed(() => operationTitle(props.operation, props.name, props.detailed));
const step = computed(() => stepText(props.operation));
const progress = computed(() => progressOf(props.operation, props.detailed));
/** Le texte d'état : seulement quand il apporte quelque chose (réponse attendue, annulation, retour arrière). */
const status = computed(() =>
  props.operation.status === "running" || props.operation.status === "accepted"
    ? null
    : statusText(props.operation),
);
const waiting = computed(
  () =>
    props.operation.decision !== null &&
    (props.operation.status === "awaiting_decision" ||
      props.operation.status === "needs_attention"),
);
const decideLabel = computed(() =>
  t(props.operation.status === "needs_attention" ? "serviceOps.verify" : "serviceOps.decide"),
);
const decideName = computed(() =>
  t(
    props.operation.status === "needs_attention"
      ? "serviceOps.verifyLabel"
      : "serviceOps.decideLabel",
    { label: title.value },
  ),
);
// Sans le droit, une phrase dessous dit pourquoi le bouton est inactif (jamais seulement une infobulle).
const reason = computed(() => (props.canDecide ? undefined : t("serviceOps.needsRight")));
</script>

<template>
  <article class="op" :data-operation="operation.id" :data-status="operation.status">
    <h3 class="op__title">{{ title }}</h3>
    <p v-if="step" class="op__step" data-operation-step>{{ step }}</p>
    <p v-if="status" class="op__status" data-operation-status>{{ status }}</p>
    <div v-if="progress.ratio !== null" class="op__meter">
      <HMeter :ratio="progress.ratio" :label="t('serviceOps.progress', { label: title })" />
    </div>
    <p v-if="progress.text" class="op__figures" data-operation-progress>{{ progress.text }}</p>
    <p v-if="by" class="op__by" data-operation-by>{{ by }}</p>
    <div v-if="waiting" class="op__act">
      <HButton
        variant="primary"
        size="sm"
        :disabled="!canDecide"
        :hint="reason"
        :reason-below="!canDecide"
        :aria-label="decideName"
        data-operation-decide
        @click="emit('decide', operation)"
      >
        {{ decideLabel }}
      </HButton>
    </div>
  </article>
</template>

<style scoped>
.op {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  padding: var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.op__title {
  margin: 0;
  color: var(--tx);
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.op__step,
.op__status,
.op__figures,
.op__by {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-body);
}

.op__status {
  color: var(--warn);
}

.op__figures {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.op__act {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-1);
}
</style>
