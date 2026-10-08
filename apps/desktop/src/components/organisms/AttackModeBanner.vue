<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import SecurityBanner from "@/components/molecules/SecurityBanner.vue";
import { t } from "@/i18n";

// Bandeau « Mode attaque actif » (BR-TRUST-010), permanent sur toutes les pages du serveur tant que le
// mode est actif ou suspendu, non fermable. Le sens est écrit en clair (jamais seulement dans une
// infobulle). Variante suspendue : le serveur a redémarré, le mode reprend dans N minutes
// (BR-TRUST-020). Aucun bouton de désactivation ici : on désactive depuis la page Sécurité, où la
// confirmation et les raisons d'indisponibilité sont au même endroit.
const props = defineProps<{
  suspended: boolean;
  /** Minutes avant la reprise (0 : moins d'une minute) ; seulement suspendu. */
  minutes: number | null;
  stamp?: string;
  onPage?: boolean;
}>();

defineEmits<{ open: [] }>();

const text = computed(() => {
  if (!props.suspended) return t("security.modeText");
  return props.minutes === null || props.minutes === 0
    ? t("security.bannerSuspendedSoon")
    : t("security.bannerSuspended", { n: props.minutes });
});
</script>

<template>
  <SecurityBanner
    :tone="suspended ? 'suspended' : 'attack'"
    :title="t(suspended ? 'security.modeSuspendedTitle' : 'security.modeActive')"
    :stamp="stamp"
    data-attack-mode-banner
  >
    {{ text }}
    <template #actions>
      <HButton v-if="!onPage" variant="secondary" size="sm" data-security-open @click="$emit('open')">
        {{ t("security.seePage") }}
      </HButton>
    </template>
  </SecurityBanner>
</template>
