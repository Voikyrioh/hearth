<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import { capitalize, originOf, outcomeLabel, sourceOf, whenOf } from "@/audit/format";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import { t } from "@/i18n";
import type { AuditEntry } from "@/link";

// Une entrée du journal en entier : c'est ici qu'on lit les valeurs tronquées dans le tableau.
// Texte non fiable : interpolation seulement, caractères de contrôle neutralisés, retour à la ligne
// des valeurs longues. Élément `<dialog>` natif (piège à focus, Échap) ; le focus revient à la ligne.
const props = defineProps<{ entry: AuditEntry | null }>();
const emit = defineEmits<{ close: [] }>();

const titleId = useId();
const dialog = ref<HTMLDialogElement | null>(null);
const closeButton = ref<{ $el: HTMLElement } | null>(null);
let opener: HTMLElement | null = null;

const fields = computed(() => {
  const entry = props.entry;
  if (!entry) return [];
  // HRT-43 (C32) : pas de ligne vide (cible ou raison absentes) ; source lisible, raison avec majuscule.
  const rows: Array<readonly [string, string]> = [
    [t("audit.colWhen"), whenOf(entry)],
    [t("audit.detailSource"), sourceOf(entry)],
    [t("audit.colAccount"), safeText(entry.account)],
    [t("audit.colOrigin"), originOf(entry)],
    [t("audit.colAction"), safeText(entry.actionLabel)],
    [t("audit.colTarget"), safeText(entry.target)],
    [t("audit.colOutcome"), outcomeLabel(entry.outcome)],
    [t("audit.colReason"), capitalize(safeText(entry.reason))],
    [t("audit.detailId"), String(entry.id)],
  ];
  return rows.filter(([, value]) => value.trim() !== "");
});

watch(
  () => props.entry,
  (entry, previous) => {
    if (entry && !previous) {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    }
  },
  { immediate: true, flush: "pre" },
);

watch(
  () => props.entry !== null,
  async (open) => {
    if (open) {
      await nextTick();
      dialog.value?.showModal();
      closeButton.value?.$el.querySelector("button")?.focus();
    } else {
      opener?.focus();
      opener = null;
    }
  },
  { immediate: true, flush: "post" },
);

onBeforeUnmount(() => {
  opener?.focus();
  opener = null;
});

function onNativeClose() {
  if (props.entry) emit("close");
}
</script>

<template>
  <Teleport to="body">
    <dialog
      v-if="entry"
      ref="dialog"
      class="detail"
      :aria-labelledby="titleId"
      @cancel.prevent="emit('close')"
      @close="onNativeClose"
    >
      <h2 :id="titleId" class="detail__title">{{ t("audit.details") }}</h2>
      <dl class="detail__list">
        <template v-for="[label, value] in fields" :key="label">
          <dt class="detail__label">{{ label }}</dt>
          <dd class="detail__value">{{ value }}</dd>
        </template>
      </dl>
      <div class="detail__actions">
        <HButton ref="closeButton" variant="secondary" @click="emit('close')">
          {{ t("common.close") }}
        </HButton>
      </div>
    </dialog>
  </Teleport>
</template>

<style scoped>
.detail {
  width: var(--audit-detail-width);
  max-width: calc(100vw - var(--page-pad) * 2);
  max-height: calc(100vh - var(--page-pad) * 2);
  padding: var(--space-5);
  overflow-y: auto;
  border: 0;
  border-radius: var(--radius-card);
  background: var(--card);
  color: var(--tx);
  box-shadow: var(--card-edge);
}

.detail::backdrop {
  background: var(--scrim);
}

.detail__title {
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.detail__list {
  display: grid;
  grid-template-columns: max-content 1fr;
  gap: var(--space-2) var(--space-4);
  margin-top: var(--space-4);
}

.detail__label {
  color: var(--tx2);
}

.detail__value {
  min-width: 0;
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.detail__actions {
  display: flex;
  justify-content: flex-end;
  margin-top: var(--space-5);
}
</style>
