<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { cloneDraft, emptyDraft, type FilterDraft, hasAnyFilter, sameDraft } from "@/audit/filters";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSpinner from "@/components/atoms/HSpinner.vue";
import HTag from "@/components/atoms/HTag.vue";
import AuditDetailDialog from "@/components/molecules/AuditDetailDialog.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import AuditFilters from "@/components/organisms/AuditFilters.vue";
import AuditTable from "@/components/organisms/AuditTable.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useNow } from "@/composables/useNow";
import { t } from "@/i18n";
import { type AuditEntry, failureMessage } from "@/link";
import { AUDIT_WINDOW_MAX, useAuditStore } from "@/stores/audit";
import { useToastsStore } from "@/stores/toasts";

// Journal d'activité (HRT-14) : réservé aux administrateurs (route `adminOnly`). La page ne porte
// aucune règle : l'agent filtre, cherche et pagine ; `audit/grouping.ts` regroupe les rafales ; le
// store tient la liste (suite contiguë, sans doublon, ordre stable). Les données périmées sont
// marquées par le GABARIT (`StaleSurface`, une seule fois pour toutes les pages) : ici seulement le
// message, l'étiquette et le « Rechargement manuel » propres au journal.
const router = useRouter();
const audit = useAuditStore();
const toasts = useToastsStore();
const { server, isConnected } = useCurrentServer();
const now = useNow();

const draft = ref<FilterDraft>(emptyDraft());
const appliedDraft = ref<FilterDraft>(emptyDraft());
const detail = ref<AuditEntry | null>(null);

const dirty = computed(() => !sameDraft(draft.value, appliedDraft.value));
const active = computed(() => hasAnyFilter(draft.value) || hasAnyFilter(appliedDraft.value));
const unfiltered = computed(() => !hasAnyFilter(appliedDraft.value));
const loading = computed(() => audit.status === "loading");
const empty = computed(() => audit.status !== "loading" && audit.entries.length === 0);
const failed = computed(() => audit.status === "error");
const forbidden = computed(() => audit.failure?.kind === "forbidden");

const count = computed(() => {
  const n = audit.entries.length;
  if (audit.hasMoreBelow) return t("audit.countMore", { n });
  return n === 1 ? t("audit.countOne") : t("audit.count", { n });
});
const newLabel = computed(() =>
  audit.pendingOverflow
    ? t("audit.newEntriesMore", { n: audit.newCount })
    : audit.newCount === 1
      ? t("audit.newEntriesOne")
      : t("audit.newEntries", { n: audit.newCount }),
);

// Un seul serveur à la fois : l'arrivée sur la page ouvre son journal, le départ le referme (rien
// n'est gardé : filtres et liste repartent de zéro, et l'écoute du direct est relâchée).
watch(
  () => server.value?.id,
  (id) => {
    draft.value = emptyDraft();
    appliedDraft.value = emptyDraft();
    detail.value = null;
    if (id) void audit.open(id);
    else void audit.close();
  },
  { immediate: true },
);
onBeforeUnmount(() => void audit.close());

// Le lien revient : ce qui a été manqué se rattrape tout seul (BR-AUDIT-011).
watch(isConnected, (connected, was) => {
  if (connected && was === false) audit.resume();
});

// Le compte n'a plus le droit (rôle abaissé, compte supprimé) : retour au carnet (BR-AUDIT-001).
watch(forbidden, (denied) => {
  if (!denied) return;
  toasts.push({ kind: "error", message: t("audit.revoked") });
  void router.replace("/servers");
});

async function apply() {
  const result = await audit.apply(draft.value, Date.now());
  if (result === "applied") appliedDraft.value = cloneDraft(draft.value);
  else if (result === "failed") toasts.push({ kind: "error", message: t("audit.loadFailed") });
}

async function clear() {
  const ok = await audit.clearFilters();
  if (ok) {
    draft.value = emptyDraft();
    appliedDraft.value = emptyDraft();
  } else {
    toasts.push({ kind: "error", message: t("audit.resetFailed") });
  }
}

const reasonText = computed(() =>
  audit.failure && audit.failure.kind !== "forbidden"
    ? failureMessage(audit.failure)
    : t("audit.loadFailed"),
);
</script>

<template>
  <Teleport defer to="#header-actions">
    <HButton
      needs-link
      tip-placement="end"
      variant="secondary"
      :busy="audit.exporting"
      :disabled="audit.entries.length === 0"
      @click="audit.exportCsv()"
    >
      <HIcon name="download" size="sm" />
      {{ t("pages.export") }}
    </HButton>
  </Teleport>

  <div class="audit">
    <AuditFilters
      v-model:draft="draft"
      :accounts="audit.knownAccounts"
      :dirty="dirty"
      :active="active"
      :busy="loading"
      :now="now"
      @apply="apply"
      @clear="clear"
    />

    <div v-if="!isConnected" class="audit__stale" role="status">
      <HTag tone="warn">{{ t("audit.staleBadge") }}</HTag>
      <span>{{ t("audit.stale") }}</span>
      <HButton variant="ghost" size="sm" :busy="audit.reloading" @click="audit.reloadManually()">
        {{ audit.reloading ? t("audit.reloadingNow") : t("audit.reload") }}
      </HButton>
    </div>

    <div v-if="failed && !forbidden" class="audit__error" role="alert">
      <span>{{ reasonText }}</span>
      <HButton variant="ghost" size="sm" @click="audit.reload()">{{ t("common.retry") }}</HButton>
    </div>

    <div class="audit__meta">
      <span v-if="!empty" class="audit__count">{{ count }}</span>
      <HSpinner v-if="loading" :label="t('audit.loading')" />
    </div>

    <div class="audit__area">
      <HButton
        v-if="audit.newCount > 0 || audit.pendingOverflow"
        class="audit__new"
        variant="primary"
        size="sm"
        @click="audit.showPending()"
      >
        <HIcon name="arrow-up" size="sm" />
        {{ newLabel }}
      </HButton>

      <AuditTable
        v-if="!empty"
        :entries="audit.entries"
        :busy="loading"
        :has-more="audit.hasMoreBelow"
        :loading-more="audit.loadingMore"
        :scroll-signal="audit.topSignal"
        @at-top="audit.setAtTop"
        @load-more="audit.loadMore()"
        @open="detail = $event"
      />
      <EmptyState
        v-else-if="!failed"
        heading="h2"
        :title="unfiltered ? t('audit.emptyAll') : t('audit.emptyFiltered')"
        :text="''"
      >
        <template v-if="!unfiltered" #action>
          <HButton variant="secondary" @click="clear">{{ t("audit.clear") }}</HButton>
        </template>
      </EmptyState>
    </div>

    <p v-if="audit.windowFull" class="audit__note">
      {{ t("audit.windowFull", { n: AUDIT_WINDOW_MAX }) }}
    </p>
    <p class="audit__retention">{{ t("audit.retention") }}</p>

    <p class="audit__sr" aria-live="polite" aria-atomic="true">{{ audit.announcement }}</p>
    <AuditDetailDialog :entry="detail" @close="detail = null" />
  </div>
</template>

<style scoped>
.audit {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
}

.audit__stale,
.audit__error {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  border: var(--border-width) solid var(--bd);
  border-radius: var(--radius-control);
  background: var(--card);
}

.audit__error {
  border-color: var(--crit);
  background: var(--crit-tint);
}

.audit__meta {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  min-height: var(--control-sm);
  color: var(--tx2);
}

.audit__area {
  position: relative;
}

.audit__new {
  position: absolute;
  top: calc(var(--audit-row-height) + var(--space-3));
  left: 50%;
  z-index: var(--z-tooltip);
  transform: translateX(-50%);
}

.audit__note,
.audit__retention {
  color: var(--tx3);
  font-size: var(--fs-small);
}

.audit__sr {
  position: absolute;
  width: var(--border-width);
  height: var(--border-width);
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
