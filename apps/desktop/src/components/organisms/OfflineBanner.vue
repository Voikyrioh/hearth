<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import { formatClock } from "@/composables/format";
import { t } from "@/i18n";
import type { Blocked, Role } from "@/link";

// Bandeau « Hors ligne » (BR-RESIL-004) : heure du dernier contact, nouvelle tentative
// automatique, bouton pour réessayer tout de suite. Jamais modal. Quand les tentatives sont
// suspendues (`blocked`), il dit pourquoi : identité du serveur changée (bouton « Voir l'alerte »),
// agent ou client trop ancien (BR-CONN-003, 014). Versions incompatibles : le message dit lequel mettre à
// jour (BR-UPDATE-020) ; pour un compte Lecture seule, il demande à un administrateur (BR-UPDATE-021) ;
// client trop ancien : le bouton lance la mise à jour du CLIENT (HRT-16), ou la cherche s'il n'y en a
// pas encore d'annoncée. L'agent trop ancien ne peut pas être mis à jour depuis l'application : il
// refuse toute requête d'une version d'interface qu'il ne parle pas (BR-CONN-014), la demande ne
// passerait pas ; le message dit quoi faire.
const props = withDefaults(
  defineProps<{
    lastContactAt: number | null;
    blocked?: Blocked | null;
    role?: Role;
    /** Client trop ancien : « installer » (une version est annoncée) ou « chercher » (aucune pour l'instant). */
    clientAction?: "install" | "check" | null;
    clientBusy?: boolean;
  }>(),
  { blocked: null, role: "admin", clientAction: null, clientBusy: false },
);

defineEmits<{ retry: []; alert: []; client: [] }>();

const text = computed(() => {
  if (props.blocked === "fingerprint_changed") return t("link.fingerprintBanner");
  if (props.blocked === "incompatible_agent") {
    return t(props.role === "admin" ? "link.agentTooOld" : "link.agentTooOldReadonly");
  }
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
    <HButton
      v-else-if="blocked === 'incompatible_client' && clientAction === 'install'"
      variant="secondary"
      size="sm"
      :busy="clientBusy"
      data-update-client
      @click="$emit('client')"
    >
      {{ t("link.updateClient") }}
    </HButton>
    <HButton
      v-else-if="blocked === 'incompatible_client'"
      variant="secondary"
      size="sm"
      :busy="clientBusy"
      data-update-client
      @click="$emit('client')"
    >
      {{ t("link.checkClient") }}
    </HButton>
    <span v-else-if="blocked === 'incompatible_agent'" />
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
