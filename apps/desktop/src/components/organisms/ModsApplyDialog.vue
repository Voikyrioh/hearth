<script setup lang="ts">
import { computed, onMounted } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import ModsWarning from "@/components/molecules/ModsWarning.vue";
import PlayersOnlineCard from "@/components/molecules/PlayersOnlineCard.vue";
import AdminActPanel from "@/components/organisms/AdminActPanel.vue";
import { useAdminAct } from "@/composables/useAdminAct";
import type { useModsApply } from "@/composables/useModsApply";
import type { OnlinePlayersState } from "@/composables/useOnlinePlayers";
import type { ActReport } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { ModsBatch } from "@/link";
import { formatSize } from "@/serviceOps/messages";
import {
  type ChangeRow,
  spaceMissingText,
  stepsFor,
  stepText,
  stepTitle,
} from "@/services/modsApply";

// « Vérifiez avant d'appliquer » (HRT-263, BR-MOD-025, 026, 031, maquette Confirmer révisée) : ce qui va changer, les
// étapes dans l'ordre, les joueurs connectés (nombre, places, pseudos) qui seront déconnectés, la durée et le retour
// automatique, la taille de la sauvegarde, la phrase de BR-MOD-009, et le mot de passe (acte `ModsApply`, jamais couvert
// par l'élévation : demandé à chaque fois). Espace insuffisant pour la sauvegarde : dit avant, rien n'est lancé. Console
// muette ou serveur arrêté : la carte des joueurs n'a pas de chiffre et l'application reste possible.
const props = defineProps<{
  serverId: string;
  batch: ModsBatch;
  rows: ChangeRow[];
  running: boolean;
  apply: ReturnType<typeof useModsApply>;
  players: OnlinePlayersState;
  playersRefreshing: boolean;
  now: number;
  /** « Ouvrir la console » existe : l'onglet Console est offert pour ce serveur. */
  consoleOpen: boolean;
  /** Les mods qui empêcheront encore le serveur de démarrer après ce lot (une phrase chacun). */
  blockers: string[];
}>();
const emit = defineEmits<{
  back: [];
  refreshPlayers: [];
  openConsole: [];
  launched: [];
}>();

onMounted(() => void props.apply.loadPreview());

const plan = computed(() => stepsFor(props.running));
const playersKnown = computed(
  () => props.players.kind === "ready" && props.players.view.state === "ok",
);
const preview = computed(() =>
  props.apply.preview.value.kind === "ready" ? props.apply.preview.value.preview : null,
);
const missing = computed(() =>
  preview.value
    ? spaceMissingText(preview.value.bytes, preview.value.freeBytes, preview.value.reserveBytes)
    : null,
);
const blocked = computed(() => missing.value !== null);

function send(adminPassword: string | null): Promise<ActReport> {
  return props.apply.perform(props.batch, props.rows, adminPassword);
}

const act = useAdminAct({
  serverId: () => props.serverId,
  kind: () => "mods_apply",
  role: () => null,
  active: () => true,
  canSubmit: () => !blocked.value && props.rows.length > 0,
  blockedReason: () => missing.value ?? undefined,
  perform: send,
  refusalText: () => props.apply.refusal.value,
  onFinished: () => emit("launched"),
});
</script>

<template>
  <form
    class="ma"
    data-review
    data-mods-apply
    novalidate
    @submit.prevent="act.canSend.value && act.submit()"
  >
    <h3 class="ma__title">{{ t("mods.apply.title") }}</h3>
    <p class="ma__lead">{{ t("mods.apply.lead") }}</p>

    <div class="ma__grid">
      <div class="ma__col">
    <section v-if="blockers.length > 0" class="ma__block ma__block--alert" data-mods-apply-blockers>
      <h4 class="ma__subtitle">{{ t("mods.apply.blockersTitle") }}</h4>
      <Callout v-for="line in blockers" :key="line" tone="error" role="alert" data-mods-apply-blocker>{{ line }}</Callout>
    </section>

    <section class="ma__block" data-mods-apply-changes>
      <h4 class="ma__subtitle">{{ t("mods.apply.changes") }}</h4>
      <ul class="ma__list">
        <li v-for="row in rows" :key="row.key" class="ma__change" data-mods-apply-change>
          <span class="ma__sign" aria-hidden="true">{{ row.sign }}</span>
          <span class="ma__change-text">
            <strong>{{ row.title }}</strong>
            <span class="ma__detail">{{ row.detail }}</span>
          </span>
        </li>
      </ul>
    </section>

    <section class="ma__block" data-mods-apply-steps>
      <h4 class="ma__subtitle">{{ t("mods.apply.order") }}</h4>
      <p v-if="!running" class="ma__detail" data-mods-apply-stopped>{{ t("mods.apply.stopped") }}</p>
      <ol class="ma__list">
        <li v-for="(step, index) in plan" :key="step" class="ma__step" data-mods-apply-step>
          <span class="ma__num" aria-hidden="true">{{ index + 1 }}</span>
          <span class="ma__change-text">
            <strong>{{ stepTitle(step) }}</strong>
            <span class="ma__detail">{{ stepText(step, rows.length, playersKnown) }}</span>
          </span>
        </li>
      </ol>
    </section>
      </div>
      <div class="ma__col">
    <PlayersOnlineCard
      v-if="running"
      :state="players"
      :now="now"
      :refreshing="playersRefreshing"
      compact
      :link-label="consoleOpen ? t('players.openConsole') : undefined"
      @refresh="emit('refreshPlayers')"
      @open-console="emit('openConsole')"
    />
    <p v-if="running && playersKnown && players.kind === 'ready' && (players.view.online ?? 0) > 0" class="ma__detail" data-mods-apply-kick>
      {{ t(consoleOpen ? "mods.apply.kickConsole" : "mods.apply.kick") }}
    </p>

    <section class="ma__block" data-mods-apply-duration>
      <h4 class="ma__subtitle">{{ t("mods.apply.durationTitle") }}</h4>
      <HSkeleton v-if="apply.preview.value.kind === 'loading'" size="md" role="status" :aria-label="t('mods.apply.estimating')" />
      <p v-else-if="preview" class="ma__detail" data-mods-apply-size>
        {{ t("mods.apply.duration", { size: formatSize(preview.bytes) }) }}
      </p>
      <p v-else class="ma__detail" data-mods-apply-size>{{ t("mods.apply.durationBare") }}</p>
      <p class="ma__detail">{{ t("mods.apply.revert") }}</p>
      <Callout v-if="missing" tone="error" role="alert" data-mods-apply-nospace>{{ missing }}</Callout>
    </section>

    <section class="ma__block">
      <h4 class="ma__subtitle">{{ t("mods.apply.confirm") }}</h4>
      <AdminActPanel :act="act" :server-id="serverId" />
      <p class="ma__detail">{{ t("mods.apply.passwordHelp") }}</p>
      <Callout v-if="act.error.value" tone="error" role="alert" data-mods-apply-error>
        {{ act.error.value }}
      </Callout>
    
    <div class="ma__bar">
      <HButton variant="ghost" data-mods-apply-back @click="emit('back')">
        {{ t("mods.apply.backToList") }}
      </HButton>
      <HButton type="submit" :disabled="!act.canSend.value" :busy="act.sending.value" data-mods-apply-submit>
        {{ t(running ? "mods.apply.submit" : "mods.apply.submitStopped") }}
      </HButton>
    </div>
    </section>
      </div>
    </div>
    <ModsWarning class="ma__warning" />
  </form>
</template>

<style scoped>
.ma {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.ma__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.ma__lead,
.ma__detail,
.ma__warning {
  margin: 0;
  color: var(--tx-secondary);
}

.ma__warning {
  font-size: var(--fs-small);
}

.ma__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--space-4);
  align-items: start;
}

.ma__col {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

@container (max-width: 760px) {
  .ma__grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

.ma__block {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.ma__subtitle {
  margin: 0;
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.ma__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin: 0;
  padding: 0;
  list-style: none;
}

.ma__change,
.ma__step {
  display: flex;
  gap: var(--space-3);
}

.ma__sign,
.ma__num {
  flex: none;
  width: var(--icon-size);
  color: var(--ac);
  font-family: var(--font-mono);
  font-weight: var(--fw-bold);
  text-align: center;
}

.ma__change-text {
  display: flex;
  flex-direction: column;
  gap: var(--space-half);
  min-width: 0;
}

.ma__bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}
</style>
