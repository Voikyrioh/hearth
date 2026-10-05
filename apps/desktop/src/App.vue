<script setup lang="ts">
import { defineAsyncComponent } from "vue";
import { RouterView, useRoute } from "vue-router";
import ErrorBoundary from "@/components/molecules/ErrorBoundary.vue";
import ToastStack from "@/components/molecules/ToastStack.vue";
import ServerRail from "@/components/organisms/ServerRail.vue";
import { reportUiError } from "@/errors/report";
import { useLinkStore } from "@/stores/link";

const route = useRoute();

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
      <ErrorBoundary :reset-key="route.fullPath"><RouterView /></ErrorBoundary>
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
