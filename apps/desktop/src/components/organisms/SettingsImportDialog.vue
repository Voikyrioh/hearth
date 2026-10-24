<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HSelect from "@/components/atoms/HSelect.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTextarea from "@/components/atoms/HTextarea.vue";
import Callout from "@/components/molecules/Callout.vue";
import type { ActReport } from "@/composables/useReauth";
import { t } from "@/i18n";
import type { AnalyzeOutcome, ImportAnalysis, SaveRefusal, SaveSettingsRequest } from "@/link";
import {
  type ImportRow,
  importChanges,
  importRows,
  toImportRequest,
  undoneLabels,
} from "@/settings/importing";
import { importStateText, saveRefusalText, valueText } from "@/settings/messages";
import { isSecret, type SettingModel } from "@/settings/model";
import AdminActDialog from "./AdminActDialog.vue";

// « Importer des réglages » (HRT-109, BR-FICHE-052) : l'utilisateur COLLE le contenu d'un ancien fichier de réglages
// (Hearth ne lit jamais de lui-même la configuration d'un ancien serveur). Le texte est envoyé à l'agent pour être lu
// (il connaît le format du jeu), puis oublié ; il n'est ni gardé ni écrit au journal. Un tableau dit, réglage par
// réglage : la valeur actuelle, la valeur collée (jamais celle d'un secret) et l'état (« Reconnu », « Inconnu : sera
// gardé », « Figé : ignoré », « Valeur que ce jeu ne connaît pas »). Une valeur refusée n'est PAS appliquée : on choisit
// une valeur de la liste du réglage ou on garde l'ancienne. Rien n'est appliqué avant « Appliquer ces réglages », qui est
// l'enregistrement ordinaire (origine `import`, même acte confirmé). Le texte vit dans cette fenêtre, en mémoire, et
// s'efface à sa fermeture, à l'issue ou au démontage.
const props = defineProps<{
  open: boolean;
  serverId: string;
  models: readonly SettingModel[];
  baseRev: number;
  fileStamp: string;
  /** Lit le texte (l'agent) ; `null` : la lecture a échoué (déjà notifié). */
  analyze: (text: string) => Promise<AnalyzeOutcome | null>;
  /** Enregistre l'import (l'acte confirmé) et rend ce que la fenêtre de confirmation doit savoir. */
  save: (request: SaveSettingsRequest, adminPassword: string | null) => Promise<ActReport>;
  labelOf: (id: string) => string | null;
}>();

const emit = defineEmits<{ close: [] }>();

const text = ref("");
const analysis = ref<ImportAnalysis | null>(null);
const unreadable = ref<{ line: number | null } | null>(null);
const reading = ref(false);
const picks = reactive<Record<string, string>>({});

function wipe() {
  text.value = "";
  analysis.value = null;
  unreadable.value = null;
  for (const id of Object.keys(picks)) delete picks[id];
}

watch(
  () => props.open,
  () => wipe(),
);
// Changer le texte invalide la lecture : on n'applique jamais un texte autre que celui qui a été lu.
watch(text, () => {
  analysis.value = null;
  unreadable.value = null;
});
onBeforeUnmount(wipe);

const rows = computed<ImportRow[]>(() =>
  analysis.value ? importRows(analysis.value, props.models) : [],
);
const changes = computed(() => importChanges(rows.value, picks));
const canApply = computed(() => analysis.value !== null && changes.value.length > 0);
const undone = computed(() => (analysis.value ? undoneLabels(analysis.value, props.models) : []));

async function read() {
  if (reading.value || text.value === "") return;
  reading.value = true;
  const sent = text.value;
  try {
    const outcome = await props.analyze(sent);
    // Le texte a pu changer pendant la lecture : cette réponse n'est plus la sienne.
    if (sent !== text.value) return;
    if (outcome === null) return;
    if (outcome.kind === "unreadable") {
      analysis.value = null;
      unreadable.value = { line: outcome.line };
    } else {
      unreadable.value = null;
      analysis.value = outcome.analysis;
    }
  } finally {
    reading.value = false;
  }
}

function currentOf(row: ImportRow): string {
  if (!row.model) return "";
  if (isSecret(row.model.setting)) {
    return row.model.state === "secret_set"
      ? t("gameSettings.importSecret")
      : valueText(row.model, null);
  }
  return valueText(row.model, row.model.value);
}

function pastedOf(row: ImportRow): string {
  if (row.line.secret || (row.model && isSecret(row.model.setting))) {
    return t("gameSettings.importSecret");
  }
  return row.model ? valueText(row.model, row.line.pasted) : safeText(row.line.pasted ?? "");
}

function labelOfRow(row: ImportRow): string {
  return row.model ? safeText(row.model.setting.question.label) : row.line.id;
}

function optionsOf(row: ImportRow) {
  return [
    { value: "", label: t("gameSettings.importKeepOld") },
    ...row.alternatives.map((choice) => ({ value: choice.value, label: safeText(choice.label) })),
  ];
}

async function perform(adminPassword: string | null): Promise<ActReport> {
  const built = toImportRequest({
    rows: rows.value,
    picks,
    text: text.value,
    baseRev: props.baseRev,
    fileStamp: props.fileStamp,
  });
  if (built === null) return { kind: "failed" };
  const report = await props.save(built.request, adminPassword);
  // Fait ou résultat inconnu : la fenêtre se ferme et le texte part de la mémoire.
  if (report.kind === "done" || report.kind === "unknown" || report.kind === "right_lost") wipe();
  return report;
}

function refusalText(refusal: { kind: string; retry_after_s?: number }): string | undefined {
  return saveRefusalText(refusal as SaveRefusal, props.labelOf);
}
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    kind="game_settings_save"
    :title="t('gameSettings.importTitle')"
    :submit-label="t('gameSettings.importApply')"
    :cancel-label="t('gameSettings.importCancel')"
    :can-submit="canApply"
    :perform="perform"
    :refusal-text="refusalText"
    wide
    data-import-dialog
    @close="emit('close')"
  >
    <div class="imp">
      <p class="imp__intro">{{ t("gameSettings.importIntro") }}</p>
      <HTextarea
        v-model="text"
        :label="t('gameSettings.importLabel')"
        :help="t('gameSettings.importPrivacy')"
        :disabled="reading"
        mono
        :rows="12"
        :max-rows="12"
        data-import-text
      />
      <div class="imp__read">
        <HButton
          variant="secondary"
          :disabled="text === '' || reading"
          :busy="reading"
          data-import-read
          @click="read"
        >
          {{ t("gameSettings.importRead") }}
        </HButton>
        <HSpinner v-if="reading" :label="t('gameSettings.importReading')" />
      </div>

      <Callout v-if="unreadable" tone="error" role="alert" data-import-unreadable>
        {{
          unreadable.line === null
            ? t("gameSettings.importUnreadable")
            : t("gameSettings.importUnreadableLine", { line: unreadable.line })
        }}
      </Callout>

      <template v-if="analysis">
        <Callout v-if="undone.length > 0" tone="warn" data-import-undone>
          {{ t("gameSettings.importUndone", { labels: undone.join(", ") }) }}
        </Callout>
        <table v-if="rows.length > 0" class="imp__table" data-import-table>
          <thead>
            <tr>
              <th scope="col">{{ t("gameSettings.importSetting") }}</th>
              <th scope="col">{{ t("gameSettings.importCurrent") }}</th>
              <th scope="col">{{ t("gameSettings.importPasted") }}</th>
              <th scope="col">{{ t("gameSettings.importState") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in rows" :key="row.line.id" :data-import-line="row.line.id" :data-import-state="row.line.state">
              <th scope="row">{{ labelOfRow(row) }}</th>
              <td>{{ currentOf(row) }}</td>
              <td>{{ pastedOf(row) }}</td>
              <td>
                <span class="imp__state">{{ importStateText(row.line.state) }}</span>
                <template v-if="row.line.state === 'rejected'">
                  <p class="imp__why">
                    {{ row.alternatives.length > 0 ? t("gameSettings.importRejectedLine") : t("gameSettings.importRejectedOther") }}
                  </p>
                  <HSelect
                    v-if="row.alternatives.length > 0"
                    :model-value="picks[row.line.id] ?? ''"
                    :options="optionsOf(row)"
                    :label="t('gameSettings.importPick', { label: labelOfRow(row) })"
                    @update:model-value="picks[row.line.id] = $event"
                  />
                </template>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="analysis.unknown > 0" class="imp__note" data-import-unknown>
          {{ t("gameSettings.importUnknown", { n: analysis.unknown }) }}
        </p>
        <p v-if="analysis.duplicates > 0" class="imp__note" data-import-duplicates>
          {{ t("gameSettings.importDuplicates", { n: analysis.duplicates }) }}
        </p>
        <p class="imp__note" data-import-count>
          {{ changes.length === 0 ? t("gameSettings.importNothing") : t("gameSettings.importCount", { n: changes.length }) }}
        </p>
      </template>
    </div>
  </AdminActDialog>
</template>

<style scoped>
.imp {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.imp__intro {
  margin: 0;
}

.imp__read {
  display: flex;
  align-items: center;
  gap: var(--space-3);
}

.imp__table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}

.imp__table th,
.imp__table td {
  padding: var(--space-2) var(--space-3) var(--space-2) 0;
  vertical-align: top;
  overflow-wrap: anywhere;
}

.imp__table thead th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
}

.imp__state {
  font-weight: var(--fw-medium);
}

.imp__why,
.imp__note {
  margin: var(--space-1) 0 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
