<script setup lang="ts">
import { computed } from "vue";
import { safeText } from "@/audit/text";
import HTextarea from "@/components/atoms/HTextarea.vue";
import { t } from "@/i18n";
import type { FicheSetting } from "@/link";
import { listIssue, listItems, listText, listValue } from "@/settings/edits";
import { listIssueText } from "@/settings/messages";
import Callout from "./Callout.vue";

// Une liste d'identifiants (HRT-109, BR-FICHE-037) : un champ multiligne en police à chasse fixe, un
// identifiant par ligne, avec le compteur « {n} sur {max} ». Le séparateur, le jeu de caractères et le nombre maximal
// sont ceux de la fiche ; AUCUNE règle de jeu ici. Sous le champ, toujours : le jeu lit lui-même ces identifiants, Hearth
// ne les vérifie pas. La valeur voulue est celle que le jeu lit (les identifiants joints par le séparateur de la fiche).
const props = defineProps<{
  setting: FicheSetting;
  /** La valeur voulue, telle que le jeu la lit. */
  modelValue: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const text = computed(() => listText(props.setting, props.modelValue));
const count = computed(() => listItems(text.value).length);
const issue = computed(() => {
  const found = listIssue(props.setting, text.value);
  return found ? listIssueText(found, props.setting.list?.itemCharset ?? "unknown") : undefined;
});

function update(next: string) {
  emit("update:modelValue", listValue(props.setting, next));
}
</script>

<template>
  <div class="idlist" data-id-list>
    <HTextarea
      :model-value="text"
      :label="safeText(setting.question.label)"
      :help="t('gameSettings.listHelp')"
      :error="issue"
      :disabled="disabled"
      mono
      :rows="4"
      :max-rows="12"
      @update:model-value="update"
    />
    <p v-if="setting.list" class="idlist__count" data-id-list-count>
      {{ t("gameSettings.listCount", { n: count, max: setting.list.maxItems }) }}
    </p>
    <Callout tone="info" inline data-id-list-note>{{ t("gameSettings.listNote") }}</Callout>
  </div>
</template>

<style scoped>
.idlist {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.idlist__count {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}
</style>
