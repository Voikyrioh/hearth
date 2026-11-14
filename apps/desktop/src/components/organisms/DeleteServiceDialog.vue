<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HCheckbox from "@/components/atoms/HCheckbox.vue";
import HInput from "@/components/atoms/HInput.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import Callout from "@/components/molecules/Callout.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { ActReport } from "@/composables/useReauth";
import type { PreviewState } from "@/composables/useServiceDelete";
import { t } from "@/i18n";
import { DELETE_ACT_KIND, type DeleteRequest, type ServiceInfo } from "@/link";
import {
  effectText,
  erasableFolders,
  folderList,
  folderNotes,
  folderRows,
  mustRetype,
  nameMatches,
  offersBackupsBox,
  offersDataBox,
  previewRefusalText,
  refusalText,
  requestOf,
  runningText,
  staleNotice,
  unmeasuredNotes,
} from "@/services/delete";

// La fenêtre de suppression d'un service (HRT-86, BR-DEL-004 à 013, 022, 026) : UNE fenêtre pour l'effet ET la
// confirmation du socle (`AdminActDialog`, acte `service_delete`, JAMAIS couvert par l'élévation de 5 minutes : le
// mot de passe est demandé même juste après un autre acte, même sans case cochée, parce que la configuration est perdue).
// Elle dit d'abord ce que la suppression fait (le service est retiré, sa configuration effacée, ses journaux perdus,
// l'image reste), montre les dossiers de données gérés par Hearth avec leur taille et leur sort, puis propose deux
// cases DÉCOCHÉES : effacer les données, effacer les sauvegardes. Cocher une case demande de retaper le nom du service,
// casse comprise ; « Supprimer le service » reste plein rouge dans tous les cas. Les dossiers du serveur ne sont jamais
// effacés. La mesure des dossiers dure jusqu'à 10 secondes : la confirmation reste possible pendant ce temps, la fenêtre
// attend la liste avant de proposer les cases (rien ne s'efface sans que la liste ait été lue). Si le service change sous
// la fenêtre (BR-LIFE-022) elle le dit et la confirmation devient inactive ; « Annuler » reste actif et ne laisse aucune
// trace. Le moteur pas prêt ou le service disparu désactive aussi la confirmation (BR-DEL-018, 022).
const props = defineProps<{
  open: boolean;
  serverId: string;
  /** Le service tel qu'il était à l'ouverture (nom). */
  service: ServiceInfo;
  /** Le service tel qu'il est maintenant dans la liste ; `null` : il a disparu. */
  current: ServiceInfo | null;
  /** Une action tient le service depuis l'ouverture de la fenêtre. */
  locked: boolean;
  engineReady: boolean;
  preview: PreviewState;
  perform: (request: DeleteRequest, adminPassword: string | null) => Promise<ActReport>;
}>();
const emit = defineEmits<{ close: []; retry: [] }>();

const eraseData = ref(false);
const eraseBackups = ref(false);
const typed = ref("");

// Rouverte, la fenêtre repart des cases décochées : rien n'est gardé d'une ouverture à l'autre (BR-DEL-005).
watch(
  () => props.open,
  () => {
    eraseData.value = false;
    eraseBackups.value = false;
    typed.value = "";
  },
);

const ready = computed(() => (props.preview.kind === "ready" ? props.preview.preview : null));
const name = computed(() => safeText(props.service.name));
const boxes = computed(() => ({ eraseData: eraseData.value, eraseBackups: eraseBackups.value }));
const retype = computed(() => mustRetype(boxes.value));
const nameWrong = computed(() => retype.value && !nameMatches(typed.value, props.service.name));
const stale = computed(() => staleNotice(props.current, props.locked, props.engineReady));

const rows = computed(() => (ready.value ? folderRows(ready.value, eraseData.value) : []));
const notes = computed(() => (ready.value ? folderNotes(ready.value) : []));
const sizeNotes = computed(() => (ready.value ? unmeasuredNotes(ready.value) : []));
const erasable = computed(() => (ready.value ? erasableFolders(ready.value) : []));
const list = computed(() => folderList(erasable.value));
const dataBox = computed(() => (ready.value ? offersDataBox(ready.value) : false));
const backupsBox = computed(() => (ready.value ? offersBackupsBox(ready.value) : false));
const volumes = computed(() => ready.value?.volumes ?? 0);

/** Pourquoi la confirmation est inactive (et ce qui la rend active) : écrit à côté du bouton. */
const canSubmit = computed(() => ready.value !== null && stale.value === null && !nameWrong.value);

function send(adminPassword: string | null): Promise<ActReport> {
  return props.perform(requestOf(boxes.value, typed.value), adminPassword);
}

function refusal(refused: { kind: string }): string {
  return refusalText(refused);
}
</script>

<template>
  <!-- FIX:01M5A5VMDW33WJG3S1BWKE3N15 : aucun attribut ici que la fenêtre (racine téléportée) ne pourrait recevoir. -->
  <AdminActDialog
    :open="open"
    :server-id="serverId"
    :kind="DELETE_ACT_KIND"
    :title="t('serviceDelete.title', { name })"
    :submit-label="t('serviceDelete.submit')"
    :can-submit="canSubmit"
    :blocked-reason="nameWrong ? t('serviceDelete.nameReason') : undefined"
    :perform="send"
    :refusal-text="refusal"
    destructive
    wide
    @close="emit('close')"
  >
    <Callout v-if="stale" tone="info" role="status" data-delete-stale>{{ stale }}</Callout>

    <div v-if="preview.kind === 'loading'" class="del__wait" role="status" data-delete-reading>
      <HSpinner :label="t('serviceDelete.reading')" />
      <p class="del__help">{{ t("serviceDelete.readingHelp") }}</p>
    </div>

    <div v-else-if="preview.kind === 'failed'" class="del__state" data-delete-failed>
      <Callout tone="error" role="alert">{{ t("serviceDelete.previewFailed") }}</Callout>
      <HButton variant="secondary" needs-link data-delete-retry @click="emit('retry')">
        {{ t("serviceDelete.retry") }}
      </HButton>
    </div>

    <Callout v-else-if="preview.kind === 'refused'" tone="error" role="alert" data-delete-refused>
      {{ previewRefusalText(preview.refusal) }}
    </Callout>

    <template v-else-if="ready">
      <Callout v-if="ready.running" tone="warn" data-delete-running>{{ runningText(ready) }}</Callout>
      <Callout tone="warn" data-delete-effect>{{ effectText(ready) }}</Callout>
      <Callout v-if="ready.containerMissing" tone="info" data-delete-missing>
        {{ t("serviceDelete.containerMissing") }}
      </Callout>

      <section v-if="rows.length > 0" class="del__folders" data-delete-folders>
        <table class="del__table">
          <caption class="sr-only">{{ t("serviceDelete.foldersLabel") }}</caption>
          <thead>
            <tr>
              <th scope="col">{{ t("serviceDelete.colFolder") }}</th>
              <th scope="col">{{ t("serviceDelete.colSize") }}</th>
              <th scope="col">{{ t("serviceDelete.colItems") }}</th>
              <th scope="col">{{ t("serviceDelete.colFate") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in rows" :key="row.key" :data-erased="row.erased ? '' : undefined">
              <td class="del__path" :title="row.path">{{ row.path }}</td>
              <td class="del__num" :data-label="t('serviceDelete.colSize')">{{ row.size }}</td>
              <td class="del__num" :data-label="t('serviceDelete.colItems')">{{ row.items }}</td>
              <td :class="['del__fate', { 'del__fate--erased': row.erased }]" :data-label="t('serviceDelete.colFate')">
                {{ row.fate }}
              </td>
            </tr>
          </tbody>
        </table>
        <p v-for="note in sizeNotes" :key="note.key" class="del__help" data-delete-size-note>{{ note.text }}</p>
        <p v-for="note in notes" :key="note.key" class="del__help" data-delete-note>{{ note.text }}</p>
        <p v-if="ready.serverFolders.length > 0" class="del__help" data-delete-server>
          {{ t("serviceDelete.serverFolders") }}
        </p>
        <p v-if="volumes > 0" class="del__help" data-delete-volumes>
          {{ volumes === 1 ? t("serviceDelete.volumesOne") : t("serviceDelete.volumesMany", { n: volumes }) }}
        </p>
      </section>
      <p v-else class="del__help" data-delete-no-folders>{{ t("serviceDelete.noFolders") }}</p>

      <Callout v-if="dataBox && !eraseData" tone="info" data-delete-kept>
        {{ t("serviceDelete.keptNotice", { list, name }) }}
      </Callout>
      <!-- Ce qui ne se défait pas sort du lot : le rouge lui est réservé (revue UX de Nora du 14/11, constat 7). -->
      <Callout v-if="dataBox && eraseData" tone="error" data-delete-erased>
        {{ t("serviceDelete.erasedNotice", { list }) }}
      </Callout>
      <Callout v-if="backupsBox && eraseBackups" tone="warn" data-delete-backups-notice>
        {{ t("serviceDelete.backupsNotice", { n: ready.backups }) }}
      </Callout>

      <fieldset v-if="dataBox || backupsBox" class="del__boxes" data-delete-boxes>
        <legend class="sr-only">{{ t("serviceDelete.boxesLegend") }}</legend>
        <div v-if="dataBox" class="del__box">
          <HCheckbox v-model="eraseData" :label="t('serviceDelete.dataBox')" data-delete-data />
          <p class="del__help">{{ t("serviceDelete.dataHelp") }}</p>
          <p v-if="eraseData" class="del__help" data-delete-no-backup>{{ t("serviceDelete.noBackup") }}</p>
        </div>
        <div v-if="backupsBox" class="del__box">
          <HCheckbox v-model="eraseBackups" :label="t('serviceDelete.backupsBox')" data-delete-backups />
        </div>
      </fieldset>

      <HInput
        v-if="retype"
        v-model="typed"
        :label="t('serviceDelete.nameLabel')"
        :error="typed !== '' && nameWrong ? t('serviceDelete.nameError') : undefined"
        :help="t('serviceDelete.nameHelp')"
        mono
        data-delete-name
      />
    </template>
  </AdminActDialog>
</template>

<style scoped>
.del__wait {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}

.del__state {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-3);
}

.del__help {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.del__folders {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

.del__table {
  width: 100%;
  border-collapse: collapse;
  font-size: var(--fs-small);
}

.del__table th,
.del__table td {
  padding: var(--space-2) var(--space-3);
  border-bottom: var(--border-width) solid var(--bd);
  text-align: left;
  vertical-align: top;
}

/* Le chemin prend toute la place qui reste ; les trois autres colonnes gardent leur largeur (revue UX de Nora, constat 3). */
.del__table th:first-child {
  width: 100%;
}

.del__table th:not(:first-child),
.del__table td:not(.del__path) {
  white-space: nowrap;
}

.del__table th {
  color: var(--tx-secondary);
  font-weight: var(--fw-semibold);
}

.del__path {
  font-family: var(--font-mono);
  overflow-wrap: anywhere;
}

.del__num {
  font-family: var(--font-mono);
  white-space: nowrap;
}

.del__fate--erased {
  color: var(--crit);
  font-weight: var(--fw-semibold);
}

.del__boxes {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
}

.del__box {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}

/* Sous 960 px de colonne, la taille, les éléments et le sort passent sous le chemin (deux lignes par dossier). */
@media (max-width: 960px) {
  .del__table thead {
    display: none;
  }

  .del__table tr,
  .del__table td {
    display: block;
  }

  .del__table td[data-label]::before {
    content: attr(data-label) " : ";
    color: var(--tx-secondary);
  }
}
</style>
