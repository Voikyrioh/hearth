<script setup lang="ts">
import { computed } from "vue";
import { type MessageKey, t } from "@/i18n";
import type { LinkState } from "@/link";

// Pastille d'état du lien : point de couleur + libellé exact (la couleur ne porte jamais
// le sens seule). Reconnexion = clignotement doux, coupé si le mouvement est réduit.
const props = defineProps<{ state: LinkState }>();

const LABELS: Record<LinkState, MessageKey> = {
  connected: "link.connected",
  reconnecting: "link.reconnecting",
  offline: "link.offline",
  session_expired: "link.sessionExpired",
  access_revoked: "link.accessRevoked",
};

const label = computed(() => t(LABELS[props.state]));
</script>

<template>
  <span :class="['pill', `pill--${state}`]" role="status" :data-state="state">
    <i class="pill__dot" aria-hidden="true" />
    <span class="pill__label">{{ label }}</span>
  </span>
</template>

<style scoped>
.pill {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  padding: 6px var(--space-3);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-pill);
  background: var(--card);
  font-weight: 500;
  white-space: nowrap;
}

.pill__dot {
  width: var(--pill-dot);
  height: var(--pill-dot);
  border-radius: 50%;
  background: var(--ok);
}

.pill--reconnecting .pill__dot {
  background: var(--warn);
  animation: pill-blink var(--motion-pulse) ease-in-out infinite;
}

.pill--offline .pill__dot,
.pill--access_revoked .pill__dot {
  background: var(--crit);
}

.pill--session_expired .pill__dot {
  background: var(--warn);
}

@keyframes pill-blink {
  50% {
    opacity: var(--opacity-pulse-low);
  }
}
</style>
