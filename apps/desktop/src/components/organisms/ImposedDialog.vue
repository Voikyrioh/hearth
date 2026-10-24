<script setup lang="ts">
import { safeText } from "@/audit/text";
import FormDialog from "@/components/molecules/FormDialog.vue";
import { t } from "@/i18n";
import type { ImposedSetting } from "@/link";

// Ce que la fiche d'un jeu impose (HRT-102, BR-JEU-039, BR-FICHE-025) : les réglages que la fiche pose sans que
// l'utilisateur les ait choisis, un par ligne, avec leur valeur et POURQUOI. Une valeur tirée, calculée ou secrète n'est
// jamais montrée : la cellule dit qu'elle est calculée par Hearth. Fenêtre de lecture seule.
defineProps<{ open: boolean; settings: readonly ImposedSetting[] }>();

const emit = defineEmits<{ close: [] }>();
</script>

<template>
  <FormDialog
    :open="open"
    :title="t('fiche.imposedTitle')"
    hide-submit
    wide
    :cancel-label="t('common.close')"
    data-imposed-dialog
    @cancel="emit('close')"
  >
    <div class="imposed__scroll">
      <table class="imposed__table" :aria-label="t('fiche.imposedTitle')">
        <thead>
          <tr>
            <th scope="col">{{ t("fiche.imposedColSetting") }}</th>
            <th scope="col">{{ t("fiche.imposedColValue") }}</th>
            <th scope="col">{{ t("fiche.imposedColWhy") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(setting, index) in settings" :key="index" data-imposed-row>
            <th scope="row">{{ safeText(setting.label) }}</th>
            <td class="imposed__value">
              {{ setting.value === null ? t("fiche.imposedComputed") : safeText(setting.value) }}
            </td>
            <td>{{ safeText(setting.why) }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </FormDialog>
</template>

<style scoped>
.imposed__scroll {
  overflow-x: auto;
}

.imposed__table {
  width: 100%;
  border-collapse: collapse;
  text-align: left;
}

.imposed__table th,
.imposed__table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
  vertical-align: top;
  overflow-wrap: anywhere;
}

.imposed__table thead th {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  font-weight: var(--fw-semibold);
}

.imposed__table tbody th {
  font-weight: var(--fw-medium);
}

.imposed__value {
  font-family: var(--font-mono);
}
</style>
