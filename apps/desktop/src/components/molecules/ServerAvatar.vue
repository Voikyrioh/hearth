<script setup lang="ts">
import { computed } from "vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { initials } from "@/composables/format";
import { type MessageKey, t } from "@/i18n";
import type { LinkState, ServerColor } from "@/link";
import type { SecurityMark } from "@/security/mark";

// Avatar rond d'un serveur : initiales, anneau de la couleur du serveur quand il est
// actif, pastille d'état en bas à droite. Le nom accessible porte l'état (pas la couleur).
const props = withDefaults(
  defineProps<{
    name: string;
    color: ServerColor;
    state: LinkState;
    active?: boolean;
    /** Marque de sécurité (HRT-26) : mode attaque, suspendu ou alerte ; jamais la couleur seule. */
    mark?: SecurityMark | null;
  }>(),
  { active: false, mark: null },
);

const MARK_LABELS: Record<SecurityMark, MessageKey> = {
  attack: "security.markAttack",
  suspended: "security.markSuspended",
  alert: "security.markAlert",
};
const MARK_ICONS = { attack: "shield", suspended: "clock", alert: "alert" } as const;

const STATE_LABELS: Record<LinkState, MessageKey> = {
  connected: "link.connected",
  reconnecting: "link.reconnecting",
  offline: "link.offline",
  session_expired: "link.sessionExpired",
  access_revoked: "link.accessRevoked",
};

const text = computed(() => initials(props.name));
const label = computed(() =>
  props.mark
    ? t("server.avatarLabelMarked", {
        name: props.name,
        state: t(STATE_LABELS[props.state]),
        mark: t(MARK_LABELS[props.mark]),
      })
    : t("server.avatarLabel", { name: props.name, state: t(STATE_LABELS[props.state]) }),
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
    <span v-if="mark" :class="['avatar__mark', `avatar__mark--${mark}`]" :data-mark="mark" aria-hidden="true">
      <HIcon :name="MARK_ICONS[mark]" size="sm" />
    </span>
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
  font-weight: var(--fw-semibold);
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

.avatar__mark {
  position: absolute;
  /* FIX:01M4DJZAXMGAXW5PPQBMQVC96Y : la marque dépasse du coin, elle ne recouvre plus les initiales (C38) */
  top: var(--mark-offset);
  right: var(--mark-offset);
  display: grid;
  place-items: center;
  width: var(--mark-size);
  height: var(--mark-size);
  border-radius: 50%;
  background: var(--card);
  box-shadow: 0 0 0 var(--ring-width) var(--side);
}

.avatar__mark :deep(svg) {
  width: var(--mark-icon);
  height: var(--mark-icon);
}

.avatar__mark--attack {
  color: var(--ac2);
}

.avatar__mark--suspended {
  color: var(--cool);
}

.avatar__mark--alert {
  color: var(--warn);
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
