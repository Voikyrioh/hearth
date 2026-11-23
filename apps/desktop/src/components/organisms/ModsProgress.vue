<script setup lang="ts">
import { computed, ref } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { useModsApply } from "@/composables/useModsApply";
import { t } from "@/i18n";
import type { ModsView } from "@/link";
import { dateOf } from "@/services/backups";
import { countsOf } from "@/services/mods";
import { stepStates, stepsFor, stepText, stepTitle } from "@/services/modsApply";

// Le suivi d'un lot de mods et son issue (HRT-263, BR-MOD-025 à 027, maquettes Suivi, Fin, Echec) : les étapes cochées au
// fil du flux, quittable (l'opération continue sur le serveur), « Annuler l'opération » jusqu'au début de la mise en place ;
// puis la fin réussie (ce qui a changé, la sauvegarde faite, « Pensez à vos joueurs »), ou l'échec : le serveur n'a pas
// démarré avec ces mods, Hearth a remis les mods d'avant (sans choix à faire), les dernières lignes du journal et, quand
// l'agent la reconnaît, la phrase en clair. Les lignes du journal sont du texte du jeu : brutes, jamais du HTML.
const props = defineProps<{
  serverId: string;
  apply: ReturnType<typeof useModsApply>;
  /** La liste des mods relue après la fin : « reparti avec N mods ». */
  view: ModsView | null;
}>();
const emit = defineEmits<{ close: []; players: []; logs: [] }>();

const launch = computed(() => props.apply.launch.value);
const operation = computed(() => props.apply.operation.value);
const finished = computed(() => props.apply.finished.value);
const outcome = computed(() => props.apply.outcome.value);
const cancelOpen = ref(false);

const plan = computed(() => stepsFor(launch.value?.running ?? true));
const states = computed(() =>
  stepStates(
    plan.value,
    finished.value?.operation ?? operation.value ?? null,
    outcome.value === "succeeded",
  ),
);
const position = computed(() => {
  const at = states.value.findIndex((item) => item.state === "running");
  return Math.max(1, at + 1);
});
/** La phrase du haut porte le nom de l'étape en cours : le même que dans la liste (« Mise en place des mods »). */
const currentTitle = computed(() => {
  const step = plan.value[position.value - 1];
  return step ? stepTitle(step) : t("mods.progress.title");
});
const ratio = computed(
  () => (position.value - 1) / plan.value.length + 1 / (plan.value.length * 2),
);
const canCancel = computed(
  () => operation.value?.cancellable === true && finished.value === undefined,
);
const noteOf = computed(() => {
  const found = props.apply.backup.value;
  if (!found) return null;
  return found.note ? safeText(found.note) : dateOf(found.createdAt);
});
const enabledCount = computed(() => (props.view ? countsOf(props.view).enabled : 0));

function stateText(state: "done" | "running" | "waiting"): string {
  return t(`mods.progress.state.${state}`);
}

function close() {
  props.apply.dismiss();
  emit("close");
}
</script>

<template>
  <section v-if="launch" class="mp" data-mods-progress>
    <!-- Le suivi : tant que l'opération court -->
    <template v-if="!finished">
      <h3 class="mp__title" data-mods-progress-title>{{ currentTitle }}</h3>
      <p class="mp__lead" data-mods-progress-lead>
        {{ t("mods.progress.lead", { n: position, total: plan.length }) }}
      </p>
      <HMeter :ratio="ratio" :label="t('mods.progress.meter', { n: position, total: plan.length })" />
      <ol class="mp__steps">
        <li v-for="item in states" :key="item.step" :class="['mp__step', `mp__step--${item.state}`]" data-mods-progress-step :data-state="item.state">
          <span class="mp__step-text">
            <strong>{{ stepTitle(item.step) }}</strong>
            <span class="mp__detail">{{ stepText(item.step, launch.count, true) }}</span>
          </span>
          <span class="mp__state">{{ stateText(item.state) }}</span>
        </li>
      </ol>
      <div class="mp__bar">
        <HButton
          v-if="canCancel"
          variant="secondary"
          needs-link
          data-mods-progress-cancel
          @click="cancelOpen = true"
        >
          {{ t("mods.progress.cancel") }}
        </HButton>
        <p class="mp__detail">{{ t("mods.progress.cancelNote") }}</p>
      </div>
      <AdminActDialog
        :open="cancelOpen"
        :server-id="serverId"
        kind="operation_cancel"
        :title="t('mods.progress.cancelTitle')"
        :submit-label="t('mods.progress.cancelSubmit')"
        :cancel-label="t('mods.progress.cancelKeep')"
        :perform="apply.cancel"
        @close="cancelOpen = false"
      >
        <p class="mp__detail">{{ t("mods.progress.cancelHelp") }}</p>
      </AdminActDialog>
    </template>

    <!-- La fin réussie -->
    <template v-else-if="outcome === 'succeeded'">
      <Callout tone="ok" data-mods-end="succeeded">
        <strong class="mp__title">{{ t(launch.running ? "mods.end.title" : "mods.end.titleStopped") }}</strong>
        {{ t(launch.running ? "mods.end.leadRunning" : "mods.end.leadStopped", { n: enabledCount }) }}
      </Callout>
      <section class="mp__block">
        <h4 class="mp__subtitle">{{ t("mods.end.changed") }}</h4>
        <ul class="mp__list">
          <li v-for="row in launch.rows" :key="row.key" class="mp__change">
            <span class="mp__sign" aria-hidden="true">{{ row.sign }}</span>
            <span class="mp__step-text">
              <strong>{{ row.title }}</strong>
              <span class="mp__detail">{{ row.detail }}</span>
            </span>
          </li>
        </ul>
        <p class="mp__detail" data-mods-end-backup>
          {{ noteOf ? t("mods.end.backup", { note: noteOf }) : t("mods.end.backupBare") }}
        </p>
      </section>
      <section class="mp__block" data-mods-end-players>
        <h4 class="mp__subtitle">{{ t("mods.end.playersTitle") }}</h4>
        <p class="mp__detail">{{ t("mods.end.playersText") }}</p>
        <div class="mp__bar">
          <HButton variant="primary" data-mods-end-list @click="emit('players')">
            {{ t("mods.end.playersList") }}
          </HButton>
          <HButton variant="secondary" data-mods-end-close @click="close">{{ t("mods.end.close") }}</HButton>
        </div>
      </section>
    </template>

    <!-- L'échec : le serveur n'a pas démarré, les mods d'avant sont remis -->
    <template v-else-if="outcome === 'rolled_back' || outcome === 'failed'">
      <h3 class="mp__title" data-mods-end="rolled_back">{{ t("mods.fail.title") }}</h3>
      <Callout tone="warn" data-mods-fail-lead>
        {{ t(outcome === "rolled_back" ? "mods.fail.reverted" : "mods.fail.failed") }}
      </Callout>
      <section v-if="apply.tail.value && apply.tail.value.lines.length > 0" class="mp__block" data-mods-fail-tail>
        <h4 class="mp__subtitle">{{ t("mods.fail.says") }}</h4>
        <pre class="mp__lines">{{ apply.tail.value.lines.join("\n") }}</pre>
        <p v-if="apply.tail.value.sentence" class="mp__detail" data-mods-fail-sentence>
          {{ t("mods.fail.inClear", { sentence: safeText(apply.tail.value.sentence) }) }}
        </p>
      </section>
      <section class="mp__block">
        <p class="mp__detail" data-mods-fail-backup>
          {{ noteOf ? t("mods.fail.backup", { note: noteOf }) : t("mods.fail.backupBare") }}
        </p>
        <div class="mp__bar">
          <HButton variant="secondary" data-mods-fail-logs @click="emit('logs')">
            {{ t("mods.fail.logs") }}
          </HButton>
          <HButton variant="ghost" data-mods-end-close @click="close">{{ t("mods.end.close") }}</HButton>
        </div>
      </section>
    </template>

    <!-- Annulée, interrompue, ou à vérifier -->
    <template v-else>
      <h3 class="mp__title" :data-mods-end="outcome">
        {{ t(outcome === "cancelled" ? "mods.cancelled.title" : "mods.attention.title") }}
      </h3>
      <Callout :tone="outcome === 'cancelled' ? 'info' : 'warn'">
        {{ t(outcome === "cancelled" ? "mods.cancelled.text" : "mods.attention.text") }}
      </Callout>
      <div class="mp__bar">
        <HButton variant="secondary" data-mods-end-close @click="close">{{ t("mods.end.close") }}</HButton>
      </div>
    </template>
  </section>
  <Callout v-else-if="apply.unknownResult.value" tone="warn" data-mods-apply-unknown>
    {{ t("mods.apply.unknown") }}
  </Callout>
</template>

<style scoped>
.mp {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.mp__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.mp__title {
  display: block;
}

.mp__lead,
.mp__detail {
  margin: 0;
  color: var(--tx-secondary);
}

.mp__steps,
.mp__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.mp__step {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
}

.mp__step--running {
  box-shadow: var(--card-edge);
}

.mp__step--waiting {
  opacity: var(--opacity-muted);
}

.mp__step-text {
  display: flex;
  flex-direction: column;
  gap: var(--space-half);
  min-width: 0;
}

.mp__state {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  white-space: nowrap;
}

.mp__block {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.mp__subtitle {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.mp__change {
  display: flex;
  gap: var(--space-3);
}

.mp__sign {
  flex: none;
  width: var(--icon-size);
  color: var(--ac);
  font-family: var(--font-mono);
  font-weight: var(--fw-bold);
  text-align: center;
}

.mp__lines {
  max-height: var(--log-min-height);
  margin: 0;
  padding: var(--space-3);
  overflow: auto;
  border-radius: var(--radius-control);
  background: var(--bg);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.mp__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}
</style>
