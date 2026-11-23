<script setup lang="ts">
import { computed, ref, useId } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTag from "@/components/atoms/HTag.vue";
import BackupModsPanel from "@/components/molecules/BackupModsPanel.vue";
import type { BackupModsEntry } from "@/composables/useBackupMods";
import type { VerifyNote } from "@/composables/useServiceBackups";
import { t } from "@/i18n";
import type { BackupInfo } from "@/link";
import { formatSize } from "@/serviceOps/messages";
import { columnText } from "@/services/backupMods";
import {
  accountOf,
  dataLine,
  dateOf,
  flagIsWarning,
  flagLabels,
  imageOf,
  integrityText,
  modeLabel,
  noteOf,
  rowLabel,
} from "@/services/backups";

// La liste des sauvegardes d'un service (HRT-98, BR-BACKUP-012, 013) : une ligne par sauvegarde, la plus récente d'abord
// (ordre fixe, jamais trié par l'interface). Date et repères en texte (« À chaud », « Peut être incohérente »…), note,
// taille (archive, puis données et fichiers), mode, état d'intégrité (le mot ET l'icône : « Corrompue » n'est jamais la
// couleur seule), actions. Une ligne se déplie (bouton, `aria-expanded`) : compte, image et version du service au moment
// de la sauvegarde, fichiers et éléments non archivés. « Vérifier » est une consultation dite dans la ligne ; « Supprimer »
// ouvre la fenêtre de confirmation. « Restaurer » et « Télécharger » ne sont pas encore proposés (HRT-95, HRT-99).
// Une sauvegarde corrompue ou disparue garde « Vérifier » et « Supprimer ».
const props = defineProps<{
  backups: readonly BackupInfo[];
  verifying: ReadonlySet<string>;
  notes: ReadonlyMap<string, VerifyNote>;
  /** Le compte peut vérifier et supprimer (droit « gérer les services » accordé). */
  canWrite: boolean;
  /** L'`id` de la raison du droit écrite ailleurs, une seule fois : les boutons désactivés la citent. */
  rightsReasonId: string;
  /** La liste des mods de chaque sauvegarde déjà lue (HRT-294) ; sans elle, aucune ligne dépliée ne montre de mods. */
  modsStates?: ReadonlyMap<string, BackupModsEntry>;
  /** L'adresse que les joueurs utilisent, pour « Copier la liste » d'une sauvegarde. */
  modsAddress?: string | null;
}>();
const emit = defineEmits<{
  verify: [backup: BackupInfo];
  remove: [backup: BackupInfo];
  modsOpen: [backup: BackupInfo];
  modsClose: [backup: BackupInfo];
  modsCompare: [backup: BackupInfo];
  modsHideCompare: [backup: BackupInfo];
}>();

/** La colonne « Mods » n'existe que pour un serveur à mods : l'agent joint un résumé à chacune de ses sauvegardes (BR-MOD-035). */
const withMods = computed(() => props.backups.some((backup) => backup.mods !== null));

const idBase = useId();
const open = ref<ReadonlySet<string>>(new Set());

function toggle(backup: BackupInfo): void {
  const next = new Set(open.value);
  if (next.has(backup.id)) {
    next.delete(backup.id);
    if (backup.mods) emit("modsClose", backup);
  } else {
    next.add(backup.id);
    if (backup.mods) emit("modsOpen", backup);
  }
  open.value = next;
}

function noteText(note: VerifyNote | undefined, files = 0): string | null {
  if (!note) return null;
  switch (note.kind) {
    case "ok":
      return files === 1 || note.files === 1
        ? t("backups.verifiedNowOne")
        : t("backups.verifiedNow", { count: note.files });
    case "corrupt":
      return t("backups.verifyCorrupt");
    case "missing":
      return t("backups.verifyMissing");
    case "in-use":
      return t("backups.verifyInUse");
    case "unknown":
      return t("backups.verifyUnknown");
    default:
      return t("backups.verifyFailed");
  }
}

function integrityIcon(backup: BackupInfo): "check" | "alert" | "info" {
  if (backup.integrity === "verified") return "check";
  if (backup.integrity === "corrupt" || backup.integrity === "missing") return "alert";
  return "info";
}
</script>

<template>
  <div class="bk" data-backup-table>
    <table class="bk__table" :aria-label="t('backups.tableLabel')">
      <thead>
        <tr>
          <th scope="col" class="bk__expand"><span class="sr-only">{{ t("backups.expand", { date: "" }) }}</span></th>
          <th scope="col">{{ t("backups.colDate") }}</th>
          <th scope="col">{{ t("backups.colNote") }}</th>
          <th scope="col" class="bk__num">{{ t("backups.colSize") }}</th>
          <th scope="col">{{ t("backups.colMode") }}</th>
          <th scope="col">{{ t("backups.colState") }}</th>
          <th v-if="withMods" scope="col">{{ t("backups.colMods") }}</th>
          <th scope="col" class="bk__actions-head">{{ t("backups.colActions") }}</th>
        </tr>
      </thead>
      <tbody>
        <template v-for="backup in backups" :key="backup.id">
          <tr
            class="bk__row"
            :data-backup="backup.id"
            :data-integrity="backup.integrity"
            :aria-label="rowLabel(backup)"
          >
            <td class="bk__expand">
              <button
                type="button"
                class="bk__toggle"
                :aria-expanded="open.has(backup.id)"
                :aria-controls="`${idBase}-${backup.id}`"
                :aria-label="t(open.has(backup.id) ? 'backups.expandHide' : 'backups.expand', { date: dateOf(backup.createdAt) })"
                data-backup-toggle
                @click="toggle(backup)"
              >
                <HIcon :name="open.has(backup.id) ? 'chevron-down' : 'chevron-right'" size="sm" />
              </button>
            </td>
            <td class="bk__date">
              <span class="bk__when" data-backup-date>{{ dateOf(backup.createdAt) }}</span>
              <span v-if="flagLabels(backup).length > 0" class="bk__flags" data-backup-flags>
                <HTag v-for="label in flagLabels(backup)" :key="label" :tone="flagIsWarning(label) ? 'warn' : 'neutral'">{{ label }}</HTag>
              </span>
            </td>
            <td class="bk__note" data-backup-note>
              <span v-if="noteOf(backup)">{{ noteOf(backup) }}</span>
              <span v-else class="bk__soft">{{ t("backups.noNote") }}</span>
            </td>
            <td class="bk__num">
              <span data-backup-size>{{ formatSize(backup.archiveBytes) }}</span>
              <span class="bk__soft bk__line2">{{ dataLine(backup) }}</span>
            </td>
            <td data-backup-mode>{{ modeLabel(backup.mode) }}</td>
            <td class="bk__state">
              <span v-if="verifying.has(backup.id)" class="bk__verifying" role="status" data-backup-verifying>
                <HSpinner :label="t('backups.verifying')" />
                <HMeter :ratio="null" indeterminate :label="t('backups.verifying')" />
              </span>
              <span v-else :class="['bk__integrity', `bk__integrity--${backup.integrity}`]" data-backup-integrity>
                <HIcon :name="integrityIcon(backup)" size="sm" />
                {{ integrityText(backup) }}
              </span>
              <span v-if="notes.get(backup.id)" class="bk__verdict" role="status" data-backup-verdict>
                {{ noteText(notes.get(backup.id), backup.files) }}
              </span>
            </td>
            <td v-if="withMods" class="bk__mods" data-backup-mods-column>
              <span v-if="backup.mods" class="bk__mods-text">{{ columnText(backup.mods) }}</span>
            </td>
            <td class="bk__actions">
              <HButton
                variant="secondary"
                size="sm"
                needs-link
                :disabled="!canWrite || verifying.has(backup.id)"
                :described-by="canWrite ? undefined : rightsReasonId"
                :aria-label="t('backups.verifyLabel', { date: dateOf(backup.createdAt) })"
                data-backup-verify
                @click="emit('verify', backup)"
              >
                {{ t("backups.verify") }}
              </HButton>
              <HButton
                variant="danger"
                size="sm"
                needs-link
                :disabled="!canWrite"
                :described-by="canWrite ? undefined : rightsReasonId"
                :aria-label="t('backups.removeLabel', { date: dateOf(backup.createdAt) })"
                data-backup-remove
                @click="emit('remove', backup)"
              >
                {{ t("backups.remove") }}
              </HButton>
            </td>
          </tr>
          <tr v-if="open.has(backup.id)" :id="`${idBase}-${backup.id}`" class="bk__more" data-backup-more>
            <td />
            <td :colspan="withMods ? 7 : 6">
              <dl class="bk__defs">
                <div>
                  <dt>{{ t("backups.detailAccount") }}</dt>
                  <dd>{{ accountOf(backup) }}</dd>
                </div>
                <div>
                  <dt>{{ t("backups.detailImage") }}</dt>
                  <dd class="bk__mono">{{ imageOf(backup) }}</dd>
                </div>
                <div>
                  <dt>{{ t("backups.detailData") }}</dt>
                  <dd>{{ dataLine(backup) }}</dd>
                </div>
                <div v-if="backup.changedFiles > 0">
                  <dt>{{ t("backups.detailChanged") }}</dt>
                  <dd>{{ backup.changedFiles }}</dd>
                </div>
                <div v-if="backup.skipped > 0">
                  <dt>{{ t("backups.detailSkipped") }}</dt>
                  <dd>{{ backup.skipped }}</dd>
                </div>
              </dl>
              <BackupModsPanel
                v-if="backup.mods"
                :state="modsStates?.get(backup.id)"
                :created-at="backup.createdAt"
                :address="modsAddress ?? null"
                @compare="emit('modsCompare', backup)"
                @hide-compare="emit('modsHideCompare', backup)"
              />
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.bk {
  min-width: 0;
  overflow-x: auto;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.bk__table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-body);
}

.bk__table th {
  padding: var(--space-3) var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
  text-align: left;
  white-space: nowrap;
}

.bk__table td {
  padding: var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
  vertical-align: top;
  min-width: 0;
}

.bk__row:last-child td,
.bk__more:last-child td {
  border-bottom: 0;
}

.bk__mods-text {
  white-space: nowrap;
}

.bk__expand {
  width: var(--target-size);
  padding-right: 0;
}

.bk__toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: var(--target-size);
  min-height: var(--target-size);
  border: 0;
  border-radius: var(--radius-control);
  background: transparent;
  color: var(--tx-secondary);
  cursor: pointer;
}

.bk__toggle:hover {
  background: var(--card-2);
  color: var(--tx);
}

.bk__date {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: var(--backup-date-min);
}

.bk__when {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  white-space: nowrap;
}

.bk__flags {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
}

.bk__note {
  min-width: var(--backup-note-min);
  overflow-wrap: anywhere;
}

.bk__num {
  text-align: right;
  white-space: nowrap;
}

.bk__line2 {
  display: block;
  font-size: var(--fs-small);
}

.bk__soft {
  color: var(--tx-secondary);
}

.bk__state {
  min-width: var(--backup-state-min);
}

.bk__integrity {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
}

.bk__integrity--verified {
  color: var(--ok);
}

.bk__integrity--corrupt,
.bk__integrity--missing {
  color: var(--crit);
}

.bk__integrity--unchecked,
.bk__integrity--unknown {
  color: var(--tx-secondary);
}

.bk__verifying {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-width: var(--backup-progress-min);
}

.bk__verdict {
  display: block;
  margin-top: var(--space-1);
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.bk__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-2);
}

.bk__actions-head {
  text-align: right;
}

.bk__more td {
  background: var(--card-2);
}

.bk__defs {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(var(--backup-detail-col), 1fr));
  gap: var(--space-2) var(--space-4);
  margin: 0;
}

.bk__defs dt {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.bk__defs dd {
  margin: 0;
  overflow-wrap: anywhere;
}

.bk__mono {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}
</style>
