<script setup lang="ts">
import { computed } from "vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import { useAttackModeActions } from "@/composables/useAttackModeActions";
import { t } from "@/i18n";
import { attackModeRefusalMessage } from "@/security/messages";

// Activation ou désactivation du mode attaque : un acte d'administration (Q14 point 3, Q16), confirmé par
// la fenêtre commune des actes (`AdminActDialog`) : le mot de passe actuel, jamais couvert par le délai
// de 5 minutes ; la preuve de la clé de ce PC est faite par la coquille, sans geste. Le bouton de
// confirmation n'est pas destructeur : activer n'est pas une destruction. Un refus s'affiche DANS la
// fenêtre ; le lien coupé avant la réponse ferme la fenêtre, le résultat inconnu est dit et l'action
// n'est jamais rejouée (BR-RESIL-009).
const props = defineProps<{ open: boolean; serverId: string; active: boolean }>();
const emit = defineEmits<{ close: [] }>();

const actions = useAttackModeActions(() => props.serverId);

const title = computed(() =>
  t(props.active ? "security.confirmOnTitle" : "security.confirmOffTitle"),
);
const message = computed(() =>
  t(props.active ? "security.confirmOnMessage" : "security.confirmOffMessage"),
);
const submitLabel = computed(() => t(props.active ? "security.activate" : "security.deactivate"));

// Jamais couvert : le champ est toujours là, le mot de passe n'est jamais `null`.
const perform = (adminPassword: string | null) => actions.change(props.active, adminPassword ?? "");
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="active ? 'attack_mode_enable' : 'attack_mode_disable'"
    :title="title"
    :submit-label="submitLabel"
    :perform="perform"
    :refusal-text="(refusal) => attackModeRefusalMessage(refusal as never)"
    @close="emit('close')"
  >
    <p class="attack__message">{{ message }}</p>
  </AdminActDialog>
</template>

<style scoped>
.attack__message {
  color: var(--tx);
}
</style>
