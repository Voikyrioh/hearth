<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HInput from "@/components/atoms/HInput.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import { t } from "@/i18n";
import { PUBLIC_ADDRESS_ACT_KIND } from "@/link";
import {
  publicAddressIssue,
  publicAddressIssueText,
  publicAddressRefusalText,
} from "@/services/game";

// La fenêtre « Adresse pour vos joueurs » (HRT-215, BR-JEU-022) : UNE fenêtre pour la saisie ET la confirmation du socle
// (`AdminActDialog`, acte `service_update`, JAMAIS couvert par l'élévation : le mot de passe est demandé à chaque fois,
// c'est la modification d'un service, BR-EDIT-017). L'adresse est facultative : le champ vide l'efface. Le client refuse un
// protocole, un port, un espace ou plus de 253 caractères avant d'envoyer ; l'agent juge ensuite la forme. L'adresse est
// gardée sans recréer le service ; elle n'est pas un secret mais n'entre dans aucune trace.
const props = defineProps<{
  open: boolean;
  serverId: string;
  /** L'adresse que l'agent garde, `null` : aucune. */
  current: string | null;
  perform: (address: string, adminPassword: string | null) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: [] }>();

const typed = ref("");
const touched = ref(false);
watch(
  () => props.open,
  (open) => {
    if (open) {
      typed.value = props.current ?? "";
      touched.value = false;
    }
  },
  { immediate: true },
);

const issue = computed(() => publicAddressIssue(typed.value));
const error = computed(() =>
  touched.value && issue.value !== null ? publicAddressIssueText(issue.value) : undefined,
);
/** Rien ne change : l'adresse saisie est celle que l'agent garde. */
const unchanged = computed(() => typed.value.trim() === (props.current ?? ""));
const canSubmit = computed(() => issue.value === null && !unchanged.value);

function send(adminPassword: string | null): Promise<ActReport> {
  return props.perform(typed.value.trim(), adminPassword);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="PUBLIC_ADDRESS_ACT_KIND"
    :title="t('game.address.editTitle')"
    :submit-label="t('game.address.editSubmit')"
    :can-submit="canSubmit"
    :perform="send"
    :refusal-text="publicAddressRefusalText"
    @close="emit('close')"
  >
    <HInput
      v-model="typed"
      data-public-address-input
      :label="t('game.address.editLabel')"
      :help="t('game.address.editHelp')"
      :placeholder="t('game.address.editPlaceholder')"
      :error="error"
      mono
      @blur="touched = true"
    />
  </AdminActDialog>
</template>
