<script setup lang="ts">
import { computed, ref } from "vue";
import { historyMessage, progressSentence, refusalMessage } from "@/agentUpdate/messages";
import HButton from "@/components/atoms/HButton.vue";
import HTag from "@/components/atoms/HTag.vue";
import AgentUpdateSteps from "@/components/molecules/AgentUpdateSteps.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import LinkStatePill from "@/components/molecules/LinkStatePill.vue";
import { formatAgo } from "@/composables/formatAgo";
import { useNow } from "@/composables/useNow";
import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import { getLinkBridge, type ServerInfo } from "@/link";
import { useAgentUpdatesStore } from "@/stores/agentUpdates";
import { useLinkStore } from "@/stores/link";
import { useSettingsStore } from "@/stores/settings";
import { useToastsStore } from "@/stores/toasts";

// « État du serveur : {nom} » (réglages) : versions du client et de l'agent (BR-UPDATE-022, 025), la
// mention « Mise à jour disponible », le bouton « Mettre à jour l'agent » (administrateur ; désactivé
// avec son explication pour le rôle Lecture seule), la confirmation, les étapes une à une avec le
// pourcentage du téléchargement (BR-UPDATE-013), le résultat (BR-UPDATE-015, 017, 019) et, si
// l'installation est gérée par le système, l'explication sans bouton. Le lien « Reconnexion… » pendant
// le redémarrage n'est pas une erreur (BR-UPDATE-014) : la carte garde ses étapes et le dit.
// L'interface ne fournit NI adresse, NI signature, NI somme : elle ne transmet que le numéro de la
// version qu'elle montre ; l'agent est l'arbitre (rôle, signature, somme).
const props = defineProps<{ server: ServerInfo }>();

const agents = useAgentUpdatesStore();
const link = useLinkStore();
const settings = useSettingsStore();
const toasts = useToastsStore();
const action = useServerAction();
const now = useNow();

const confirming = ref(false);

const state = computed(() => link.stateOf(props.server.id));
const blocked = computed(() => link.eventOf(props.server.id)?.blocked ?? null);
const entry = computed(() => agents.entry(props.server.id));
const view = computed(() => entry.value.view);
const isAdmin = computed(() => props.server.role === "admin");
const progress = computed(() => agents.progressOf(props.server.id));
const running = computed(() => agents.runningOf(props.server.id));
const notice = computed(() => agents.noticeOf(props.server.id));
const available = computed(() => view.value?.available ?? null);

/** L'étape à montrer : celle du flux ; « contrôle » quand l'agent dit seulement « en cours ». */
const step = computed(() => {
  const current = progress.value?.step;
  return current && current !== "done" ? current : "check";
});

const compat = computed(() =>
  blocked.value === "incompatible_agent" || blocked.value === "incompatible_client"
    ? blocked.value
    : null,
);
const compatMessage = computed(() => {
  if (compat.value === "incompatible_client") return t("link.clientTooOld");
  return t(isAdmin.value ? "link.agentTooOld" : "link.agentTooOldReadonly");
});

const versions = computed(() => {
  const params = {
    client: settings.version ?? t("settings.versionUnavailable"),
    agent: view.value?.current ?? t("settings.versionUnavailable"),
  };
  return available.value
    ? t("agentUpdate.versionsAvailable", { ...params, available: available.value.version })
    : t("agentUpdate.versions", params);
});
/** Le serveur ne répond plus en pleine mise à jour : la carte ne dit pas ce qu'elle ne sait plus. */
const silent = computed(() => state.value === "offline");

/** Le bouton existe quand une version plus récente est disponible ; il est inerte sans droit ou pendant l'opération. */
const showButton = computed(() => available.value !== null && view.value?.managed === false);
const hint = computed(() => (isAdmin.value ? undefined : t("agentUpdate.readOnlyHint")));

const history = computed(() => {
  const last = view.value?.last;
  if (!last || notice.value || running.value || last.recent) return null;
  const at = Date.parse(last.at);
  return historyMessage(last, Number.isNaN(at) ? "" : formatAgo(at, now.value));
});

async function confirm() {
  const version = available.value?.version;
  if (!version) {
    confirming.value = false;
    return;
  }
  const result = await action.run(() => getLinkBridge().updateAgent(props.server.id, version), {
    unknownMessage: "agentUpdate.unknownResult",
    forbiddenMessage: "agentUpdate.forbidden",
  });
  confirming.value = false;
  if (!result) {
    // Échec (lien coupé, refus de rôle de l'agent) : déjà notifié ; l'état se relit.
    void agents.refresh(props.server.id);
    return;
  }
  if (result.kind === "accepted") {
    agents.begin(props.server.id, result.version);
  } else if (result.kind === "refused") {
    toasts.push({
      kind: result.refusal.kind === "in_progress" ? "warn" : "error",
      message: refusalMessage(result.refusal),
    });
    void agents.refresh(props.server.id);
  }
  // `unknown` : déjà dit par `useServerAction`, jamais rejoué ; l'état se relit au retour du lien.
}
</script>

<template>
  <section class="agent" :data-server="server.id" data-agent-card>
    <header class="agent__head">
      <h3 class="agent__title">{{ t("agentUpdate.cardTitle", { name: server.name }) }}</h3>
      <HTag v-if="available && !running" tone="accent" data-agent-tag>{{ t("agentUpdate.tag") }}</HTag>
      <LinkStatePill :state="state" />
    </header>
    <p class="agent__versions" data-agent-versions>{{ versions }}</p>
    <p v-if="available && !running && !compat" class="agent__available" data-agent-available>
      {{ t("agentUpdate.availableText") }}
    </p>

    <p v-if="compat" class="agent__incompat" role="alert" data-agent-incompat>{{ compatMessage }}</p>
    <p v-else-if="view?.managed" class="agent__note" data-agent-managed>
      {{ t("agentUpdate.managed") }}
    </p>
    <p v-else-if="state !== 'connected' && !view && !running" class="agent__note" data-agent-unavailable>
      {{ t("agentUpdate.unavailable") }}
    </p>
    <p v-else-if="entry.status === 'error' && !view && state === 'connected'" class="agent__note" role="alert">
      {{ t("agentUpdate.readFailed") }}
    </p>
    <p
      v-else-if="view && !available && !running && !notice && !history && view.last === null"
      class="agent__note"
      data-agent-uptodate
    >
      {{ t("agentUpdate.upToDate") }}
    </p>

    <div v-if="running" class="agent__progress" data-agent-running>
      <p v-if="silent" class="agent__sentence" role="status" data-agent-lost>
        {{ t("agentUpdate.runningLost") }}
      </p>
      <template v-else>
        <p class="agent__sentence" role="status" data-agent-progress>
          {{ progressSentence(step, progress?.percent ?? null) }}
        </p>
        <AgentUpdateSteps :step="step" :percent="progress?.percent ?? null" />
        <p class="agent__cut">{{ t("agentUpdate.cutAnnounce") }}</p>
      </template>
    </div>

    <div
      v-if="notice"
      :class="['agent__result', `agent__result--${notice.tone}`]"
      role="status"
      data-agent-result
      :data-tone="notice.tone"
    >
      <p class="agent__result-text">{{ notice.message }}</p>
      <HButton variant="ghost" size="sm" @click="agents.dismiss(server.id)">
        {{ t("agentUpdate.dismiss") }}
      </HButton>
    </div>
    <p v-else-if="history" class="agent__history" data-agent-history>{{ history }}</p>

    <div v-if="showButton" class="agent__actions">
      <HButton
        :needs-link="{ server: server.id }"
        :disabled="!isAdmin || running"
        :hint="hint"
        :busy="action.busy.value"
        data-agent-update-button
        @click="confirming = true"
      >
        {{ t("agentUpdate.button") }}
      </HButton>
    </div>

    <ConfirmDialog
      :open="confirming"
      :title="t('agentUpdate.confirmTitle')"
      :message="t('agentUpdate.confirmMessage')"
      :confirm-label="t('agentUpdate.confirmYes')"
      :busy="action.busy.value"
      @confirm="confirm"
      @cancel="confirming = false"
    />
  </section>
</template>

<style scoped>
.agent {
  margin-top: var(--space-5);
  padding: var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.agent__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}

.agent__title {
  flex: 1;
  min-width: 0;
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.agent__versions {
  margin-top: var(--space-3);
  font-family: var(--font-mono);
  font-variant-numeric: tabular-nums;
}

.agent__available,
.agent__note,
.agent__history {
  margin-top: var(--space-2);
  color: var(--tx2);
}

.agent__incompat {
  margin-top: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--crit);
  border-radius: var(--radius-control);
  background: var(--crit-tint);
}

.agent__progress {
  margin-top: var(--space-4);
}

.agent__sentence {
  margin-bottom: var(--space-3);
  font-weight: var(--fw-medium);
}

.agent__cut {
  margin-top: var(--space-3);
  color: var(--tx3);
}

.agent__result {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  margin-top: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
}

.agent__result-text {
  flex: 1;
  min-width: 0;
}

.agent__result--ok {
  border-color: var(--ok);
  background: var(--ok-tint);
}

.agent__result--warn {
  border-color: var(--warn);
  background: var(--warn-tint);
}

.agent__result--crit {
  border-color: var(--crit);
  background: var(--crit-tint);
}

.agent__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
  margin-top: var(--space-4);
}
</style>
