<script setup lang="ts">
import { defineAsyncComponent } from "vue";
import { RouterView } from "vue-router";
import ErrorBoundary from "@/components/molecules/ErrorBoundary.vue";
import ToastStack from "@/components/molecules/ToastStack.vue";
import ServerRail from "@/components/organisms/ServerRail.vue";
import { reportUiError } from "@/errors/report";
import { useLinkStore } from "@/stores/link";

// L'interface écoute le pont de liaison dès le démarrage (états des liens, issues d'opérations).
useLinkStore()
  .start()
  .catch((error) => reportUiError(error, "link:start"));

// Panneau de simulation : seulement en mode développement (retiré du binaire livré).
const DevLinkPanel = import.meta.env.DEV
  ? defineAsyncComponent(() => import("@/components/organisms/DevLinkPanel.vue"))
  : null;
</script>

<template>
  <div class="shell">
    <ServerRail />
    <div class="shell__content">
      <!-- La coquille (barre des serveurs, ci-dessus) n'est dans aucune frontière. Les vues d'un serveur
           ont leur propre frontière DANS le gabarit, sous l'en-tête : navigation, pastille du lien et
           bandeau ne sont jamais remplacés. -->
      <RouterView v-slot="{ Component, route: view }">
        <component :is="Component" v-if="view.matched.some((r) => r.meta.ownBoundary)" />
        <ErrorBoundary v-else :reset-key="view.fullPath"><component :is="Component" /></ErrorBoundary>
      </RouterView>
    </div>
  </div>
  <ToastStack />
  <component :is="DevLinkPanel" v-if="DevLinkPanel" />
</template>

<style scoped>
.shell {
  display: flex;
  height: 100%;
}

.shell__content {
  flex: 1;
  min-width: 0;
}
</style>
