<script setup lang="ts">
import { computed, onBeforeUnmount, watch } from "vue";
import { RouterView, useRoute } from "vue-router";
import ErrorBoundary from "@/components/molecules/ErrorBoundary.vue";
import AppHeader from "@/components/organisms/AppHeader.vue";
import OfflineBanner from "@/components/organisms/OfflineBanner.vue";
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
        @retry="link.retryNow(server.id)"
      />
      <div class="layout__content">
        <ErrorBoundary :reset-key="route.fullPath"><RouterView /></ErrorBoundary>
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
