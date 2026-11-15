<script setup lang="ts">
import { computed } from "vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import { t } from "@/i18n";
import { BACKUP_DELETE_ACT_KIND, type BackupInfo } from "@/link";
import { formatSize } from "@/serviceOps/messages";
import { dateOf, isOnlyVerified, refusalText } from "@/services/backups";

// La fenêtre de suppression d'une sauvegarde (HRT-98, BR-BACKUP-027) : UNE fenêtre pour l'effet ET la confirmation du
// socle (`AdminActDialog`, acte `backup_delete`, JAMAIS couvert par l'élévation : le mot de passe est demandé à chaque
// fois). Elle dit qu'il n'y a pas de corbeille et, si c'est la seule sauvegarde vérifiée du service, le dit en avertissement.
// « Annuler » reste le bouton de repli ; « Supprimer la sauvegarde » est plein rouge. Une sauvegarde qu'une opération
// utilise est refusée par l'agent (« Cette sauvegarde est en cours d'utilisation. Attendez la fin de l'opération. »).
const props = defineProps<{
  open: boolean;
  serverId: string;
  backup: BackupInfo | null;
  backups: readonly BackupInfo[];
  perform: (adminPassword: string | null) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: [] }>();

const title = computed(() =>
  props.backup
    ? t("backups.deleteTitle", {
        date: dateOf(props.backup.createdAt),
        size: formatSize(props.backup.archiveBytes),
      })
    : t("backups.title"),
);
const only = computed(() => (props.backup ? isOnlyVerified(props.backups, props.backup) : false));
const rowOnly = computed(
  () => props.backup?.integrity === "corrupt" || props.backup?.integrity === "missing",
);
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="BACKUP_DELETE_ACT_KIND"
    :title="title"
    :submit-label="t('backups.deleteSubmit')"
    :perform="perform"
    :refusal-text="refusalText"
    :cancel-label="t('backups.includeCancel')"
    destructive
    @close="emit('close')"
  >
    <p class="bdel__text" data-backup-delete-text>{{ t("backups.deleteText") }}</p>
    <Callout v-if="only" tone="warn" data-backup-delete-only>{{ t("backups.deleteOnlyVerified") }}</Callout>
    <p v-if="rowOnly" class="bdel__text" data-backup-delete-row-only>{{ t("backups.deleteLineOnly") }}</p>
  </AdminActDialog>
</template>

<style scoped>
.bdel__text {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
