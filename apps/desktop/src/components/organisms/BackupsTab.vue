<script setup lang="ts">
import { computed, useId } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HMeter from "@/components/atoms/HMeter.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import DashCard from "@/components/molecules/DashCard.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import BackupDialog from "@/components/organisms/BackupDialog.vue";
import BackupTable from "@/components/organisms/BackupTable.vue";
import DeleteBackupDialog from "@/components/organisms/DeleteBackupDialog.vue";
import IgnoredItemsDialog from "@/components/organisms/IgnoredItemsDialog.vue";
import { useBackupMods } from "@/composables/useBackupMods";
import { useServiceBackups } from "@/composables/useServiceBackups";
import { t } from "@/i18n";
import type { BackupInfo, ServiceInfo } from "@/link";
import { formatSize } from "@/serviceOps/messages";
import { MODS_LEGEND } from "@/services/backupMods";
import {
  blockedReason,
  dataRoots,
  dateOf,
  itemPath,
  readRefusalText,
  refusalText,
  spaceLine,
  spaceWarnings,
  writeGate,
} from "@/services/backups";
import { type PilotLock } from "@/services/pilot";
import { canAct, type RightsGate } from "@/services/rights";

// L'onglet « Sauvegardes » d'un service créé par Hearth (HRT-98, BR-BACKUP-001 à 015, 026 à 028) : la liste lisible par tout
// compte, « Sauvegarder » (principal) et « Éléments ignorés » (secondaire) pour qui a le droit de gérer les services, les
// deux visibles et désactivés avec leur raison écrite dessous sinon (le droit par `ServiceRightReason`, une seule fois ;
// une action qui tient le service ; un état que Hearth ne sauvegarde pas ; le moteur pas prêt). La carte « Espace » dit
// l'espace pris, l'espace libre et les avertissements permanents (même disque, mots de passe possibles, plus de 10
// sauvegardes, disque presque plein). L'opération qui court (progression, étapes) est la carte du haut du détail ; l'onglet
// la suit : son issue (créée et vérifiée, incohérente, annulée, échec) est dite ici jusqu'à sa fermeture. Les sauvegardes
// faites hors de Hearth ne sont jamais listées ni touchées (BR-BACKUP-004) : l'onglet le dit.
const props = defineProps<{
  serverId: string;
  service: ServiceInfo;
  gate: RightsGate;
  lock: PilotLock;
  /** L'`id` de la raison du droit que le détail écrit déjà, une seule fois (BR-GRANT-014). */
  rightsReasonId: string;
  connected: boolean;
  engineReady: boolean;
}>();

const flow = useServiceBackups(
  () => props.serverId,
  () => props.service,
  () => props.connected,
);

const reasonId = useId();
const mods = useBackupMods(
  () => props.serverId,
  () => props.service.key,
);
const view = computed(() => (flow.list.value.kind === "ready" ? flow.list.value.view : null));
const backups = computed<readonly BackupInfo[]>(() => view.value?.backups ?? []);
const shownName = computed(() => safeText(props.service.name));
const roots = computed(() => dataRoots(props.service));

/** Les boutons d'écriture : « Sauvegarder » juge aussi l'état et le moteur ; les autres actes ne demandent que le droit. */
const saveGate = computed(() =>
  writeGate({
    gate: props.gate,
    lock: props.lock,
    engineReady: props.engineReady,
    state: props.service.state,
  }),
);
const rightsGate = computed(() =>
  writeGate({ gate: props.gate, lock: props.lock, engineReady: true, state: null }),
);
const canWrite = computed(() => canAct(props.gate));
const reason = computed(() => blockedReason(saveGate.value));
const rightsDenied = computed(() => saveGate.value.kind === "blocked" && saveGate.value.rights);
const describedBy = computed(() => {
  if (rightsDenied.value) return props.rightsReasonId;
  return reason.value ? reasonId : undefined;
});
/** « Éléments ignorés » ne demande que le droit et le lien (jamais le moteur) ; l'action qui tient le service ne le bloque pas. */
const ignoredBlocked = computed(
  () => rightsGate.value.kind === "blocked" && rightsGate.value.rights,
);

const warnings = computed(() => (view.value ? spaceWarnings(view.value) : []));
const freeRatio = computed(() => {
  const current = view.value;
  if (!current || current.freeBytes === null) return null;
  const used = current.totalBytes;
  const all = used + current.freeBytes;
  return all > 0 ? used / all : null;
});
const chosenIgnored = computed(() => view.value?.settings.ignored ?? []);
const includedItems = computed(() => view.value?.settings.included ?? []);

const emptyWithBackups = computed(() => view.value !== null && view.value.backups.length === 0);

/** La phrase qui dit l'issue de la dernière sauvegarde lancée d'ici (BR-BACKUP-007, 009, 013). */
interface ResultLine {
  key: string;
  tone: "ok" | "info" | "warn" | "error";
  text: string;
}
const resultLines = computed<ResultLine[]>(() => {
  const result = flow.result.value;
  if (!result) return [];
  const { operation, backup, name } = result;
  const say = (key: Parameters<typeof t>[0], tone: ResultLine["tone"], id: string) => ({
    key: id,
    tone,
    text: t(key, { name: safeText(name) }),
  });
  switch (operation.status) {
    case "succeeded": {
      const lines: ResultLine[] = [];
      lines.push(
        backup
          ? {
              key: "done",
              tone: "ok",
              text: t("backups.resultDone", {
                date: dateOf(backup.createdAt),
                size: formatSize(backup.archiveBytes),
              }),
            }
          : say("backups.resultDoneBare", "ok", "done"),
      );
      if (backup?.mode === "hot") {
        lines.push(
          backup.changedFiles === 0
            ? { key: "hot", tone: "info", text: t("backups.resultHotClean") }
            : {
                key: "hot",
                tone: "warn",
                text:
                  backup.changedFiles === 1
                    ? t("backups.resultHotChangedOne")
                    : t("backups.resultHotChanged", { n: backup.changedFiles }),
              },
        );
      }
      if (backup && backup.skipped > 0) {
        lines.push({
          key: "skipped",
          tone: "info",
          text:
            backup.skipped === 1
              ? t("backups.resultSkippedOne")
              : t("backups.resultSkipped", { n: backup.skipped }),
        });
      }
      if (operation.cause === "not_restarted") {
        lines.push(say("backups.resultNotRestarted", "warn", "not-restarted"));
      }
      return lines;
    }
    case "failed":
      switch (operation.cause) {
        case "backup_corrupt":
          return [say("backups.resultCorrupt", "error", "failed")];
        case "no_space":
          return [say("backups.resultNoSpace", "error", "failed")];
        case "disk_almost_full":
          return [say("backups.resultAlmostFull", "error", "failed")];
        case "engine_unavailable":
        case "engine_refused":
          return [say("backups.resultEngine", "error", "failed")];
        default:
          return [say("backups.resultFailed", "error", "failed")];
      }
    case "cancelled":
      return [say("backups.resultCancelled", "info", "cancelled")];
    case "interrupted":
      return [
        operation.cause === "timeout"
          ? say("backups.resultTimeout", "warn", "interrupted")
          : say("backups.resultInterrupted", "warn", "interrupted"),
      ];
    default:
      return [say("backups.resultFailed", "warn", "other")];
  }
});
</script>

<template>
  <div class="bt" data-backups-tab>
    <header class="bt__head">
      <h3 class="bt__title">{{ t("backups.title") }}</h3>
      <div v-if="view && !view.nothingToSave" class="bt__buttons">
        <HButton
          variant="secondary"
          needs-link
          :disabled="ignoredBlocked"
          :described-by="ignoredBlocked ? rightsReasonId : undefined"
          data-backups-ignored
          @click="flow.openIgnored()"
        >
          {{ t("backups.ignoredButton") }}
        </HButton>
        <HButton
          v-if="backups.length > 0"
          variant="primary"
          needs-link
          :disabled="saveGate.kind === 'blocked'"
          :described-by="describedBy"
          data-backups-save
          @click="flow.openCreate()"
        >
          {{ t("backups.save") }}
        </HButton>
      </div>
    </header>
    <p v-if="reason" :id="reasonId" class="bt__reason" data-backups-reason>{{ reason }}</p>

    <!-- L'issue de la sauvegarde lancée d'ici, dite une fois, fermable -->
    <section v-if="resultLines.length > 0" class="bt__result" data-backups-result>
      <Callout
        v-for="line in resultLines"
        :key="line.key"
        :tone="line.tone"
        :role="line.tone === 'error' ? 'alert' : 'status'"
        :data-backups-result-line="line.key"
      >
        {{ line.text }}
      </Callout>
      <HButton variant="ghost" size="sm" data-backups-dismiss @click="flow.dismissResult()">
        {{ t("backups.dismiss") }}
      </HButton>
    </section>
    <Callout v-if="flow.unknownResult.value" tone="warn" data-backups-unknown>
      {{ t("backups.unknownResult") }} {{ t("backups.unknownAfter") }}
    </Callout>

    <!-- Une sauvegarde court : l'annuler tant qu'elle court (BR-BACKUP-011) ; la progression est la carte du haut du détail -->
    <div v-if="flow.ongoingBackup.value && canWrite" class="bt__running" data-backups-running>
      <HButton variant="danger" size="sm" needs-link data-backups-cancel @click="flow.cancelOpen.value = true">
        {{ t("backups.cancelRunning") }}
      </HButton>
    </div>

    <div v-if="flow.list.value.kind === 'loading'" class="bt__loading" role="status" :aria-label="t('backups.loading')" data-backups-loading>
      <HSkeleton v-for="n in 5" :key="n" size="lg" />
    </div>

    <div v-else-if="flow.list.value.kind === 'failed'" class="bt__state" data-backups-failed>
      <Callout tone="error" role="alert">{{ t("backups.loadFailed") }}</Callout>
      <HButton variant="secondary" needs-link data-backups-retry @click="flow.load()">
        {{ t("backups.retry") }}
      </HButton>
    </div>

    <Callout v-else-if="flow.list.value.kind === 'refused'" tone="warn" data-backups-refused>
      {{ readRefusalText(flow.list.value.refusal) }}
    </Callout>

    <template v-else-if="view">
      <Callout v-if="view.nothingToSave" tone="info" data-backups-nothing>
        {{ t("backups.nothingToSave") }}
      </Callout>

      <template v-else>
        <DashCard :title="t('backups.spaceTitle')" data-backups-space>
          <p class="bt__line" data-backups-space-line>
            {{ spaceLine(view) }}
            <template v-if="view.freeBytes !== null">
              · {{ t("backups.spaceFree", { free: formatSize(view.freeBytes) }) }}
            </template>
          </p>
          <div v-if="freeRatio !== null" class="bt__meter">
            <HMeter :ratio="freeRatio" :label="t('backups.spaceMeter')" />
          </div>
          <Callout
            v-for="warning in warnings"
            :key="warning.key"
            tone="warn"
            inline
            :data-backups-warning="warning.key"
          >
            {{ warning.text }}
          </Callout>
        </DashCard>

        <DashCard :title="t('backups.ignoredTitle')" data-backups-ignored-card>
          <ul v-if="chosenIgnored.length > 0 || includedItems.length > 0" class="bt__paths">
            <li v-for="item in chosenIgnored" :key="`i-${item.root}-${item.path}`" class="bt__path">
              {{ t("backups.ignoredChosen", { path: itemPath(item) }) }}
            </li>
            <li v-for="item in includedItems" :key="`n-${item.root}-${item.path}`" class="bt__path">
              {{ t("backups.ignoredIncluded", { path: itemPath(item) }) }}
            </li>
          </ul>
          <p v-else class="bt__soft">{{ t("backups.ignoredNone") }}</p>
          <p class="bt__soft">{{ t("backups.ignoredDefaultHint") }}</p>
        </DashCard>

        <EmptyState
          v-if="emptyWithBackups"
          :title="t('backups.empty')"
          :text="t('backups.emptyHelp')"
          heading="h2"
          data-backups-empty
        >
          <template #action>
            <HButton
              variant="primary"
              needs-link
              :disabled="saveGate.kind === 'blocked'"
              :described-by="describedBy"
              data-backups-save-empty
              @click="flow.openCreate()"
            >
              {{ t("backups.save") }}
            </HButton>
          </template>
        </EmptyState>

        <BackupTable
          v-else
          :backups="backups"
          :verifying="flow.verifying.value"
          :notes="flow.verifyNotes.value"
          :can-write="canWrite"
          :rights-reason-id="rightsReasonId"
          :mods-states="mods.states.value"
          :mods-address="mods.address.value"
          @verify="flow.verify($event)"
          @remove="flow.requestDelete($event)"
          @mods-open="mods.open($event.id)"
          @mods-close="mods.close($event.id)"
          @mods-compare="mods.compare($event.id)"
          @mods-hide-compare="mods.hideCompare($event.id)"
        />
        <ul v-if="backups.some((backup) => backup.mods)" class="bt__legend" data-backups-mods-legend>
          <li v-for="entry in MODS_LEGEND" :key="entry.term">
            <strong>{{ t(entry.term) }}</strong> : {{ t(entry.text) }}
          </li>
        </ul>
      </template>
    </template>

    <p class="bt__soft" data-backups-foreign>{{ t("backups.foreignNote") }}</p>

    <BackupDialog
      :open="flow.createOpen.value"
      :server-id="serverId"
      :preview="flow.preview.value"
      :perform="flow.performCreate"
      @close="flow.closeCreate()"
      @retry="flow.loadPreview()"
    />
    <IgnoredItemsDialog
      :open="flow.ignoredOpen.value"
      :server-id="serverId"
      :preview="flow.ignoredPreview.value"
      :settings="view?.settings ?? { ignored: [], included: [] }"
      :roots="roots"
      :perform="flow.performSaveSettings"
      @close="flow.closeIgnored()"
    />
    <DeleteBackupDialog
      :open="flow.deleting.value !== null"
      :server-id="serverId"
      :backup="flow.deleting.value"
      :backups="backups"
      :perform="flow.performDelete"
      @close="flow.closeDelete()"
    />
    <AdminActDialog
      :open="flow.cancelOpen.value"
      :server-id="serverId"
      kind="operation_cancel"
      :title="t('backups.cancelTitle', { name: shownName })"
      :submit-label="t('backups.cancelRunning')"
      :cancel-label="t('backups.cancelKeep')"
      :perform="flow.performCancel"
      :refusal-text="refusalText"
      destructive
      @close="flow.cancelOpen.value = false"
    >
      <p class="bt__soft" data-backups-cancel-text>{{ t("backups.cancelText") }}</p>
    </AdminActDialog>
  </div>
</template>

<style scoped>
.bt {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  min-width: 0;
}

.bt__head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2) var(--space-3);
}

.bt__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.bt__buttons {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
}

.bt__reason,
.bt__legend {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-5);
  margin: 0;
  padding: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  list-style: none;
}

.bt__soft {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.bt__result,
.bt__state,
.bt__loading {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: var(--space-2);
}

.bt__result > :deep(.callout),
.bt__loading > * {
  align-self: stretch;
}

.bt__running {
  display: flex;
}

.bt__line {
  margin: 0;
  font-family: var(--font-mono);
  font-size: var(--fs-small);
}

.bt__meter {
  max-width: var(--content-max);
  margin: var(--space-2) 0;
}

.bt__paths {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  margin: 0 0 var(--space-2);
  padding: 0;
  list-style: none;
}

.bt__path {
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}
</style>
