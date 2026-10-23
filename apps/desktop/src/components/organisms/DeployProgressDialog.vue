<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import ProgressSteps, { type ProgressStep } from "@/components/molecules/ProgressSteps.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import { cancelRefusalText, exitSummary, failureText } from "@/deploy/messages";
import { type MessageKey, t } from "@/i18n";
import type { ServiceInfo, ServiceOperation } from "@/link";
import { progressOf } from "@/serviceOps/messages";

// La progression d'un déploiement (HRT-69, BR-DEPLOY-016, 019, 020, 028) : une fenêtre modale qui remplace le
// récapitulatif au lancement. Elle montre les étapes DANS L'ORDRE de l'agent (vérification, récupération de l'image,
// création, démarrage, vérification du démarrage), la progression de la récupération (pourcentage et octets quand
// l'agent les donne, sinon une barre indéterminée) et deux gestes :
//
// - « Annuler le déploiement » n'existe QUE pendant la récupération ; dès que la création a commencé il reste visible,
//   inactif, avec « Trop tard pour annuler » écrit dessous (l'agent le refuserait de toute façon) ;
// - « Continuer en arrière-plan » ferme la fenêtre : le déploiement continue, sa progression reste dans la liste.
//
// Les issues se disent ICI, avec leur cause (jamais un texte de l'agent, seulement ses CODES) : un échec où rien n'a
// été créé (« Modifier » rouvre le formulaire rempli), un service conservé qui s'est arrêté juste après son démarrage
// (« Voir les journaux », « Arrêter » s'il boucle), un résultat inconnu (le lien est tombé avant la réponse : rien ne
// se relance, l'issue sera annoncée au retour du lien). Succès et annulation sont dits par une notification, la page
// ferme la fenêtre.
const props = defineProps<{
  open: boolean;
  serverId: string;
  /** Le nom du service que l'on déploie. */
  name: string;
  /** L'opération telle qu'elle court ; `null` avant sa première annonce. */
  operation: ServiceOperation | null;
  /** L'opération a pris fin sans créer de service ou en le conservant : sa dernière image. */
  ended: ServiceOperation | null;
  /** Le lien est tombé avant la réponse : le résultat n'est pas connu. */
  unknown: boolean;
  /** Le service tel qu'il est dans la liste (code de sortie, boucle), `null` s'il n'y est pas. */
  service: ServiceInfo | null;
  /** Annule la récupération (acte confirmé, couvert par l'élévation). */
  performCancel: (adminPassword: string | null) => Promise<ActReport>;
}>();

const emit = defineEmits<{
  background: [];
  modify: [];
  logs: [];
  stop: [];
  close: [];
}>();

/** Le plan d'un déploiement, dans l'ordre de l'agent, et le texte de chaque étape (BR-DEPLOY-016). */
const STEP_LABELS = {
  check: "serviceOps.steps.check",
  pull: "serviceOps.steps.pull",
  create: "serviceOps.steps.create",
  start: "serviceOps.steps.start",
  watch: "serviceOps.steps.watch",
} as const satisfies Record<string, MessageKey>;
const STEPS = Object.keys(STEP_LABELS) as (keyof typeof STEP_LABELS)[];

const titleId = useId();
const dialog = ref<HTMLDialogElement | null>(null);
const primary = ref<{ $el: HTMLElement } | null>(null);
const cancelOpen = ref(false);
let opener: HTMLElement | null = null;

watch(
  () => props.open,
  (open) => {
    if (open) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
  },
  { immediate: true, flush: "pre" },
);
watch(
  () => props.open,
  async (open) => {
    if (open) {
      await nextTick();
      dialog.value?.showModal();
      primary.value?.$el.querySelector("button")?.focus();
    } else {
      cancelOpen.value = false;
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true, flush: "post" },
);
onBeforeUnmount(() => {
  opener?.focus();
  opener = null;
});

const name = computed(() => safeText(props.name));

/** Ce que la fenêtre montre : la course, le résultat inconnu, ou l'issue qui demande un mot. */
type View = "running" | "unknown" | "failed" | "kept" | "interrupted";
const view = computed<View>(() => {
  const done = props.ended;
  if (done) {
    if (done.status === "succeeded") return "kept";
    if (done.status === "interrupted") return "interrupted";
    return "failed";
  }
  return props.unknown ? "unknown" : "running";
});

const cancelling = computed(() => props.operation?.status === "cancelling");
const cancellable = computed(
  () => props.operation?.cancellable === true && !cancelling.value && props.operation !== null,
);
/** La création a commencé : l'annulation n'existe plus (BR-DEPLOY-016). */
const tooLate = computed(() => props.operation !== null && !props.operation.cancellable);

const steps = computed<ProgressStep[]>(() => {
  const op = props.operation;
  const current = op?.step ? STEPS.indexOf(op.step as keyof typeof STEP_LABELS) : -1;
  const index = current < 0 ? 0 : current;
  return STEPS.map((key, position): ProgressStep => {
    const state = position < index ? "done" : position === index ? "now" : "later";
    const label = t(STEP_LABELS[key]);
    if (key === "pull" && state === "now" && op) {
      const progress = progressOf(op, true);
      return { key, label, state, bar: { ratio: progress.ratio, text: progress.text } };
    }
    return { key, label, state };
  });
});

// ── Le service conservé (BR-DEPLOY-019, 020) ─────────────────────────────────────────────────────
const kept = computed(() => {
  const service = props.service;
  const code = service?.exitCode ?? null;
  if (service?.state === "restart_loop") {
    return {
      tone: "warn" as const,
      text: t("deploy.loops", {
        name: name.value,
        code: exitSummary(code) || t("deploy.short.other"),
      }),
      stoppable: true,
    };
  }
  if (code === 0) {
    return {
      tone: "warn" as const,
      text: t("deploy.endedAtOnce", { name: name.value }),
      stoppable: false,
    };
  }
  return {
    tone: "error" as const,
    text:
      code === null
        ? t("deploy.crashedBare", { name: name.value })
        : t("deploy.crashed", { name: name.value, code: exitSummary(code) }),
    stoppable: false,
  };
});

const failure = computed(() => (props.ended ? failureText(props.ended) : ""));

function onCancelKey() {
  // Échap : en course, c'est « Continuer en arrière-plan » ; une issue demande un geste explicite.
  if (view.value === "running" || view.value === "unknown") emit("background");
}

function cancelRefusal(refused: { kind: string }): string {
  return cancelRefusalText(refused);
}
</script>

<template>
  <Teleport to="body">
    <dialog
      v-if="open"
      ref="dialog"
      class="dialog"
      :aria-labelledby="titleId"
      data-deploy-progress
      @cancel.prevent="onCancelKey"
    >
      <h2 :id="titleId" class="dialog__title">{{ t("deploy.progressTitle", { name }) }}</h2>

      <div v-if="view === 'running'" class="dialog__body">
        <ProgressSteps :steps="steps" :label="t('deploy.progressSteps')" />
        <p v-if="cancelling" class="dialog__note" role="status" data-deploy-cancelling>
          {{ t("deploy.cancelling") }}
        </p>
        <p v-else-if="tooLate" class="dialog__note" data-deploy-too-late>{{ t("deploy.tooLate") }}</p>
      </div>

      <Callout v-else-if="view === 'unknown'" tone="warn" role="alert" data-deploy-unknown>
        <strong>{{ t("deploy.unknownResult") }}</strong>
        {{ t("deploy.unknownAfter") }}
      </Callout>

      <Callout v-else-if="view === 'failed'" tone="error" role="alert" data-deploy-failed>
        {{ failure }}
      </Callout>

      <Callout v-else-if="view === 'interrupted'" tone="warn" role="alert" data-deploy-interrupted>
        {{ t("deploy.interrupted", { name }) }}
      </Callout>

      <Callout v-else :tone="kept.tone" role="alert" data-deploy-kept>{{ kept.text }}</Callout>

      <div class="dialog__actions">
        <template v-if="view === 'running'">
          <HButton
            variant="danger"
            data-deploy-cancel
            :disabled="!cancellable"
            :hint="tooLate ? t('deploy.tooLate') : t('deploy.cancelling')"
            @click="cancelOpen = true"
          >
            {{ t("deploy.cancel") }}
          </HButton>
          <HButton ref="primary" variant="secondary" data-deploy-background @click="emit('background')">
            {{ t("deploy.background") }}
          </HButton>
        </template>
        <HButton
          v-else-if="view === 'unknown'"
          ref="primary"
          variant="secondary"
          data-deploy-background
          @click="emit('background')"
        >
          {{ t("deploy.background") }}
        </HButton>
        <HButton
          v-else-if="view === 'failed'"
          ref="primary"
          data-deploy-modify
          @click="emit('modify')"
        >
          {{ t("deploy.modify") }}
        </HButton>
        <template v-else-if="view === 'kept'">
          <HButton variant="secondary" data-deploy-logs @click="emit('logs')">
            {{ t("deploy.seeLogs") }}
          </HButton>
          <HButton
            v-if="kept.stoppable"
            variant="secondary"
            data-deploy-stop
            @click="emit('stop')"
          >
            {{ t("lifecycle.stop") }}
          </HButton>
          <HButton ref="primary" data-deploy-close @click="emit('close')">
            {{ t("common.close") }}
          </HButton>
        </template>
        <HButton v-else ref="primary" data-deploy-close @click="emit('close')">
          {{ t("common.close") }}
        </HButton>
      </div>
    </dialog>
  </Teleport>

  <AdminActDialog
    :open="open && cancelOpen"
    :server-id="serverId"
    kind="operation_cancel"
    :title="t('deploy.cancelTitle', { name })"
    :submit-label="t('deploy.cancel')"
    :cancel-label="t('deploy.keep')"
    destructive
    :perform="performCancel"
    :refusal-text="cancelRefusal"
    @close="cancelOpen = false"
  />
</template>

<style scoped>
.dialog {
  width: var(--progress-dialog-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  max-height: calc(100dvh - var(--page-pad) * 2);
  overflow: auto;
  padding: var(--space-5);
  border: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  color: var(--tx);
  box-shadow: var(--card-edge);
}

.dialog[open] {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.dialog::backdrop {
  background: var(--scrim);
}

.dialog__title {
  margin: 0;
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.dialog__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.dialog__note {
  margin: 0;
  color: var(--tx-secondary);
}

.dialog__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
