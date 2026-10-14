<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from "vue";
import { RouterView } from "vue-router";
import BridgeDownBanner from "@/components/molecules/BridgeDownBanner.vue";
import ErrorBoundary from "@/components/molecules/ErrorBoundary.vue";
import ToastStack from "@/components/molecules/ToastStack.vue";
import FingerprintAlert from "@/components/organisms/FingerprintAlert.vue";
import ServerRail from "@/components/organisms/ServerRail.vue";
import UpdateBanner from "@/components/organisms/UpdateBanner.vue";
import { reportUiError } from "@/errors/report";
import { t } from "@/i18n";
import { failureMessage, failureOf } from "@/link";
import { useAgentUpdatesStore } from "@/stores/agentUpdates";
import { useLinkStore } from "@/stores/link";
import { useSecurityStore } from "@/stores/security";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";
import { useUpdatesStore } from "@/stores/updates";

// L'interface écoute le pont de liaison dès le démarrage (états des liens, issues d'opérations).
const link = useLinkStore();
const servers = useServersStore();
const toasts = useToastsStore();
link.start().catch((error) => reportUiError(error, "link:start"));
// Mise à jour du client : l'état vient de la coquille, le bandeau s'affiche seulement si elle le décide.
useUpdatesStore()
  .start()
  .catch((error) => reportUiError(error, "updates:start"));

// Mise à jour de l'agent de chaque serveur (HRT-17) : la progression arrive du flux, l'état se relit
// à chaque retour du lien.
useAgentUpdatesStore()
  .start()
  .catch((error) => reportUiError(error, "agent-update:start"));

// Sécurité de chaque serveur (HRT-26) : l'alerte et le mode attaque arrivent du flux, l'état se relit à
// chaque retour du lien ; la marque d'un serveur se lit dans la barre, quelle que soit la page.
useSecurityStore()
  .start()
  .catch((error) => reportUiError(error, "security:start"));

// Panneau de simulation : seulement en mode développement (retiré du binaire livré).
const DevLinkPanel = import.meta.env.DEV
  ? defineAsyncComponent(() => import("@/components/organisms/DevLinkPanel.vue"))
  : null;
const DevActionPanel = import.meta.env.DEV
  ? defineAsyncComponent(() => import("@/components/organisms/DevActionPanel.vue"))
  : null;

// Liste des serveurs illisible : message à l'écran et bouton pour réessayer.
const reloading = ref(false);
async function reload() {
  reloading.value = true;
  try {
    await servers.load();
  } catch {
    // Toujours en panne : le message reste affiché.
  } finally {
    reloading.value = false;
  }
}

// Lien d'évitement (HRT-141) : premier arrêt du clavier, il donne le focus à la zone principale de la page
// (le routeur est en mode « hash » : pas d'ancre, le focus est posé à la main).
function skipToContent() {
  const main = document.querySelector<HTMLElement>("main");
  if (!main) return;
  if (!main.hasAttribute("tabindex")) main.setAttribute("tabindex", "-1");
  main.focus();
}

// Alerte d'empreinte changée : celle du serveur affiché, sinon la première en attente.
const alert = computed(() => {
  const ids = [servers.currentId, ...servers.servers.map((server) => server.id)];
  for (const id of ids) {
    const change = id ? link.pendingAlert(id) : undefined;
    const server = id ? servers.byId(id) : undefined;
    if (change && server) return { change, server };
  }
  return null;
});

async function accept(serverId: string) {
  try {
    await link.acceptAlert(serverId);
  } catch (error) {
    const failure = failureOf(error);
    if (failure) toasts.push({ kind: "error", message: failureMessage(failure) });
    else reportUiError(error, "link:accept-fingerprint");
  }
}
</script>

<template>
  <div class="shell">
    <button type="button" class="skip" data-skip-link @click="skipToContent">
      {{ t("rail.skipToContent") }}
    </button>
    <ServerRail />
    <div class="shell__content">
      <UpdateBanner />
      <BridgeDownBanner v-if="servers.loadFailed" :busy="reloading" @retry="reload" />
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
  <FingerprintAlert
    v-if="alert"
    :key="`${alert.server.id}:${alert.change.presentedHex}`"
    :change="alert.change"
    :server-name="alert.server.name"
    @refuse="link.dismissAlert(alert.server.id)"
    @accept="accept(alert.server.id)"
  />
  <component :is="DevLinkPanel" v-if="DevLinkPanel" />
  <component :is="DevActionPanel" v-if="DevActionPanel" />
</template>

<style scoped>
.shell {
  display: flex;
  height: 100%;
}

/* Caché hors de l'écran tant qu'il n'a pas le focus ; visible, bien en vue, dès le premier appui sur Tab. */
.skip {
  position: fixed;
  top: var(--space-3);
  left: var(--space-3);
  z-index: var(--z-skip);
  padding: var(--space-2) var(--space-4);
  border: var(--border-width) solid var(--ac);
  border-radius: var(--radius-control);
  background: var(--card-2);
  color: var(--tx);
  cursor: pointer;
  transform: translateY(-200%);
}

.skip:focus-visible {
  transform: none;
}

.shell__content {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.shell__content > :last-child {
  flex: 1;
  min-height: 0;
}
</style>
