<script setup lang="ts">
import { computed } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import HTag from "@/components/atoms/HTag.vue";
import BackupModsCompare from "@/components/molecules/BackupModsCompare.vue";
import Callout from "@/components/molecules/Callout.vue";
import type { BackupModsEntry } from "@/composables/useBackupMods";
import { useCopyText } from "@/composables/useCopyText";
import { t } from "@/i18n";
import {
  copyOf,
  engineLine,
  hasList,
  itemFile,
  itemName,
  itemRank,
  listed,
  placeText,
  retiredCount,
  stateSentence,
} from "@/services/backupMods";
import { loaderName, originLabel } from "@/services/mods";

// Les mods d'une sauvegarde, dans sa ligne dépliée (HRT-294, BR-MOD-034 à 036, maquette Sauvegardes) : le moteur et la
// version de Minecraft du serveur au moment de la sauvegarde, la liste triée par nom (nom, version, moteur, état, origine),
// « Mods retirés : {r} » à part, « Copier la liste » (les mods activés, au format de BR-MOD-029) et « Comparer avec les
// mods actuels ». Une sauvegarde sans liste (avant la 0.2.2, illisible, dossier ignoré) le dit, sans rien reconstituer.
// Voir la liste ne demande que le droit de voir les sauvegardes : rien n'est écrit.
const props = defineProps<{
  state: BackupModsEntry | undefined;
  /** Date de création de la sauvegarde (RFC 3339), pour le titre de la liste copiée. */
  createdAt: string;
  /** L'adresse que les joueurs utilisent, si l'agent la connaît. */
  address: string | null;
}>();
const emit = defineEmits<{ compare: []; hideCompare: [] }>();

const copier = useCopyText(() => t("mods.players.copied"));
const view = computed(() => props.state?.view ?? null);
const sentence = computed(() => (view.value ? stateSentence(view.value.state) : null));
const showList = computed(() => view.value !== null && hasList(view.value.state));
const rows = computed(() => (view.value ? listed(view.value) : []));
const retired = computed(() => (view.value ? retiredCount(view.value) : 0));
const comparing = computed(() => props.state?.comparing === true);
const compare = computed(() => view.value?.compare ?? null);
</script>

<template>
  <div class="bm" data-backup-mods>
    <HSkeleton v-if="!state || state.kind === 'loading'" size="md" role="status" :aria-label="t('backups.mods.reading')" />
    <Callout v-else-if="state.kind === 'failed'" tone="warn" data-backup-mods-failed>
      {{ t("backups.mods.failed") }}
    </Callout>
    <Callout v-else-if="state.kind === 'refused'" tone="warn" data-backup-mods-refused>
      {{ t("backups.mods.refused") }}
    </Callout>
    <template v-else-if="view">
      <p v-if="sentence" class="bm__sentence" data-backup-mods-sentence>{{ sentence }}</p>
      <template v-if="showList">
        <p class="bm__engine" data-backup-mods-engine>{{ engineLine(view) }}</p>
        <table class="bm__table" data-backup-mods-table>
          <thead>
            <tr>
              <th scope="col">{{ t("mods.table.mod") }}</th>
              <th scope="col">{{ t("mods.table.version") }}</th>
              <th scope="col">{{ t("mods.table.loader") }}</th>
              <th scope="col">{{ t("mods.table.state") }}</th>
              <th scope="col">{{ t("mods.table.origin") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="item in rows" :key="`${item.rank}-${item.file ?? ''}`" data-backup-mods-row>
              <td>
                <strong>{{ itemName(item) }}</strong>
                <span v-if="itemFile(item)" class="bm__file">{{ itemFile(item) }}</span>
                <span v-else-if="itemRank(item)" class="bm__file">{{ itemRank(item) }}</span>
              </td>
              <td class="bm__mono">{{ item.identity?.version ?? "" }}</td>
              <td>{{ item.identity ? loaderName(item.identity.loader) : "" }}</td>
              <td>
                <HTag :tone="item.place === 'disabled' ? 'neutral' : 'ok'">{{ placeText(item) }}</HTag>
              </td>
              <td>{{ originLabel(item.origin) }}</td>
            </tr>
          </tbody>
        </table>
        <p v-if="retired > 0" class="bm__retired" data-backup-mods-retired>
          {{ t("backups.mods.retired", { n: retired }) }}
        </p>
        <div class="bm__bar">
          <HButton
            variant="secondary"
            size="sm"
            data-backup-mods-copy
            @click="copier.copy(copyOf(view, createdAt, address))"
          >
            <HIcon :name="copier.state.value === 'copied' ? 'check' : 'copy'" size="sm" />
            {{ t(copier.state.value === "copied" ? "mods.players.copiedButton" : "mods.players.copy") }}
          </HButton>
          <HButton
            v-if="!comparing"
            variant="secondary"
            size="sm"
            needs-link
            data-backup-mods-compare
            @click="emit('compare')"
          >
            {{ t("backups.mods.compare.button") }}
          </HButton>
          <HButton v-else variant="ghost" size="sm" data-backup-mods-hide @click="emit('hideCompare')">
            {{ t("backups.mods.compare.hide") }}
          </HButton>
        </div>
        <p v-if="copier.state.value === 'failed'" class="bm__retired" role="alert">
          {{ t("mods.players.copyFailed") }}
        </p>
        <BackupModsCompare v-if="compare" :compare="compare" />
        <HSkeleton v-else-if="comparing" size="md" role="status" :aria-label="t('backups.mods.compare.reading')" />
      </template>
    </template>
  </div>
</template>

<style scoped>
.bm {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.bm__sentence,
.bm__engine,
.bm__retired {
  margin: 0;
  color: var(--tx-secondary);
}

.bm__table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-small);
}

.bm__table th,
.bm__table td {
  padding: var(--space-1) var(--space-3);
  text-align: left;
  vertical-align: top;
}

.bm__table th {
  color: var(--tx-secondary);
  font-weight: var(--fw-medium);
}

.bm__table td {
  border-top: var(--border-width) solid var(--bd);
}

.bm__file {
  display: block;
  color: var(--tx-faint);
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}

.bm__mono {
  font-family: var(--font-mono);
}

.bm__bar {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}
</style>
