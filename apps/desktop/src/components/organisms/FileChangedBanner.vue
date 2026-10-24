<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HSegmented from "@/components/atoms/HSegmented.vue";
import Callout from "@/components/molecules/Callout.vue";
import { t } from "@/i18n";
import type { Drift } from "@/link";
import type { DriftChoices } from "@/settings/edits";
import { valueText } from "@/settings/messages";
import type { SettingModel } from "@/settings/model";

// « Le fichier a changé depuis la dernière fois que Hearth l'a écrit » (HRT-109, BR-FICHE-048) : le jeu, ou une main, a
// réécrit le fichier. Un tableau « Réglage · Valeur de Hearth · Valeur du fichier » et, pour CHAQUE réglage, deux choix :
// « Garder la valeur du fichier » (par défaut) ou « Remettre la mienne ». Rien n'est écrit tant que l'enregistrement
// n'est pas lancé ; sans réponse, le service redémarre avec le fichier tel qu'il est. Si la valeur du fichier n'est pas
// acceptée par le réglage, « Garder la valeur du fichier » serait refusé : seul « Remettre la mienne » reste. Jamais un
// secret ici (l'agent n'en rend aucun). Les libellés et les choix viennent de la fiche.
const props = defineProps<{
  drifts: readonly Drift[];
  models: readonly SettingModel[];
  choices: DriftChoices;
  disabled?: boolean;
}>();

const emit = defineEmits<{ choose: [id: string, choice: "keep_file" | "keep_mine"] }>();

const rows = computed(() =>
  props.drifts.map((drift) => {
    const model = props.models.find((item) => item.id === drift.id) ?? null;
    return {
      drift,
      label: model ? safeText(model.setting.question.label) : drift.id,
      mine: model ? valueText(model, drift.hearth) : (drift.hearth ?? t("gameSettings.driftNone")),
      file: model ? valueText(model, drift.file) : (drift.file ?? t("gameSettings.driftNone")),
    };
  }),
);

function choiceOf(drift: Drift): "keep_file" | "keep_mine" {
  if (!drift.fileValid) return "keep_mine";
  return props.choices[drift.id] ?? "keep_file";
}

function optionsOf(drift: Drift) {
  return [
    { value: "keep_file" as const, label: t("gameSettings.driftKeepFile") },
    { value: "keep_mine" as const, label: t("gameSettings.driftKeepMine") },
  ].filter((option) => drift.fileValid || option.value === "keep_mine");
}
</script>

<template>
  <Callout tone="warn" data-file-changed>
    <p class="fc__lead">
      {{
        drifts.length > 0
          ? t("gameSettings.fileChanged", { n: drifts.length })
          : t("gameSettings.fileChangedNone")
      }}
    </p>
    <table v-if="rows.length > 0" class="fc__table" data-file-changed-table>
      <thead>
        <tr>
          <th scope="col">{{ t("gameSettings.driftSetting") }}</th>
          <th scope="col">{{ t("gameSettings.driftMine") }}</th>
          <th scope="col">{{ t("gameSettings.driftFile") }}</th>
          <th scope="col"><span class="sr-only">{{ t("gameSettings.driftKeepFile") }}</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.drift.id" :data-drift="row.drift.id">
          <th scope="row">{{ row.label }}</th>
          <td data-drift-mine>{{ row.mine }}</td>
          <td data-drift-file>{{ row.file }}</td>
          <td class="fc__choice">
            <HSegmented
              :model-value="choiceOf(row.drift)"
              :options="optionsOf(row.drift)"
              :label="t('gameSettings.driftChoose', { label: row.label })"
              :disabled="disabled"
              @update:model-value="emit('choose', row.drift.id, $event)"
            />
            <p v-if="!row.drift.fileValid" class="fc__invalid" data-drift-invalid>
              {{ t("gameSettings.driftInvalid") }}
            </p>
          </td>
        </tr>
      </tbody>
    </table>
  </Callout>
</template>

<style scoped>
.fc__lead {
  margin: 0 0 var(--space-3);
}

.fc__table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}

.fc__table th,
.fc__table td {
  padding: var(--space-2) var(--space-3) var(--space-2) 0;
  vertical-align: top;
  overflow-wrap: anywhere;
}

.fc__table thead th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-medium);
}

.fc__table tbody th {
  font-weight: var(--fw-medium);
}

.fc__choice {
  min-width: 0;
}

.fc__invalid {
  margin: var(--space-1) 0 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
