<script setup lang="ts">
import { computed, ref } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import ProgressSteps, { type ProgressStep } from "@/components/molecules/ProgressSteps.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import { cancelRefusalText, exitSummary, failureText } from "@/deploy/messages";
import { t } from "@/i18n";
import { DEPLOY_STEPS, type ServiceInfo, type ServiceOperation } from "@/link";
import { progressOf, stepName } from "@/serviceOps/messages";

// La progression d'un déploiement (HRT-69, BR-DEPLOY-016, 019, 020, 028) : une fenêtre modale (`FormDialog`, qui tient
// `showModal`, le piège à focus et le retour du focus) qui remplace le récapitulatif au lancement. Elle montre les
// étapes DANS L'ORDRE du plan de l'agent (l'opération ne dit que celles qui ont une ligne : les étapes à venir viennent
// du plan), la progression de la récupération (pourcentage et octets quand l'agent les donne, sinon une barre
// indéterminée) et deux gestes :
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

const cancelOpen = ref(false);
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
const cancellable = computed(() => props.operation?.cancellable === true && !cancelling.value);
/** La création a commencé : l'annulation n'existe plus (BR-DEPLOY-016). Avant la première annonce, rien n'est dit. */
const tooLate = computed(
  () => props.operation !== null && !props.operation.cancellable && !cancelling.value,
);
const cancelHint = computed(() => {
  if (cancelling.value) return t("deploy.cancelling");
  return tooLate.value ? t("deploy.tooLate") : undefined;
});

const steps = computed<ProgressStep[]>(() => {
  const op = props.operation;
  const current = op?.step ? DEPLOY_STEPS.indexOf(op.step as (typeof DEPLOY_STEPS)[number]) : -1;
  const index = current < 0 ? 0 : current;
  return DEPLOY_STEPS.map((key, position): ProgressStep => {
    const state = position < index ? "done" : position === index ? "now" : "later";
    const label = stepName(key) ?? key;
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
</script>

<template>
  <FormDialog
    :open="open"
    :title="t('deploy.progressTitle', { name })"
    hide-submit
    data-deploy-progress
    @cancel="onCancelKey"
  >
    <div v-if="view === 'running'" class="progress__body">
      <ProgressSteps :steps="steps" :label="t('deploy.progressSteps')" />
      <p v-if="cancelling" class="progress__note" role="status" data-deploy-cancelling>
        {{ t("deploy.cancelling") }}
      </p>
      <p v-else-if="tooLate" class="progress__note" data-deploy-too-late>{{ t("deploy.tooLate") }}</p>
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

    <template #actions>
      <template v-if="view === 'running'">
        <HButton
          variant="danger"
          data-deploy-cancel
          :disabled="!cancellable"
          :hint="cancelHint"
          @click="cancelOpen = true"
        >
          {{ t("deploy.cancel") }}
        </HButton>
        <HButton variant="secondary" autofocus data-deploy-background @click="emit('background')">
          {{ t("deploy.background") }}
        </HButton>
      </template>
      <HButton
        v-else-if="view === 'unknown'"
        variant="secondary"
        autofocus
        data-deploy-background
        @click="emit('background')"
      >
        {{ t("deploy.background") }}
      </HButton>
      <HButton v-else-if="view === 'failed'" autofocus data-deploy-modify @click="emit('modify')">
        {{ t("deploy.modify") }}
      </HButton>
      <template v-else-if="view === 'kept'">
        <HButton variant="secondary" data-deploy-logs @click="emit('logs')">
          {{ t("deploy.seeLogs") }}
        </HButton>
        <HButton v-if="kept.stoppable" variant="secondary" data-deploy-stop @click="emit('stop')">
          {{ t("lifecycle.stop") }}
        </HButton>
        <HButton autofocus data-deploy-close @click="emit('close')">
          {{ t("common.close") }}
        </HButton>
      </template>
      <HButton v-else autofocus data-deploy-close @click="emit('close')">
        {{ t("common.close") }}
      </HButton>
    </template>
  </FormDialog>

  <AdminActDialog
    :open="open && cancelOpen"
    :server-id="serverId"
    kind="operation_cancel"
    :title="t('deploy.cancelTitle', { name })"
    :submit-label="t('deploy.cancel')"
    :cancel-label="t('deploy.keep')"
    destructive
    :perform="performCancel"
    :refusal-text="cancelRefusalText"
    @close="cancelOpen = false"
  />
</template>

<style scoped>
.progress__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.progress__note {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
