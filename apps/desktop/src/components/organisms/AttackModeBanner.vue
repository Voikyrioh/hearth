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
// FIX:01M4VR6Y4Q4AVQVT9FCME7Z2SB : quand l'état du mode attaque devient illisible pour ce client (valeur inconnue,
// agent plus récent), le bandeau ne disparaît pas : il devient NEUTRE (« état de protection illisible »), au
// même endroit, sans rien affirmer (ni attaque ni calme) et sans bouton d'action sur le mode (BR-COMPAT-001 point 4).
const props = defineProps<{
  suspended: boolean;
  /** L'état du mode attaque est inconnu de ce client : bandeau neutre. */
  unknown?: boolean;
  /** Minutes avant la reprise (0 : moins d'une minute) ; seulement suspendu. */
  minutes: number | null;
  stamp?: string;
  onPage?: boolean;
}>();

defineEmits<{ open: [] }>();

const text = computed(() => {
  if (props.unknown) return t("security.modeTextUnknown");
  if (!props.suspended) return t("security.modeText");
  return props.minutes === null || props.minutes === 0
    ? t("security.bannerSuspendedSoon")
    : t("security.bannerSuspended", { n: props.minutes });
});
</script>

<template>
  <SecurityBanner
    :tone="unknown ? 'neutral' : suspended ? 'suspended' : 'attack'"
    :title="
      t(unknown ? 'security.modeUnknownTitle' : suspended ? 'security.modeSuspendedTitle' : 'security.modeActive')
    "
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
