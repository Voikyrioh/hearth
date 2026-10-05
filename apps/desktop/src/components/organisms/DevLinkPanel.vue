<script setup lang="ts">
import HButton from "@/components/atoms/HButton.vue";
import { type MessageKey, t } from "@/i18n";
import { getLinkBridge, LINK_STATES, type LinkState, SimulatedLinkBridge } from "@/link";
import { useServersStore } from "@/stores/servers";

// Panneau de développement : pilote le pont simulé (états du lien, issues d'opérations).
// Chargé seulement en mode développement et seulement dans un navigateur : jamais dans le
// binaire livré (App.vue l'importe derrière `import.meta.env.DEV`). `?nodev` le masque.
// Outil de développement : ses libellés restent ici (hors `fr.ts`) pour ne pas entrer dans le bundle livré.
const TITLE = "Simulation du lien";
const bridge = getLinkBridge();
const sim = bridge instanceof SimulatedLinkBridge ? bridge : null;
const hidden = new URLSearchParams(window.location.search).has("nodev");
const servers = useServersStore();

const LABELS: Record<LinkState, MessageKey> = {
  connected: "link.connected",
  reconnecting: "link.reconnecting",
  offline: "link.offline",
  session_expired: "link.sessionExpired",
  access_revoked: "link.accessRevoked",
};
</script>

<template>
  <details v-if="sim && !hidden" class="dev">
    <summary class="dev__title">{{ TITLE }}</summary>
    <div v-for="server in servers.servers" :key="server.id" class="dev__server">
      <span class="dev__name">{{ server.name }}</span>
      <div class="dev__buttons">
        <HButton
          v-for="state in LINK_STATES"
          :key="state"
          variant="secondary"
          size="sm"
          :data-sim="`${server.id}:${state}`"
          @click="sim.setState(server.id, state)"
        >
          {{ t(LABELS[state]) }}
        </HButton>
      </div>
    </div>
    <div class="dev__buttons">
      <HButton variant="ghost" size="sm" @click="sim.emitOperation({ opId: `sim${Date.now()}`, serverId: servers.current?.id ?? servers.first?.id ?? '', outcome: 'done' })">
        {{ t("operation.done") }}
      </HButton>
      <HButton variant="ghost" size="sm" @click="sim.emitOperation({ opId: `sim${Date.now()}`, serverId: servers.current?.id ?? servers.first?.id ?? '', outcome: 'unknown' })">
        {{ t("operation.unknown") }}
      </HButton>
    </div>
  </details>
</template>

<style scoped>
.dev {
  position: fixed;
  bottom: var(--space-3);
  left: calc(var(--rail-width) + var(--space-3));
  z-index: var(--z-dev);
  max-width: calc(100vw - var(--rail-width) - var(--toast-width) - var(--page-pad) * 3);
  padding: var(--space-2) var(--space-3);
  border: var(--border-width) dashed var(--bd);
  border-radius: var(--radius-control);
  background: var(--card);
  font-size: var(--fs-small);
}

.dev__title {
  cursor: pointer;
  color: var(--tx2);
}

.dev__server {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin-top: var(--space-2);
}

.dev__name {
  font-family: var(--font-mono);
}

.dev__buttons {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  margin-top: var(--space-1);
}
</style>
