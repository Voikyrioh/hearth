<script setup lang="ts">
import { computed, ref, useId, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import type { PreviewState } from "@/composables/useServiceBackups";
import { t } from "@/i18n";
import { BACKUP_CREATE_ACT_KIND, type BackupMode, type BackupRequest } from "@/link";
import { formatSize } from "@/serviceOps/messages";
import {
  bigFileText,
  defaultMode,
  estimateText,
  graceText,
  itemPath,
  noteLength,
  noteTooLong,
  offersModes,
  previewNoSpace,
  readRefusalText,
  refusalText,
  requestOf,
} from "@/services/backups";

// La fenêtre de création d'une sauvegarde (HRT-98, BR-BACKUP-006 à 012, 033) : UNE fenêtre pour l'effet ET la confirmation
// du socle (`AdminActDialog`, acte `backup_create`, couvert par l'élévation de 5 minutes). Elle dit d'abord ce que la
// sauvegarde contient et ne contient pas (jamais la configuration du service), qu'elle peut contenir des mots de passe, puis
// propose le mode quand le service tourne : « Sans arrêter le service (copie à chaud) » PRÉSÉLECTIONNÉ, ou « Arrêter,
// sauvegarder, relancer » avec l'avertissement sur ce qui n'est pas enregistré et le délai d'arrêt du service. Un service
// arrêté se copie directement, sans choix. Les fichiers de plus de 1 Gio sont dits un par un avec une case à cocher qui les
// ignore POUR CETTE SAUVEGARDE seulement. La note est facultative (80 caractères, jamais écrite dans le journal). L'espace
// est estimé à l'ouverture (jusqu'à 60 secondes) ; s'il manque, le refus donne les chiffres et AUCUNE case ne permet de
// passer outre : le bouton reste inactif. L'agent est l'arbitre à l'envoi et refait tous les contrôles.
const props = defineProps<{
  open: boolean;
  serverId: string;
  preview: PreviewState;
  perform: (request: BackupRequest, adminPassword: string | null) => Promise<ActReport>;
  /** Hearth enregistre le monde par la console avant d'arrêter ce jeu : la phrase du mode « Arrêter » le dit exactement. */
  consoleSave?: boolean;
}>();
const emit = defineEmits<{ close: []; retry: [] }>();

const mode = ref<BackupMode>("hot");
const note = ref("");
const skipped = ref<ReadonlySet<string>>(new Set());
const modeName = useId();

const ready = computed(() => (props.preview.kind === "ready" ? props.preview.preview : null));

// Rouverte, la fenêtre repart de ses valeurs de départ : rien n'est gardé d'une ouverture à l'autre.
watch(
  () => props.open,
  () => {
    mode.value = "hot";
    note.value = "";
    skipped.value = new Set();
  },
);
watch(ready, (preview) => {
  if (preview) mode.value = defaultMode(preview);
});

const tooLong = computed(() => noteTooLong(note.value));
const noSpace = computed(() => (ready.value ? previewNoSpace(ready.value) : null));
const nothing = computed(() => ready.value?.nothingToSave === true);
const canSubmit = computed(
  () => ready.value !== null && noSpace.value === null && !nothing.value && !tooLong.value,
);
const blockedReason = computed(() => {
  if (tooLong.value) return t("backups.noteTooLong");
  if (noSpace.value) return t("backups.noSpaceReason");
  if (nothing.value) return t("backups.refused.nothingToSave");
  return undefined;
});

function key(item: { root: string; path: string }): string {
  return `${item.root}\u0000${item.path}`;
}

function toggleBig(item: { root: string; path: string }, on: boolean): void {
  const next = new Set(skipped.value);
  if (on) next.add(key(item));
  else next.delete(key(item));
  skipped.value = next;
}

function send(adminPassword: string | null): Promise<ActReport> {
  const preview = ready.value;
  if (!preview) return Promise.resolve({ kind: "failed" });
  const ignored = preview.bigFiles.filter((file) => skipped.value.has(key(file)));
  return props.perform(
    requestOf({ preview, mode: mode.value, note: note.value, ignored }),
    adminPassword,
  );
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="BACKUP_CREATE_ACT_KIND"
    :title="t('backups.createTitle', { name: safeText(ready?.name ?? '') })"
    :submit-label="t('backups.createSubmit')"
    :can-submit="canSubmit"
    :blocked-reason="blockedReason"
    :perform="send"
    :refusal-text="refusalText"
    wide
    @close="emit('close')"
  >
    <div v-if="preview.kind === 'loading'" class="bd__wait" role="status" data-backup-reading>
      <HSkeleton size="md" />
      <p class="bd__help">{{ t("backups.reading") }}</p>
      <p class="bd__help">{{ t("backups.readingHelp") }}</p>
    </div>

    <div v-else-if="preview.kind === 'failed'" class="bd__state" data-backup-failed>
      <Callout tone="error" role="alert">{{ t("backups.previewFailed") }}</Callout>
      <HButton variant="secondary" needs-link data-backup-retry @click="emit('retry')">
        {{ t("backups.retry") }}
      </HButton>
    </div>

    <Callout v-else-if="preview.kind === 'refused'" tone="error" role="alert" data-backup-refused>
      {{ readRefusalText(preview.refusal) }}
    </Callout>

    <template v-else-if="ready">
      <Callout tone="info" data-backup-info>{{ t("backups.createInfo") }}</Callout>
      <Callout tone="warn" data-backup-content-warning>{{ t("backups.contentWarning") }}</Callout>

      <fieldset v-if="offersModes(ready)" class="bd__modes" data-backup-modes>
        <legend class="bd__legend">{{ t("backups.modeLegend") }}</legend>
        <label :class="['bd__card', { 'bd__card--on': mode === 'hot' }]" data-backup-mode-hot>
          <input v-model="mode" class="bd__radio" type="radio" :name="modeName" value="hot" />
          <span class="bd__text">
            <span class="bd__name">{{ t("backups.modeHot") }}</span>
            <span class="bd__help">{{ t("backups.modeHotText") }}</span>
          </span>
        </label>
        <label :class="['bd__card', { 'bd__card--on': mode === 'stop_copy_start' }]" data-backup-mode-stop>
          <input v-model="mode" class="bd__radio" type="radio" :name="modeName" value="stop_copy_start" />
          <span class="bd__text">
            <span class="bd__name">{{ t("backups.modeStop") }}</span>
            <span class="bd__help">{{ consoleSave ? t("backups.modeStopTextConsole") : t("backups.modeStopText") }}</span>
            <span class="bd__help">{{ graceText(ready) }}</span>
          </span>
        </label>
      </fieldset>
      <p v-else class="bd__soft" data-backup-stopped>{{ t("backups.stoppedLine") }}</p>

      <section v-if="ready.bigFiles.length > 0" class="bd__big" data-backup-big>
        <div v-for="file in ready.bigFiles" :key="key(file)" class="bd__bigitem">
          <Callout tone="info" inline>{{ bigFileText(file) }}</Callout>
          <HCheckbox
            :model-value="skipped.has(key(file))"
            :label="t('backups.bigFileBox')"
            data-backup-big-box
            @update:model-value="toggleBig(file, $event)"
          />
        </div>
      </section>

      <div class="bd__note">
        <HInput
          v-model="note"
          :label="t('backups.noteLabel')"
          :placeholder="t('backups.notePlaceholder')"
          :error="tooLong ? t('backups.noteTooLong') : undefined"
          :help="t('backups.noteHelp')"
          data-backup-note
        />
        <p class="bd__count" :class="{ 'bd__count--over': tooLong }" aria-hidden="true" data-backup-note-count>
          {{ t("backups.noteCount", { n: noteLength(note) }) }}
        </p>
      </div>

      <Callout v-if="noSpace" tone="error" role="alert" data-backup-no-space>{{ noSpace }}</Callout>
      <template v-else>
        <p class="bd__soft" data-backup-estimate>{{ estimateText(ready) }}</p>
        <p v-if="ready.freeBytes !== null" class="bd__soft" data-backup-space>
          {{ t("backups.spaceAfter", { free: formatSize(ready.freeBytes), reserve: formatSize(ready.reserveBytes) }) }}
        </p>
      </template>

      <section v-if="ready.ignored.length > 0" class="bd__ignored" data-backup-ignored>
        <p class="bd__soft">{{ t("backups.ignoredThisTime") }}</p>
        <ul class="bd__list">
          <li v-for="item in ready.ignored" :key="key(item)" class="bd__path">
            {{ t(item.why === "looks_like_backups" ? "backups.ignoredLine" : "backups.ignoredLineChosen", { path: itemPath(item) }) }}
          </li>
        </ul>
        <p v-if="ready.ignoredMore > 0" class="bd__soft">{{ t("backups.ignoredMore", { n: ready.ignoredMore }) }}</p>
      </section>
    </template>
  </AdminActDialog>
</template>

<style scoped>
.bd__wait,
.bd__state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}

.bd__wait > :first-child {
  align-self: stretch;
}

.bd__help,
.bd__soft {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.bd__modes {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: 0;
  padding: 0;
  border: 0;
  min-width: 0;
}

.bd__legend {
  padding: 0;
  margin-bottom: var(--space-2);
  font-weight: var(--fw-semibold);
}

.bd__card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-3);
  padding: var(--space-3);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  cursor: pointer;
}

.bd__card--on {
  border-color: var(--ac);
  background: var(--card-2);
}

.bd__card:focus-within {
  outline: var(--focus-ring) solid var(--ac);
  outline-offset: var(--focus-offset);
}

.bd__radio {
  margin-top: var(--space-1);
  accent-color: var(--ac);
}

.bd__text {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.bd__name {
  font-weight: var(--fw-semibold);
}

.bd__big {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.bd__bigitem {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.bd__note {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.bd__count {
  margin: 0;
  align-self: flex-end;
  color: var(--tx-secondary);
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.bd__count--over {
  color: var(--crit);
}

.bd__ignored {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

.bd__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.bd__path {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}
</style>
