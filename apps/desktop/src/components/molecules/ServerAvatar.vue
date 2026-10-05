<script setup lang="ts">
import { computed } from "vue";
import { initials } from "@/composables/format";
import { type MessageKey, t } from "@/i18n";
import type { LinkState, ServerColor } from "@/link";

// Avatar rond d'un serveur : initiales, anneau de la couleur du serveur quand il est
// actif, pastille d'état en bas à droite. Le nom accessible porte l'état (pas la couleur).
const props = withDefaults(
  defineProps<{ name: string; color: ServerColor; state: LinkState; active?: boolean }>(),
  { active: false },
);

const STATE_LABELS: Record<LinkState, MessageKey> = {
  connected: "link.connected",
  reconnecting: "link.reconnecting",
  offline: "link.offline",
  session_expired: "link.sessionExpired",
  access_revoked: "link.accessRevoked",
};

const text = computed(() => initials(props.name));
const label = computed(() =>
  t("server.avatarLabel", { name: props.name, state: t(STATE_LABELS[props.state]) }),
);
</script>

<template>
  <span
    :class="['avatar', `avatar--c${color}`, { 'avatar--active': active }]"
    role="img"
    :aria-label="label"
  >
    <span class="avatar__initials" aria-hidden="true">{{ text }}</span>
    <i :class="['avatar__state', `avatar__state--${state}`]" aria-hidden="true" />
  </span>
</template>

<style scoped>
.avatar {
  position: relative;
  display: grid;
  place-items: center;
  width: var(--avatar-size);
  height: var(--avatar-size);
  border: var(--border-width) solid var(--bd);
  border-radius: 50%;
  background: var(--card);
  color: var(--tx2);
  font-family: var(--font-title);
  font-size: var(--fs-small);
  font-weight: 600;
}

.avatar--active {
  color: var(--tx);
  box-shadow:
    0 0 0 var(--ring-width) var(--bg),
    0 0 0 calc(var(--ring-width) * 2) var(--ring);
}

.avatar--c1 { --ring: var(--server-1); }
.avatar--c2 { --ring: var(--server-2); }
.avatar--c3 { --ring: var(--server-3); }
.avatar--c4 { --ring: var(--server-4); }
.avatar--c5 { --ring: var(--server-5); }
.avatar--c6 { --ring: var(--server-6); }
.avatar--c7 { --ring: var(--server-7); }
.avatar--c8 { --ring: var(--server-8); }

.avatar__state {
  position: absolute;
  right: calc(var(--border-width) * -1);
  bottom: calc(var(--border-width) * -1);
  width: var(--status-dot);
  height: var(--status-dot);
  border: var(--ring-width) solid var(--side);
  border-radius: 50%;
  background: var(--ok);
}

.avatar__state--reconnecting,
.avatar__state--session_expired {
  background: var(--warn);
}

.avatar__state--offline,
.avatar__state--access_revoked {
  background: var(--crit);
}
</style>
