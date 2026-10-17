<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import { t } from "@/i18n";
import type { ServiceOperation } from "@/link";
import { operationTitle, progressOf, statusText, stepText } from "@/serviceOps/messages";
import type { TrackedOperation } from "@/stores/serviceOps";

// La bande « Opérations en cours » (HRT-84, BR-SVC-029) : le lieu où retrouver une opération longue une fois sa
// fenêtre fermée. Visible de tous les comptes du serveur ; absente quand il n'y a rien en cours. Une ligne par
// opération : le titre, l'étape, une jauge fine, qui l'a lancée et « Voir ». Au plus trois lignes, puis « +N
// autres » qui déplie. Une opération qui attend une réponse porte le bouton de cette réponse, réservé au droit
// « gérer les services » (désactivé avec sa raison sans lui). Un compte sans le droit lit « Effacement des
// données de {nom} en cours. » sans chemin ni progression. Aucun bouton d'annulation dans la bande.
const props = defineProps<{
  operations: readonly TrackedOperation[];
  /** Qui a lancé l'opération (indication de la liste des services), ou `null`. */
  launchedBy: (operation: ServiceOperation) => string | null;
  /** Le compte voit le détail et peut répondre : il a le droit de gérer les services. */
  allowed: boolean;
}>();
const emit = defineEmits<{
  see: [operation: TrackedOperation];
  decide: [operation: TrackedOperation];
}>();

const VISIBLE = 3;
const expanded = ref(false);
const shown = computed(() =>
  expanded.value ? props.operations : props.operations.slice(0, VISIBLE),
);
const hidden = computed(() => Math.max(0, props.operations.length - VISIBLE));
watch(hidden, (n) => {
  if (n === 0) expanded.value = false;
});

function title(tracked: TrackedOperation): string {
  return operationTitle(tracked.operation, tracked.name, props.allowed);
}

function figures(tracked: TrackedOperation) {
  return progressOf(tracked.operation, props.allowed);
}

function detail(tracked: TrackedOperation): string | null {
  const status = tracked.operation.status;
  if (status === "running" || status === "accepted") return stepText(tracked.operation);
  return statusText(tracked.operation);
}

function waiting(tracked: TrackedOperation): boolean {
  const op = tracked.operation;
  return (
    op.decision !== null && (op.status === "awaiting_decision" || op.status === "needs_attention")
  );
}

/** Une opération sans le droit d'effacer : seule la phrase est dite (ni étape ni jauge). */
function sentenceOnly(tracked: TrackedOperation): boolean {
  return tracked.operation.kind === "delete" && !props.allowed;
}

const count = computed(() =>
  props.operations.length === 1
    ? t("serviceOps.countOne")
    : t("serviceOps.count", { n: props.operations.length }),
);

// Le nombre d'opérations est annoncé quand il change, poliment et après 500 ms ; aucune progression chiffrée n'est annoncée.
const announced = ref("");
let timer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => props.operations.length,
  () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      announced.value = count.value;
    }, 500);
  },
);
onBeforeUnmount(() => clearTimeout(timer));

function launched(tracked: TrackedOperation): string | null {
  return props.launchedBy(tracked.operation);
}
</script>

<template>
  <section
    v-if="operations.length > 0"
    class="strip"
    role="region"
    :aria-label="t('serviceOps.stripRegion')"
    data-operations-strip
  >
    <h2 class="strip__title">{{ t("serviceOps.stripTitle") }}</h2>
    <p class="sr-only" role="status" aria-live="polite">{{ announced }}</p>
    <ul class="strip__list">
      <li
        v-for="tracked in shown"
        :key="`${tracked.serverId}/${tracked.operation.id}`"
        class="row"
        :data-operation="tracked.operation.id"
        :data-status="tracked.operation.status"
      >
        <div class="row__text">
          <span class="row__title" data-operation-title>{{ title(tracked) }}</span>
          <span v-if="!sentenceOnly(tracked) && detail(tracked)" class="row__step" data-operation-step>{{
            detail(tracked)
          }}</span>
          <span v-if="launched(tracked)" class="row__by" data-operation-by>{{ launched(tracked) }}</span>
        </div>
        <div v-if="!sentenceOnly(tracked) && figures(tracked).ratio !== null" class="row__meter">
          <HMeter
            :ratio="figures(tracked).ratio"
            :label="t('serviceOps.progress', { label: title(tracked) })"
          />
        </div>
        <span
          v-if="!sentenceOnly(tracked) && figures(tracked).text"
          class="row__figures"
          data-operation-progress
        >
          {{ figures(tracked).text }}
        </span>
        <div class="row__acts">
          <HButton
            v-if="waiting(tracked)"
            variant="primary"
            size="sm"
            :disabled="!allowed"
            :hint="allowed ? undefined : t('serviceOps.needsRight')"
            :aria-label="
              t(
                tracked.operation.status === 'needs_attention'
                  ? 'serviceOps.verifyLabel'
                  : 'serviceOps.decideLabel',
                { label: title(tracked) },
              )
            "
            data-operation-decide
            @click="emit('decide', tracked)"
          >
            {{ t(tracked.operation.status === "needs_attention" ? "serviceOps.verify" : "serviceOps.decide") }}
          </HButton>
          <HButton
            variant="secondary"
            size="sm"
            :aria-label="t('serviceOps.seeLabel', { label: title(tracked) })"
            data-operation-see
            @click="emit('see', tracked)"
          >
            {{ t("serviceOps.see") }}
          </HButton>
        </div>
        <p
          v-if="waiting(tracked) && !allowed"
          class="row__reason"
          data-operation-reason
        >
          {{ t("serviceOps.needsRight") }}
        </p>
      </li>
    </ul>
    <HButton
      v-if="hidden > 0"
      variant="ghost"
      size="sm"
      :aria-expanded="expanded"
      :aria-label="expanded ? t('serviceOps.less') : t('serviceOps.moreLabel', { n: hidden })"
      data-operations-more
      @click="expanded = !expanded"
    >
      {{ expanded ? t("serviceOps.less") : t("serviceOps.more", { n: hidden }) }}
    </HButton>
  </section>
</template>

<style scoped>
.strip {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.strip__title {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
  letter-spacing: var(--ls-label);
  text-transform: uppercase;
}

.strip__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  list-style: none;
}

.row {
  display: grid;
  grid-template-columns: var(--op-columns);
  align-items: center;
  gap: var(--space-3);
  min-height: var(--target-size);
  min-width: 0;
}

/* Sous 960 px la jauge passe sous le libellé. */
@media (max-width: 960px) {
  .row {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .row__meter,
  .row__figures {
    grid-column: 1 / -1;
  }
}

.row__text {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-1) var(--space-3);
  min-width: 0;
}

.row__title {
  color: var(--tx);
  font-size: var(--fs-body);
  font-weight: var(--fw-medium);
  overflow-wrap: anywhere;
}

.row__step,
.row__by,
.row__figures,
.row__reason {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.row__figures {
  font-family: var(--font-mono);
  white-space: nowrap;
}

.row__reason {
  grid-column: 1 / -1;
  color: var(--warn);
}

.row__acts {
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
}
</style>
