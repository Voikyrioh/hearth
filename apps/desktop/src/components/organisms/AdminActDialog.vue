<script setup lang="ts">
import { computed } from "vue";
import FormDialog from "@/components/molecules/FormDialog.vue";
import AdminActPanel from "@/components/organisms/AdminActPanel.vue";
import { useAdminAct } from "@/composables/useAdminAct";
import type { ActReport } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { AdminActKind, Role } from "@/link";

// LA fenêtre de confirmation d'un acte d'administration (HRT-30, BR-TRUST-036, 042, 043), réutilisée par TOUS les actes :
// créer un compte, changer un rôle, le mot de passe d'un autre ou le sien, supprimer un compte, fermer des sessions, mettre
// à jour l'agent, mode attaque, réglage de fréquence. Depuis HRT-245 elle n'est que l'enveloppe : la logique est dans
// `useAdminAct`, le corps dans `AdminActPanel` (que la page « Vérifiez avant de lancer » pose aussi, sans fenêtre). Le slot
// reçoit les champs propres à l'acte ; la fenêtre ajoute « Votre mot de passe » SELON CE QUE L'AGENT ANNONCE, lu à chaque
// ouverture (jamais deviné). Un refus se montre DANS la fenêtre ; fait ou résultat inconnu : elle se ferme (jamais rejoué,
// BR-RESIL-009). Échec déjà notifié : elle reste ouverte.

const props = withDefaults(
  defineProps<{
    open: boolean;
    serverId: string;
    /** Le genre d'acte : il décide ce que l'agent exige du mot de passe (règle de l'agent, `reauthPasswordNeed`). */
    kind: AdminActKind;
    /** Le rôle visé par l'acte (création, changement de rôle) : donner « Administrateur » n'est jamais couvert. */
    role?: Role | null;
    title: string;
    submitLabel: string;
    /** Les champs de l'acte sont prêts (la fenêtre ajoute ses propres conditions). */
    canSubmit?: boolean;
    destructive?: boolean;
    /** Le libellé du champ quand le mot de passe de confirmation a un nom propre (« Ancien mot de passe »). */
    passwordLabel?: string;
    /** Le libellé du bouton qui quitte sans agir (« Garder le déploiement » ; « Annuler » par défaut). */
    cancelLabel?: string;
    /** Fenêtre large (640 px), pour un récapitulatif (HRT-69). */
    wide?: boolean;
    /** Ce que l'acte attend encore avant d'être possible, écrit à côté du bouton quand `canSubmit` est faux (HRT-86). */
    blockedReason?: string;
    /**
     * Une question à poser AVANT l'acte, une fois le bouton pressé (le contrat de licence d'une fiche, HRT-102) : `false`
     * annule l'envoi sans rien perdre (le mot de passe saisi reste dans son champ, rien n'est parti).
     */
    beforeSubmit?: () => Promise<boolean>;
    /** Lance l'acte avec le mot de passe saisi (`null` : délai ouvert). */
    perform: (adminPassword: string | null) => Promise<ActReport>;
    /** Le texte d'un refus qui n'est pas celui de la confirmation ; `undefined` : la fenêtre n'en montre pas. */
    refusalText?: (refusal: { kind: string; retry_after_s?: number }) => string | undefined;
  }>(),
  {
    role: null,
    canSubmit: true,
    destructive: false,
    passwordLabel: undefined,
    cancelLabel: undefined,
    wide: false,
    blockedReason: undefined,
    beforeSubmit: undefined,
    refusalText: undefined,
  },
);
const emit = defineEmits<{ close: [] }>();

const act = useAdminAct({
  serverId: () => props.serverId,
  kind: () => props.kind,
  role: () => props.role,
  active: () => props.open,
  canSubmit: () => props.canSubmit,
  blockedReason: () => props.blockedReason,
  beforeSubmit: props.beforeSubmit
    ? () => props.beforeSubmit?.() ?? Promise.resolve(true)
    : undefined,
  perform: (adminPassword) => props.perform(adminPassword),
  refusalText: (refusal) => props.refusalText?.(refusal),
  onFinished: () => emit("close"),
});
const shownTitle = computed(() => act.shownTitle(props.title));
</script>

<template>
  <FormDialog
    :open="open"
    :title="shownTitle"
    :submit-label="submitLabel"
    :hide-submit="act.blocked.value"
    :can-submit="act.canSend.value"
    :submit-reason="act.needPasswordReason.value"
    :busy="act.sending.value"
    :error="act.error.value"
    :destructive="destructive"
    :cancel-label="act.blocked.value ? t('common.close') : cancelLabel"
    :wide="wide"
    @submit="act.submit"
    @cancel="emit('close')"
  >
    <AdminActPanel
      :act="act"
      :server-id="serverId"
      :password-label="passwordLabel"
      @leave="emit('close')"
    >
      <slot />
      <template #after><slot name="after" /></template>
    </AdminActPanel>
  </FormDialog>
</template>
