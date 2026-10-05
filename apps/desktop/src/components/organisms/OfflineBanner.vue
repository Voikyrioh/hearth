<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { formatClock } from "@/composables/format";
import { t } from "@/i18n";
import type { Blocked } from "@/link";

// Bandeau « Hors ligne » (BR-RESIL-004) : heure du dernier contact, nouvelle tentative
// automatique, bouton pour réessayer tout de suite. Jamais modal. Quand les tentatives sont
// suspendues (`blocked`), il dit pourquoi : identité du serveur changée (bouton « Voir l'alerte »),
// agent ou client trop ancien (BR-CONN-003, 014).
const props = withDefaults(
  defineProps<{ lastContactAt: number | null; blocked?: Blocked | null }>(),
  { blocked: null },
);

defineEmits<{ retry: []; alert: [] }>();

const text = computed(() => {
  if (props.blocked === "fingerprint_changed") return t("link.fingerprintBanner");
  if (props.blocked === "incompatible_agent") return t("link.agentTooOld");
  if (props.blocked === "incompatible_client") return t("link.clientTooOld");
  return props.lastContactAt === null
    ? t("link.offlineBannerNoContact")
    : t("link.offlineBanner", { time: formatClock(props.lastContactAt) });
});
</script>

<template>
  <div class="banner" role="status">
    <HIcon class="banner__icon" name="alert" />
    <p class="banner__text">{{ text }}</p>
    <HButton v-if="blocked === 'fingerprint_changed'" variant="secondary" size="sm" @click="$emit('alert')">
      {{ t("link.seeAlert") }}
    </HButton>
    <HButton v-else variant="secondary" size="sm" @click="$emit('retry')">
      <HIcon name="refresh" size="sm" />
      {{ t("link.retryNow") }}
    </HButton>
  </div>
</template>

<style scoped>
.banner {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--crit);
  border-radius: var(--radius-control);
  background: var(--crit-tint);
}

.banner__icon {
  color: var(--crit);
}

.banner__text {
  flex: 1;
  min-width: 0;
}
</style>
