<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import Callout from "@/components/molecules/Callout.vue";
import ModsWarning from "@/components/molecules/ModsWarning.vue";
import AdminActDialog from "@/components/organisms/AdminActDialog.vue";
import type { DepositItem, useModsDeposit } from "@/composables/useModsDeposit";
import { t } from "@/i18n";
import { getLinkBridge, type Unsubscribe } from "@/link";
import { formatSize, ratioOf, statusText } from "@/transfers/messages";

// « Ajouter des mods » (HRT-262, BR-MOD-015, 022, maquette Depot) : choisir des fichiers `.jar` (8 au plus), les voir partir
// dans la zone d'attente (un envoi suivi chacun, repris au besoin par la bande « Transferts »), puis « Contrôler les
// fichiers » quand tous les envois sont finis. Rien n'est écrit sur le serveur avant « Appliquer » ; le mot de passe n'est
// demandé que si l'agent l'exige pour déposer un fichier (élévation de cinq minutes). Un fichier refusé avant l'envoi dit
// pourquoi (pas un `.jar`, neuvième fichier, illisible).
const props = defineProps<{
  serverId: string;
  deposit: ReturnType<typeof useModsDeposit>;
  /** Le compte peut gérer les services : sinon rien ne se choisit. */
  canWrite: boolean;
  connected: boolean;
}>();
const emit = defineEmits<{ back: [] }>();

// Glisser des fichiers .jar sur la fenêtre revient à les choisir (le système donne leurs chemins).
let stopDrop: Unsubscribe | undefined;
let alive = true;
onMounted(async () => {
  const stop = await getLinkBridge().onFilesDropped((paths) => {
    if (props.canWrite) void props.deposit.dropped(paths);
  });
  if (alive) stopDrop = stop;
  else stop();
});
onBeforeUnmount(() => {
  alive = false;
  stopDrop?.();
});

function statusOf(item: DepositItem): string {
  if (item.refused !== null) return item.refused;
  const transfer = props.deposit.transferOf(item);
  if (!transfer) return item.sent === true ? t("mods.deposit.sent") : t("mods.deposit.waiting");
  if (transfer.status === "sending") {
    const ratio = ratioOf(transfer);
    return ratio === null
      ? t("mods.deposit.sending")
      : t("mods.deposit.sendingPercent", { percent: Math.round(ratio * 100) });
  }
  return transfer.status === "done" ? t("mods.deposit.sent") : statusText(transfer);
}

/** Les fichiers dont l'envoi attend un geste : nommés, jamais « un envoi » tout court. */
const stuckNames = computed(() =>
  props.deposit.items.value
    .filter((item) => {
      const status = props.deposit.transferOf(item)?.status;
      return status === "failed" || status === "cancelled" || status === "interrupted";
    })
    .map((item) => item.name),
);

/** « chunky.jar n'est plus dans la zone d'attente » : le fichier est nommé quand on sait lequel. */
function goneText(names: string[]): string {
  if (names.length === 0) return t("mods.deposit.depositGone");
  return t(
    names.length === 1 ? "mods.deposit.depositGoneNamed" : "mods.deposit.depositGoneNamedMany",
    {
      names: names.join(", "),
    },
  );
}

function sizeOf(item: DepositItem): string | null {
  const transfer = props.deposit.transferOf(item);
  return transfer ? formatSize(transfer.size) : null;
}
</script>

<template>
  <section class="md" data-mods-deposit>
    <HButton variant="ghost" size="sm" class="md__back" data-mods-deposit-back @click="emit('back')">
      <HIcon name="arrow-left" size="sm" />
      {{ t("mods.deposit.back") }}
    </HButton>
    <h3 class="md__title">{{ t("mods.deposit.title") }}</h3>
    <p class="md__lead">{{ t("mods.deposit.lead") }}</p>

    <div class="md__zone" data-mods-deposit-zone>
      <HIcon name="upload" class="md__zone-icon" />
      <p class="md__zone-text">{{ t("mods.deposit.zone") }}</p>
      <p class="md__hint">{{ t("mods.deposit.zoneOr") }}</p>
      <HButton
        variant="secondary"
        needs-link
        :disabled="!canWrite"
        :busy="deposit.picking.value"
        data-mods-deposit-choose
        @click="deposit.choose()"
      >
        {{ t("mods.deposit.choose") }}
      </HButton>
      <p class="md__hint">{{ t("mods.deposit.only") }}</p>
    </div>

    <section v-if="deposit.items.value.length > 0" class="md__files" data-mods-deposit-files>
      <h4 class="md__files-title">
        {{ t(deposit.items.value.length === 1 ? "mods.deposit.countOne" : "mods.deposit.count", { n: deposit.items.value.length }) }}
      </h4>
      <ul class="md__list">
        <li
          v-for="item in deposit.items.value"
          :key="item.path"
          :class="['md__file', { 'md__file--refused': item.refused !== null }]"
          data-mods-deposit-file
        >
          <span class="md__name">{{ item.name }}</span>
          <span v-if="sizeOf(item)" class="md__size">{{ sizeOf(item) }}</span>
          <span class="md__status" role="status" data-mods-deposit-status>{{ statusOf(item) }}</span>
          <HButton variant="ghost" size="sm" data-mods-deposit-remove @click="deposit.remove(item)">
            {{ t("mods.deposit.remove") }}
          </HButton>
        </li>
      </ul>
    </section>

    <Callout v-if="deposit.needsGesture.value" tone="warn" data-mods-deposit-gesture>
      {{ t("mods.deposit.needsGesture", { names: stuckNames.join(", ") }) }}
    </Callout>
    <Callout v-if="deposit.check.value.kind === 'failed'" tone="error" role="alert" data-mods-check-failed>
      {{ t("mods.deposit.checkFailed") }}
    </Callout>
    <Callout
      v-else-if="deposit.check.value.kind === 'refused'"
      tone="warn"
      data-mods-check-refused
    >
      {{
        deposit.check.value.refusal.kind === "deposit_unknown"
          ? goneText(deposit.check.value.gone ?? [])
          : deposit.check.value.refusal.kind === "not_applicable"
            ? t("mods.noMods.title")
            : t("mods.gone")
      }}
    </Callout>

    <div class="md__actions">
      <HButton
        variant="primary"
        needs-link
        :disabled="!deposit.allSent.value"
        :busy="deposit.check.value.kind === 'checking'"
        data-mods-deposit-check
        @click="deposit.runCheck()"
      >
        {{ t("mods.deposit.check") }}
      </HButton>
      <HButton
        v-if="deposit.unsent.value.length > 0"
        variant="secondary"
        needs-link
        data-mods-deposit-send
        @click="deposit.sendOpen.value = true"
      >
        {{ t("mods.deposit.send") }}
      </HButton>
      <span v-if="!deposit.allSent.value" class="md__hint" data-mods-deposit-check-note>{{ t("mods.deposit.checkNote") }}</span>
    </div>
    <ModsWarning class="md__warning" />

    <AdminActDialog
      :open="deposit.sendOpen.value"
      :server-id="serverId"
      kind="file_write"
      :title="t('mods.deposit.sendTitle')"
      :submit-label="t('mods.deposit.sendSubmit')"
      :cancel-label="t('mods.deposit.sendLater')"
      :perform="deposit.send"
      :refusal-text="() => deposit.sendRefusalText()"
      @close="deposit.sendOpen.value = false"
    >
      <p class="md__lead" data-mods-deposit-send-help>
        {{ t("mods.deposit.sendHelp", { n: deposit.unsent.value.length }) }}
      </p>
    </AdminActDialog>
  </section>
</template>

<style scoped>
.md {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.md__back {
  align-self: flex-start;
}

.md__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.md__lead,
.md__zone-text,
.md__hint,
.md__warning,
.md__files-title {
  margin: 0;
  color: var(--tx-secondary);
}

.md__hint,
.md__warning {
  font-size: var(--fs-small);
}

.md__zone-icon {
  color: var(--ac);
}

.md__zone-text {
  color: var(--tx);
  font-size: var(--fs-lead);
  font-weight: var(--fw-semibold);
}

.md__zone {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-6) var(--space-5);
  border: var(--border-width) dashed var(--tx-faint);
  border-radius: var(--radius-card);
  background: var(--card);
}

.md__files-title {
  color: var(--tx);
  font-size: var(--fs-body);
  font-weight: var(--fw-semibold);
}

.md__list {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  margin: var(--space-2) 0 0;
  padding: 0;
  list-style: none;
}

.md__file {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-control);
  background: var(--card);
}

.md__file--refused .md__status {
  color: var(--warn);
}

.md__name {
  flex: 1 1 var(--backup-note-min);
  min-width: 0;
  font-family: var(--font-mono);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.md__size,
.md__status {
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.md__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
}
</style>
