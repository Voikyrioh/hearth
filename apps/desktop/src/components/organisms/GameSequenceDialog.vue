<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import ProgressSteps from "@/components/molecules/ProgressSteps.vue";
import { useNow } from "@/composables/useNow";
import type { GameSequence } from "@/composables/useServiceLifecycle";
import { t } from "@/i18n";
import type { ServiceOperation } from "@/link";
import { outcomeOf } from "@/serviceOps/messages";
import { gameName, sequenceSteps, stopWay, worldSaved } from "@/services/game";

// La séquence d'un arrêt ou d'un redémarrage de serveur de jeu (HRT-104, BR-JEU-003, 005, 008, 011, 012) : la fenêtre de
// confirmation devient cette vue de suivi (`FormDialog`, qui tient `showModal`, le piège à focus et le retour du focus).
// Elle montre les étapes dans l'ordre du plan de l'agent : « Sauvegarde du monde » (quand la fiche en a une, avec le temps
// écoulé : la durée maximale n'est pas dans la vue de l'agent), puis « Arrêt » (avec le décompte du délai d'arrêt de la
// fiche), puis pour un redémarrage « Démarrage ». « Fermer » ne l'arrête pas : elle continue sur le serveur et figure dans
// la bande « Opérations en cours ». Quand la confirmation manque, la séquence s'arrête AVANT d'arrêter quoi que ce soit :
// la fenêtre de décision (`OperationDecisionDialog`) prend la main, cette vue la rend à la réponse.
//
// Les issues se disent ICI, avec leur cause (jamais un texte de l'agent, seulement ses CODES) ; le résultat inconnu
// (le lien est tombé avant la réponse) ne relance rien.
const props = defineProps<{
  open: boolean;
  sequence: GameSequence;
  /** L'opération telle qu'elle court ; `null` avant sa première annonce. */
  operation: ServiceOperation | null;
  /** L'opération a pris fin : sa dernière image. */
  ended: ServiceOperation | null;
}>();
const emit = defineEmits<{ close: []; decide: [] }>();

const now = useNow();
const name = computed(() => safeText(props.sequence.name));
const game = computed(() => gameName(props.sequence.game));
const restart = computed(() => props.sequence.gesture === "restart");
const running = computed(() => props.ended === null && !props.sequence.unknown);
const waiting = computed(
  () =>
    props.operation?.status === "awaiting_decision" ||
    props.operation?.status === "needs_attention",
);

// Depuis quand l'étape en cours est en cours : le décompte du délai d'arrêt et le temps écoulé de l'enregistrement.
const since = ref(Date.now());
watch(
  () => props.operation?.step ?? null,
  () => {
    since.value = Date.now();
  },
  { immediate: true },
);
const elapsedS = computed(() => Math.max(0, Math.floor((now.value - since.value) / 1000)));
const graceS = computed(() => props.sequence.game.stop.graceSeconds);
const leftS = computed(() => Math.max(0, graceS.value - elapsedS.value));

const step = computed(() => props.operation?.step ?? null);
/** La phrase de l'étape en cours, dite en direct (BR-JEU-005, BR-JEU-008). */
const line = computed(() => {
  if (waiting.value) return t("game.sequence.waiting");
  if (step.value === "save_world") {
    return elapsedS.value < 3
      ? t("game.sequence.saving", { game: game.value })
      : t("game.sequence.savingFor", { game: game.value, elapsed: secondsLabel(elapsedS.value) });
  }
  if (step.value === "stop") {
    return leftS.value > 0
      ? t("game.sequence.stopping", { left: secondsLabel(leftS.value) })
      : t("game.sequence.stoppingLate");
  }
  if (step.value === "start") return t("game.sequence.starting");
  return null;
});
function secondsLabel(seconds: number): string {
  return t(seconds === 1 ? "serviceDetail.graceOne" : "serviceDetail.grace", { n: seconds });
}

const steps = computed(() =>
  sequenceSteps(
    props.operation,
    { save: stopWay(props.sequence.game.stop) === "sequence", restart: restart.value },
    step.value === "stop" ? { stage: "stop", text: secondsLabel(leftS.value) } : null,
  ),
);
const saved = computed(() => props.operation !== null && worldSaved(props.operation));
const end = computed(() => (props.ended ? outcomeOf(props.ended, name.value) : null));
</script>

<template>
  <FormDialog
    :open="open"
    :title="t(restart ? 'game.sequence.titleRestart' : 'game.sequence.titleStop', { name })"
    hide-submit
    data-game-sequence
    @cancel="emit('close')"
  >
    <div v-if="running" class="seq__body">
      <ProgressSteps :steps="steps" :label="t('game.sequence.steps')" />
      <p v-if="saved" class="seq__note" role="status" data-game-sequence-saved>
        {{ t("game.sequence.saved") }}
      </p>
      <p v-if="line" class="seq__note" role="status" data-game-sequence-line>{{ line }}</p>
      <p class="seq__note" data-game-sequence-close-note>{{ t("game.sequence.closeNote") }}</p>
    </div>

    <Callout v-else-if="sequence.unknown && !ended" tone="warn" role="alert" data-game-sequence-unknown>
      <strong>{{ t("game.sequence.unknown") }}</strong>
      {{ t("game.sequence.unknownAfter") }}
    </Callout>

    <Callout
      v-else-if="end"
      :tone="end.tone === 'success' ? 'ok' : end.tone"
      :role="end.tone === 'error' || end.tone === 'warn' ? 'alert' : 'status'"
      data-game-sequence-end
    >
      {{ end.message }}
    </Callout>

    <template #actions>
      <HButton v-if="running && waiting" variant="secondary" data-game-sequence-decide @click="emit('decide')">
        {{ t("serviceOps.decide") }}
      </HButton>
      <HButton autofocus data-game-sequence-close @click="emit('close')">
        {{ t("common.close") }}
      </HButton>
    </template>
  </FormDialog>
</template>

<style scoped>
.seq__body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.seq__note {
  margin: 0;
  color: var(--tx-secondary);
}
</style>
