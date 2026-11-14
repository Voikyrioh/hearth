<script setup lang="ts">
import { computed, ref } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import ProgressSteps from "@/components/molecules/ProgressSteps.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import type { DeleteLaunch } from "@/composables/useServiceDelete";
import { t } from "@/i18n";
import type { ServiceOperation } from "@/link";
import { statusText } from "@/serviceOps/messages";
import { cancelRefusalText, endOf, failureCauseText, stepsOf } from "@/services/delete";

// Le suivi d'une suppression (HRT-86, BR-DEL-013 à 016, 020, 023, 025, 030, 031) : la fenêtre de confirmation devient
// cette vue de suivi (`FormDialog`, qui tient `showModal`, le piège à focus et le retour du focus). Elle montre les
// étapes dans l'ordre du plan : arrêt du service (seulement s'il tournait), suppression du service, effacement des
// données (seulement sur la case cochée) avec le nombre d'éléments effacés. Deux gestes :
//
// - « Annuler la suppression » n'existe que jusqu'à la suppression du conteneur (le point de non-retour) ; ensuite il
//   reste visible, inactif, avec « Il est trop tard pour annuler » écrit dessous (l'agent le refuserait de toute façon) ;
// - « Fermer » ferme la fenêtre : la suppression continue sur le serveur, l'effacement reste visible dans la bande
//   « Opérations en cours ».
//
// Les issues se disent ICI, avec leur cause (jamais un texte de l'agent, seulement ses CODES) : service supprimé
// (données conservées ou effacées), supprimé avec des données qui n'ont pas pu partir (la liste des dossiers concernés),
// pas supprimé (le service reste entier), annulée, résultat inconnu (le lien est tombé avant la réponse : rien ne se
// relance, l'issue sera annoncée au retour du lien).
const props = defineProps<{
  open: boolean;
  serverId: string;
  launch: DeleteLaunch;
  /** L'opération telle qu'elle court ; `null` avant sa première annonce. */
  operation: ServiceOperation | null;
  /** L'opération a pris fin : sa dernière image. */
  ended: ServiceOperation | null;
  /** Annule avant le point de non-retour (acte confirmé, couvert par l'élévation). */
  performCancel: (adminPassword: string | null) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: [] }>();

const cancelOpen = ref(false);
const name = computed(() => safeText(props.launch.name));
const paths = computed(() => props.launch.folders.map((path) => safeText(path)));

const end = computed(() => (props.ended ? endOf(props.ended) : null));
const running = computed(() => props.ended === null && !props.launch.unknown);
const steps = computed(() =>
  stepsOf(props.operation, { stop: props.launch.stop, erase: props.launch.erase }),
);
const cancelling = computed(() => props.operation?.status === "cancelling");
const cancellable = computed(() => props.operation?.cancellable === true && !cancelling.value);
/** Le conteneur est supprimé : l'annulation n'existe plus (BR-DEL-014). Avant la première annonce, rien n'est dit. */
const tooLate = computed(
  () => props.operation !== null && !props.operation.cancellable && !cancelling.value,
);
const cancelHint = computed(() => {
  if (cancelling.value) return t("serviceDelete.cancelling");
  return tooLate.value ? t("serviceDelete.tooLate") : undefined;
});
const status = computed(() => (props.operation ? statusText(props.operation) : null));
const attention = computed(() => props.operation?.status === "needs_attention");
const erasing = computed(() => running.value && props.operation?.step === "erase");
const failure = computed(() => (props.ended ? failureCauseText(props.ended.cause) : null));

function onCancelKey() {
  // Échap : en cours, c'est « Fermer » (la suppression continue) ; une issue demande un geste explicite.
  emit("close");
}
</script>

<template>
  <FormDialog
    :open="open"
    :title="t('serviceDelete.progressTitle', { name })"
    hide-submit
    data-delete-progress
    @cancel="onCancelKey"
  >
    <div v-if="running" class="prog__body">
      <ProgressSteps :steps="steps" :label="t('serviceDelete.progressSteps')" />
      <p v-if="cancelling" class="prog__note" role="status" data-delete-cancelling>
        {{ t("serviceDelete.cancelling") }}
      </p>
      <p v-else-if="tooLate" class="prog__note" data-delete-too-late>{{ t("serviceDelete.tooLate") }}</p>
      <p v-if="attention && status" class="prog__note" role="status" data-delete-attention>{{ status }}</p>
      <p class="prog__note" data-delete-close-note>
        {{ erasing ? t("serviceDelete.erasingNote") : t("serviceDelete.closeNote") }}
      </p>
    </div>

    <Callout v-else-if="launch.unknown && !ended" tone="warn" role="alert" data-delete-unknown>
      <strong>{{ t("serviceDelete.unknownNow") }}</strong>
      {{ t("serviceDelete.unknownAfter") }}
    </Callout>

    <template v-else-if="end?.kind === 'success'">
      <Callout tone="ok" role="status" data-delete-success>
        <template v-if="launch.erase">{{ t("serviceDelete.successErased", { name }) }}</template>
        <template v-else-if="paths.length > 0">
          {{ t("serviceDelete.successKept", { name, paths: paths.join(", ") }) }}
          {{ t("serviceDelete.successKeptAgain", { name }) }}
        </template>
        <template v-else>{{ t("serviceDelete.successBare", { name }) }}</template>
      </Callout>
      <Callout v-if="end.forced" tone="warn" data-delete-forced>
        {{ t("serviceDelete.forced", { name }) }}
      </Callout>
    </template>

    <template v-else-if="end?.kind === 'partial'">
      <Callout tone="warn" role="alert" data-delete-partial>
        {{ t("serviceDelete.partial", { name }) }}
        <ul class="prog__paths">
          <li v-for="path in paths" :key="path" class="prog__path">{{ path }}</li>
        </ul>
        {{ t("serviceDelete.partialAfter") }}
      </Callout>
    </template>

    <template v-else-if="end?.kind === 'cancelled'">
      <Callout tone="info" role="status" data-delete-cancelled>
        {{ end.stopped ? t("serviceDelete.cancelledStopped", { name }) : t("serviceDelete.cancelledKept", { name }) }}
      </Callout>
    </template>

    <Callout v-else-if="end?.kind === 'interrupted'" tone="warn" role="alert" data-delete-interrupted>
      {{ t("serviceDelete.interrupted", { name }) }}
    </Callout>

    <Callout v-else tone="error" role="alert" data-delete-not-deleted>
      {{ t("serviceDelete.failed", { name }) }}
      <template v-if="failure"> {{ failure }}</template>
      {{ t("serviceDelete.failedKept") }}
      {{ t("serviceDelete.failedHint") }}
    </Callout>

    <template #actions>
      <template v-if="running">
        <HButton
          variant="danger"
          data-delete-cancel
          :disabled="!cancellable"
          :hint="cancelHint"
          @click="cancelOpen = true"
        >
          {{ t("serviceDelete.cancel") }}
        </HButton>
        <HButton variant="secondary" autofocus data-delete-close @click="emit('close')">
          {{ t("common.close") }}
        </HButton>
      </template>
      <HButton v-else autofocus data-delete-close @click="emit('close')">
        {{ t("common.close") }}
      </HButton>
    </template>
  </FormDialog>

  <AdminActDialog
    :open="open && cancelOpen"
    :server-id="serverId"
    kind="operation_cancel"
    :title="t('serviceDelete.cancelTitle', { name })"
    :submit-label="t('serviceDelete.cancel')"
    :cancel-label="t('serviceDelete.keep')"
    destructive
    :perform="performCancel"
    :refusal-text="cancelRefusalText"
    @close="cancelOpen = false"
  />
</template>

<style scoped>
.prog__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.prog__note {
  margin: 0;
  color: var(--tx-secondary);
}

.prog__paths {
  margin: var(--space-2) 0 0;
  padding-left: var(--space-5);
}

.prog__path {
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}
</style>
