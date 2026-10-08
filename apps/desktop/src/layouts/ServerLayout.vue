<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";
import { RouterView, useRoute, useRouter } from "vue-router";
import ErrorBoundary from "@/components/molecules/ErrorBoundary.vue";
import StaleSurface from "@/components/molecules/StaleSurface.vue";
import AppHeader from "@/components/organisms/AppHeader.vue";
import AttackModeBanner from "@/components/organisms/AttackModeBanner.vue";
import AttackModeDialog from "@/components/organisms/AttackModeDialog.vue";
import OfflineBanner from "@/components/organisms/OfflineBanner.vue";
import ReconnectPanel from "@/components/organisms/ReconnectPanel.vue";
import SecurityAlertBanner from "@/components/organisms/SecurityAlertBanner.vue";
import ServerNav from "@/components/organisms/ServerNav.vue";
import { formatClock } from "@/composables/format";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { providePageData } from "@/composables/usePageData";
import { useResumeMinutes } from "@/composables/useResumeMinutes";
import { t } from "@/i18n";
import { alertVisible, modeOn } from "@/link";
import { attackModeBlock } from "@/security/gate";
import { useLinkStore } from "@/stores/link";
import { useSecurityStore } from "@/stores/security";
import { useServersStore } from "@/stores/servers";
import { useUpdatesStore } from "@/stores/updates";

// Gabarit d'un serveur : navigation à gauche ; à droite en-tête (titre + état du lien, hors
// de la frontière d'erreur : toujours visible), bandeau hors ligne, bandeaux de sécurité (mode attaque
// puis alerte, HORS de `StaleSurface` : une alerte ne s'estompe pas quand le lien tombe, HRT-26), puis
// la page dans sa frontière d'erreur. Les pages mettent leurs actions dans l'en-tête avec
// `<Teleport defer to="#header-actions">`.
const route = useRoute();
const router = useRouter();
const servers = useServersStore();
const security = useSecurityStore();
const link = useLinkStore();
const updates = useUpdatesStore();
const { server, state, lastContactAt } = useCurrentServer();
const pageHasData = providePageData();
// FIX:01M4E5D5GCK02241BMJ1PRGPPN (C48) : pendant la reconnexion (3 à 30 s) la page garde son apparence : « pastille Reconnexion…, rien d'autre »
// (BR-RESIL-003). Elle ne s'estompe et ne se date qu'à partir de « Hors ligne », session expirée ou accès révoqué.
const stale = computed(() => state.value !== "connected" && state.value !== "reconnecting");
const event = computed(() => (server.value ? link.eventOf(server.value.id) : undefined));

watch(
  () => route.params.id,
  (id) => servers.setCurrent(typeof id === "string" ? id : null),
  { immediate: true },
);
onBeforeUnmount(() => servers.setCurrent(null));

const title = computed(() => (route.meta.title ? t(route.meta.title) : ""));

// Sécurité : l'état connu du serveur affiché (le bandeau garde ce qu'il sait quand le lien tombe).
const securityEntry = computed(() => (server.value ? security.of(server.value.id) : undefined));
const securityState = computed(() => securityEntry.value?.state ?? null);
const block = computed(() => attackModeBlock(server.value?.role, securityEntry.value));
const minutes = useResumeMinutes(() => securityEntry.value);
const stamp = computed(() =>
  state.value !== "connected" && securityEntry.value?.at
    ? t("security.lastKnown", { time: formatClock(securityEntry.value.at) })
    : undefined,
);
const onSecurityPage = computed(() => route.name === "security");
const askedHere = computed(() => security.dialog?.serverId === server.value?.id);

function openSecurityPage() {
  if (server.value) void router.push({ name: "security", params: { id: server.value.id } });
}
</script>

<template>
  <div v-if="server" class="layout">
    <ServerNav :server="server" />
    <div class="layout__main">
      <AppHeader :title="title" :server-id="server.id">
        <template #actions><div id="header-actions" class="layout__actions" /></template>
      </AppHeader>
      <OfflineBanner
        v-if="state === 'offline'"
        :last-contact-at="lastContactAt"
        :blocked="event?.blocked ?? null"
        :role="server.role"
        :client-action="updates.clientAction"
        :client-busy="updates.busy || updates.checking"
        @retry="link.retryNow(server.id)"
        @alert="link.reopenAlert(server.id)"
        @client="updates.fixClient()"
      />
      <AttackModeBanner
        v-if="securityState && modeOn(securityState.attackMode)"
        :suspended="securityState.attackMode.state === 'suspended'"
        :minutes="minutes"
        :stamp="stamp"
        :on-page="onSecurityPage"
        @open="openSecurityPage"
      />
      <SecurityAlertBanner
        v-if="securityState && alertVisible(securityState.alert)"
        :alert="securityState.alert"
        :role="server.role"
        :block="block"
        :stamp="stamp"
        :on-page="onSecurityPage"
        :mode-on="modeOn(securityState.attackMode)"
        @activate="security.ask(server.id, true)"
        @details="openSecurityPage"
      />
      <AttackModeDialog
        :open="askedHere"
        :server-id="server.id"
        :active="security.dialog?.active ?? true"
        @close="security.closeDialog()"
      />
      <div class="layout__content">
        <!-- Sans session : le formulaire de connexion, EN SURIMPRESSION de la dernière vue (périmée), premier dans
             le DOM (atteint en premier au clavier), au-dessus par `z-index`. -->
        <ReconnectPanel
          v-if="state === 'session_expired' || state === 'access_revoked'"
          :key="server.id"
          :server="server"
          :reason="event?.reason ?? null"
          :revoked="state === 'access_revoked'"
        />
        <!-- Données périmées (BR-RESIL-007) : c'est le GABARIT qui désature et date la page, pour
             toute page présente et à venir ; une page ne l'enveloppe pas elle-même. -->
        <StaleSurface :stale="stale" :last-contact-at="lastContactAt" :stamped="pageHasData">
          <ErrorBoundary :reset-key="route.fullPath"><RouterView /></ErrorBoundary>
        </StaleSurface>
      </div>
    </div>
  </div>
</template>

<style scoped>
.layout {
  display: flex;
  height: 100%;
}

.layout__main {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
  padding: var(--page-pad);
}

.layout__actions {
  display: flex;
  gap: var(--space-3);
}

.layout__content {
  position: relative;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow-y: auto;
}
</style>
