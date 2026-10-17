<script setup lang="ts">
import { computed } from "vue";
import HIcon, { type IconName } from "@/components/atoms/HIcon.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import { t } from "@/i18n";
import type { ServiceState } from "@/link";
import { STATE_KEYS } from "@/services/format";

// L'état d'un service : un pictogramme ET un mot, jamais la couleur seule (BR-SVC-004). La seule source
// d'icône et de ton d'un état. Les états de transition (Démarrage, Arrêt en cours, Déploiement en cours, Modification en cours)
// montrent l'arc d'attente, qui reste visible quand le mouvement est réduit.
const props = defineProps<{ state: ServiceState }>();

const ICONS: Record<ServiceState, IconName | "spinner"> = {
  running: "play",
  stopped: "stop",
  starting: "spinner",
  stopping: "spinner",
  error: "alert",
  restart_loop: "loop",
  paused: "pause",
  deploying: "spinner",
  updating: "spinner",
  unknown: "question",
};

const icon = computed(() => ICONS[props.state]);
</script>

<template>
  <span :class="['badge', `badge--${state}`]" :data-state="state">
    <HSpinner v-if="icon === 'spinner'" class="badge__icon" />
    <HIcon v-else class="badge__icon" :name="icon" size="sm" />
    <span class="badge__label">{{ t(STATE_KEYS[state]) }}</span>
  </span>
</template>

<style scoped>
.badge {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
  color: var(--tx);
  font-weight: var(--fw-medium);
}

.badge__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.badge--running {
  color: var(--ok);
}

.badge--stopped,
.badge--paused,
.badge--unknown {
  color: var(--tx-secondary);
}

.badge--error,
.badge--restart_loop {
  color: var(--crit);
}

.badge--starting .badge__icon,
.badge--stopping .badge__icon,
.badge--deploying .badge__icon,
.badge--updating .badge__icon {
  color: var(--ac);
}
</style>
