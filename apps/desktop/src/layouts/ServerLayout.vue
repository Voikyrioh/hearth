<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";
import { RouterView, useRoute } from "vue-router";
import ErrorBoundary from "@/components/molecules/ErrorBoundary.vue";
import StaleSurface from "@/components/molecules/StaleSurface.vue";
import AppHeader from "@/components/organisms/AppHeader.vue";
import OfflineBanner from "@/components/organisms/OfflineBanner.vue";
import ReconnectPanel from "@/components/organisms/ReconnectPanel.vue";
import ServerNav from "@/components/organisms/ServerNav.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { t } from "@/i18n";
import { useLinkStore } from "@/stores/link";
import { useServersStore } from "@/stores/servers";

// Gabarit d'un serveur : navigation à gauche ; à droite en-tête (titre + état du lien, hors
// de la frontière d'erreur : toujours visible), bandeau hors ligne, puis la page dans sa
// frontière d'erreur. Les pages mettent leurs actions dans l'en-tête avec
// `<Teleport defer to="#header-actions">`.
const route = useRoute();
const servers = useServersStore();
const link = useLinkStore();
const { server, state, lastContactAt } = useCurrentServer();
const event = computed(() => (server.value ? link.eventOf(server.value.id) : undefined));

watch(
  () => route.params.id,
  (id) => servers.setCurrent(typeof id === "string" ? id : null),
  { immediate: true },
);
onBeforeUnmount(() => servers.setCurrent(null));

const title = computed(() => (route.meta.title ? t(route.meta.title) : ""));
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
        @retry="link.retryNow(server.id)"
        @alert="link.reopenAlert(server.id)"
      />
      <div class="layout__content">
        <!-- Sans session : le formulaire de connexion, au-dessus de la dernière vue (périmée). -->
        <ReconnectPanel
          v-if="state === 'session_expired' || state === 'access_revoked'"
          :key="server.id"
          :server="server"
          :reason="event?.reason ?? null"
          :revoked="state === 'access_revoked'"
        />
        <!-- Données périmées (BR-RESIL-007) : c'est le GABARIT qui désature et date la page, pour
             toute page présente et à venir ; une page ne l'enveloppe pas elle-même. -->
        <StaleSurface :stale="state !== 'connected'" :last-contact-at="lastContactAt">
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
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}
</style>
