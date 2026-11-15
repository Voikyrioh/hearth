<script setup lang="ts">
import { computed, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import type { PreviewState } from "@/composables/useServiceBackups";
import { t } from "@/i18n";
import { BACKUP_SETTINGS_ACT_KIND, type BackupSettings, type IgnoredItem } from "@/link";
import {
  addErrorText,
  checkAdd,
  itemPath,
  readRefusalText,
  refusalText,
  sameItem,
} from "@/services/backups";

// La fenêtre des éléments ignorés d'un service (HRT-98, BR-BACKUP-003) : ce que Hearth ne lit ni n'archive. Les éléments
// ignorés PAR DÉFAUT (ce qui ressemble à des sauvegardes faites hors de Hearth) viennent de l'estimation de l'agent ; un
// compte peut en ajouter, en retirer, ou faire entrer un ignoré par défaut dans les sauvegardes : ce dernier geste demande
// une confirmation (« Cet élément sera inclus dans les sauvegardes et pourra être remplacé par une restauration. »). Les
// modifications sont un BROUILLON : rien ne part avant « Enregistrer », un acte couvert par l'élévation de 5 minutes.
// Un chemin se tape DANS LE SERVICE, sous un de ses dossiers de données ; un autre chemin est refusé d'avance et par
// l'agent. Aucun chemin n'est écrit au journal.
const props = defineProps<{
  open: boolean;
  serverId: string;
  /** L'estimation lue à l'ouverture : elle porte les éléments ignorés par défaut. */
  preview: PreviewState;
  /** Ce que l'agent a enregistré. */
  settings: BackupSettings;
  /** Les dossiers de données du service (cibles de ses montages de dossier). */
  roots: readonly string[];
  perform: (settings: BackupSettings, adminPassword: string | null) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: [] }>();

const chosen = ref<IgnoredItem[]>([]);
const included = ref<IgnoredItem[]>([]);
const typed = ref("");
const touched = ref(false);
const toInclude = ref<IgnoredItem | null>(null);

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    chosen.value = props.settings.ignored.map((item) => ({ ...item }));
    included.value = props.settings.included.map((item) => ({ ...item }));
    typed.value = "";
    touched.value = false;
    toInclude.value = null;
  },
  { immediate: true },
);

const defaults = computed<IgnoredItem[]>(() =>
  props.preview.kind === "ready"
    ? props.preview.preview.ignored
        .filter((item) => item.why === "looks_like_backups")
        .filter((item) => !included.value.some((other) => sameItem(other, item)))
        .map(({ root, path }) => ({ root, path }))
    : [],
);

const check = computed(() => checkAdd(typed.value, props.roots, chosen.value));
const addError = computed(() => (touched.value ? addErrorText(check.value.kind) : undefined));

const changed = computed(() => {
  const same = (a: readonly IgnoredItem[], b: readonly IgnoredItem[]) =>
    a.length === b.length && a.every((item) => b.some((other) => sameItem(other, item)));
  return (
    !same(chosen.value, props.settings.ignored) || !same(included.value, props.settings.included)
  );
});
const canSubmit = computed(() => props.preview.kind !== "loading" && changed.value);

function add(): void {
  touched.value = true;
  const verdict = check.value;
  if (verdict.kind !== "ok" || !verdict.item) return;
  chosen.value = [...chosen.value, verdict.item];
  typed.value = "";
  touched.value = false;
}

function removeChosen(item: IgnoredItem): void {
  chosen.value = chosen.value.filter((other) => !sameItem(other, item));
}

function confirmInclude(): void {
  const item = toInclude.value;
  toInclude.value = null;
  if (item) included.value = [...included.value, item];
}

function putBack(item: IgnoredItem): void {
  included.value = included.value.filter((other) => !sameItem(other, item));
}

function send(adminPassword: string | null): Promise<ActReport> {
  return props.perform(
    {
      ignored: chosen.value.map(({ root, path }) => ({ root, path })),
      included: included.value.map(({ root, path }) => ({ root, path })),
    },
    adminPassword,
  );
}

const rowKey = (item: IgnoredItem, kind: string) => `${kind}:${item.root}\u0000${item.path}`;
</script>

<template>
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="BACKUP_SETTINGS_ACT_KIND"
    :title="t('backups.ignoredDialogTitle')"
    :submit-label="t('backups.ignoredDialogSubmit')"
    :can-submit="canSubmit"
    :perform="send"
    :refusal-text="refusalText"
    wide
    @close="emit('close')"
  >
    <p class="ig__help">{{ t("backups.ignoredDialogHelp") }}</p>

    <div v-if="preview.kind === 'loading'" class="ig__wait" role="status" data-ignored-reading>
      <HSkeleton size="md" />
      <p class="ig__help">{{ t("backups.ignoredDialogReading") }}</p>
    </div>
    <Callout v-else-if="preview.kind === 'refused'" tone="warn" data-ignored-refused>
      {{ readRefusalText(preview.refusal) }}
    </Callout>
    <Callout v-else-if="preview.kind === 'failed'" tone="warn" data-ignored-failed>
      {{ t("backups.previewFailed") }}
    </Callout>

    <ul class="ig__list" :aria-label="t('backups.ignoredListLabel')" data-ignored-list>
      <li v-for="item in defaults" :key="rowKey(item, 'default')" class="ig__row" data-ignored-default>
        <span class="ig__path">{{ t("backups.ignoredLine", { path: itemPath(item) }) }}</span>
        <HButton
          variant="secondary"
          size="sm"
          :aria-label="t('backups.removeFromListLabel', { path: itemPath(item) })"
          data-ignored-remove-default
          @click="toInclude = item"
        >
          {{ t("backups.removeFromList") }}
        </HButton>
      </li>
      <li v-for="item in chosen" :key="rowKey(item, 'chosen')" class="ig__row" data-ignored-chosen>
        <span class="ig__path">{{ t("backups.ignoredChosen", { path: itemPath(item) }) }}</span>
        <HButton
          variant="secondary"
          size="sm"
          :aria-label="t('backups.removeFromListLabel', { path: itemPath(item) })"
          data-ignored-remove-chosen
          @click="removeChosen(item)"
        >
          {{ t("backups.removeFromList") }}
        </HButton>
      </li>
      <li v-for="item in included" :key="rowKey(item, 'included')" class="ig__row" data-ignored-included>
        <span class="ig__path">{{ t("backups.includedLine", { path: itemPath(item) }) }}</span>
        <HButton
          variant="secondary"
          size="sm"
          :aria-label="t('backups.undoRemove') + ' ' + itemPath(item)"
          data-ignored-put-back
          @click="putBack(item)"
        >
          {{ t("backups.undoRemove") }}
        </HButton>
      </li>
    </ul>
    <p
      v-if="defaults.length === 0 && chosen.length === 0 && included.length === 0 && preview.kind !== 'loading'"
      class="ig__help"
      data-ignored-empty
    >
      {{ t("backups.ignoredEmpty") }}
    </p>

    <!-- Entrée ajoute l'élément, elle n'envoie jamais l'acte entier -->
    <div class="ig__add" @keydown.enter.prevent="add">
      <HInput
        v-model="typed"
        :label="t('backups.addLabel')"
        :help="t('backups.addHelp')"
        :error="addError"
        mono
        data-ignored-input
        @blur="touched = typed !== ''"
      />
      <HButton variant="secondary" :disabled="check.kind === 'empty'" data-ignored-add @click="add">
        {{ t("backups.addButton") }}
      </HButton>
    </div>

    <ConfirmDialog
      :open="toInclude !== null"
      :title="t('backups.includeTitle', { path: toInclude ? itemPath(toInclude) : '' })"
      :message="t('backups.includeMessage')"
      :confirm-label="t('backups.includeConfirm')"
      :cancel-label="t('backups.includeCancel')"
      @confirm="confirmInclude"
      @cancel="toInclude = null"
    />
  </AdminActDialog>
</template>

<style scoped>
.ig__help {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.ig__wait {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.ig__list {
  display: flex;
  flex-direction: column;
  margin: 0;
  padding: 0;
  list-style: none;
}

.ig__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2) var(--space-3);
  padding: var(--space-2) 0;
  border-bottom: var(--border-width) solid var(--bd);
}

.ig__row:last-child {
  border-bottom: 0;
}

.ig__path {
  flex: 1 1 var(--backup-path-basis);
  min-width: 0;
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.ig__add {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: var(--space-3);
}

.ig__add > :first-child {
  flex: 1 1 var(--backup-path-basis);
}

.ig__add > button {
  margin-top: var(--space-6);
}
</style>
