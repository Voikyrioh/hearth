<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import SecurityBanner from "@/components/molecules/SecurityBanner.vue";
import { t } from "@/i18n";
import type { Role, SecurityAlert } from "@/link";
import { type AttackModeBlock, blockMessageKey } from "@/security/gate";

// Bandeau « Attaque probable détectée » (BR-TRUST-008, 009), visible sur toutes les pages du serveur
// tant que l'alerte dure, non fermable (il informe d'un état du serveur). Le texte suit le rôle : le
// titulaire voit « ton identifiant » ; un administrateur voit en plus le NOMBRE d'autres comptes visés
// (jamais leurs noms) ; un compte Lecture seule voit l'alerte mais son bouton est grisé et dit pourquoi.
// « Activer le mode attaque » ouvre la confirmation directement ; la raison d'indisponibilité (poste
// sans clé enregistrée, agent trop ancien) est dite dans l'infobulle du bouton.
const props = defineProps<{
  alert: SecurityAlert;
  role: Role;
  block: AttackModeBlock | null;
  /** « Dernier état connu à {heure} » quand le lien n'est pas « Connecté ». */
  stamp?: string;
  /** Sur la page Sécurité : « Plus d'infos » n'a plus de sens. */
  onPage?: boolean;
  /** Le mode attaque est déjà actif ou suspendu : activer de nouveau n'a pas de sens (on désactive depuis la page). */
  modeOn?: boolean;
}>();

defineEmits<{ activate: []; details: [] }>();

const others = computed(() => props.alert.others ?? 0);

const text = computed(() => {
  if (props.role !== "admin") return t("security.alertReadonly");
  if (props.alert.own) {
    if (others.value === 0) return t("security.alertOwn");
    return others.value === 1
      ? t("security.alertOwnAndOne")
      : t("security.alertOwnAndMany", { n: others.value });
  }
  return others.value === 1
    ? t("security.alertOthersOne")
    : t("security.alertOthersMany", { n: others.value });
});

// Lecture seule : la phrase propre à l'alerte ; sinon la raison commune du panneau.
const reason = computed(() => {
  if (props.block === "readonly") return t("security.alertReadonlyHint");
  const key = blockMessageKey(props.block);
  return key ? t(key) : undefined;
});
</script>

<template>
  <SecurityBanner tone="alert" :title="t('security.alertTitle')" :stamp="stamp" data-security-alert>
    {{ text }}
    <template #actions>
      <HButton
        v-if="!modeOn && !onPage"
        variant="secondary"
        size="sm"
        needs-link
        :disabled="block !== null"
        :hint="reason"
        data-security-activate
        @click="$emit('activate')"
      >
        {{ t("security.activate") }}
      </HButton>
      <HButton v-if="!onPage" variant="secondary" size="sm" data-security-details @click="$emit('details')">
        {{ t("security.moreInfo") }}
      </HButton>
    </template>
  </SecurityBanner>
</template>
