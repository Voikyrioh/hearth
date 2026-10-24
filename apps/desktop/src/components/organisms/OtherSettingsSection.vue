<script setup lang="ts">
import { ref, useId } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HTag from "@/components/atoms/HTag.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import { t } from "@/i18n";
import type { OtherSetting } from "@/link";

// « Autres réglages » (HRT-109, BR-FICHE-047) : les lignes du fichier que la fiche ne connaît pas. Elles ne sont jamais
// perdues ni effacées. Chacune montre son nom en police à chasse fixe, sa valeur en champ de texte (masquée, sans champ,
// quand le nom évoque un secret : l'agent n'en rend pas la valeur), la mention « Hearth ne connaît pas ce réglage. » et
// « Retirer », qui demande une confirmation. Retirer ou modifier ne part qu'à l'enregistrement.
const props = defineProps<{
  others: readonly OtherSetting[];
  /** Les valeurs modifiées, par nom ; une valeur absente : celle du fichier. */
  values: Readonly<Record<string, string>>;
  /** Les noms que la personne retire (à l'enregistrement). */
  removed: ReadonlySet<string>;
  disabled?: boolean;
}>();

const emit = defineEmits<{
  set: [key: string, value: string];
  remove: [key: string];
  keep: [key: string];
}>();

const confirming = ref<string | null>(null);
const titleId = useId();

function confirm() {
  if (confirming.value !== null) emit("remove", confirming.value);
  confirming.value = null;
}
</script>

<template>
  <section v-if="others.length > 0" class="others" :aria-labelledby="titleId" data-others>
    <h2 :id="titleId" class="others__title">{{ t("gameSettings.othersTitle") }}</h2>
    <ul class="others__list">
      <li v-for="other in others" :key="other.key" class="others__item" :data-other="other.key">
        <div class="others__head">
          <code class="others__key">{{ safeText(other.key) }}</code>
          <HTag v-if="removed.has(other.key)" tone="warn" data-other-removed>
            {{ t("gameSettings.othersRemoved") }}
          </HTag>
        </div>
        <p v-if="other.masked" class="others__note" data-other-masked>
          {{ t("gameSettings.othersMasked") }} {{ t("gameSettings.othersMaskedHelp") }}
        </p>
        <HInput
          v-else
          :model-value="values[other.key] ?? other.value ?? ''"
          :label="t('gameSettings.othersValue', { key: safeText(other.key) })"
          :disabled="disabled || removed.has(other.key)"
          mono
          @update:model-value="emit('set', other.key, $event)"
        />
        <p class="others__note">{{ t("gameSettings.othersNote") }}</p>
        <div class="others__actions">
          <HButton
            v-if="!removed.has(other.key)"
            variant="ghost"
            size="sm"
            :disabled="disabled"
            data-other-remove
            @click="confirming = other.key"
          >
            {{ t("gameSettings.othersRemove") }}
          </HButton>
          <HButton v-else variant="ghost" size="sm" data-other-keep @click="emit('keep', other.key)">
            {{ t("gameSettings.othersUndo") }}
          </HButton>
        </div>
      </li>
    </ul>
    <ConfirmDialog
      :open="confirming !== null"
      :title="t('gameSettings.othersRemoveTitle')"
      :message="t('gameSettings.othersRemoveText')"
      :confirm-label="t('gameSettings.othersRemove')"
      destructive
      @confirm="confirm"
      @cancel="confirming = null"
    />
  </section>
</template>

<style scoped>
.others {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  padding: var(--space-4) var(--space-5);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.others__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.others__list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
}

.others__item {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) 0;
  border-bottom: var(--border-width) solid var(--bd);
}

.others__item:last-child {
  border-bottom: 0;
}

.others__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}

.others__key {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.others__note {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.others__actions {
  display: flex;
}
</style>
