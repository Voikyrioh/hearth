<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { RouterLink } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import ServiceRightReason from "@/components/molecules/ServiceRightReason.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import ServiceTable from "@/components/organisms/ServiceTable.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useNow } from "@/composables/useNow";
import { usePageData } from "@/composables/usePageData";
import { useServiceRights } from "@/composables/useServiceRights";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import { getLinkBridge } from "@/link";
import { countServices } from "@/services/counts";
import { shortAge } from "@/services/format";
import { show } from "@/services/view";
import { useServicesStore } from "@/stores/services";

// La section « Services » d'un serveur, en LECTURE SEULE (HRT-62) : l'état du moteur d'abord (BR-ENGINE-001),
// puis la liste, à jour en direct. Les deux rôles voient la même chose ; seule l'aide à recopier est réservée
// à l'administrateur (BR-ENGINE-016). Aucun bouton d'action sur un service. Quand le lien n'est pas
// « Connecté », le gabarit du serveur grise et date la page (`StaleSurface`) : un seul mécanisme à la fois,
// donc la mention « Liste du moteur d'il y a… » n'apparaît que lien bon (BR-SVC-017, 018).
const { server, isConnected, isAdmin } = useCurrentServer();
const store = useServicesStore();
const now = useNow();
// Le droit « gérer les services » du compte connecté, lu à l'ouverture et à chaque reconnexion (HRT-78,
// BR-GRANT-011). Aucun bouton d'action de service n'existe encore dans cette section : ceux des tickets suivants
// consommeront `rights.gate`, `rights.reasonId` et `ServiceRightReason` ; en attendant la raison est dite une fois,
// sous la carte du moteur, pour un compte qui ne peut pas agir (BR-GRANT-012, 014, 016).
const rights = useServiceRights();
const rightsNotice = computed(() =>
  ["denied", "old-agent", "failed"].includes(rights.gate.value.kind),
);

watch(
  () => server.value?.id,
  (id) => {
    if (id) void store.start();
  },
  { immediate: true },
);

const entry = computed(() => (server.value ? store.of(server.value.id) : undefined));
const shown = computed(() => show(entry.value));
const loading = computed(() => shown.value.kind === "loading");
// Rien n'est arrivé : rien à dater, le gabarit ne pose pas « Vu il y a… » (mécanisme commun, HRT-38).
usePageData(() => entry.value !== undefined);

/** Au-delà de ce délai connecté, une première détection qui ne finit pas (elle dure 5 s au plus) est dite en échec. */
const SLOW_LOADING_MS = 10_000;
const slow = ref(false);
let slowTimer: ReturnType<typeof setTimeout> | undefined;
watch(
  () => loading.value && isConnected.value,
  (waiting) => {
    clearTimeout(slowTimer);
    slow.value = false;
    if (waiting) slowTimer = setTimeout(() => (slow.value = true), SLOW_LOADING_MS);
  },
  { immediate: true },
);
onBeforeUnmount(() => clearTimeout(slowTimer));

/** La lecture est en échec : l'abonnement a échoué, ou la première détection ne finit pas. */
const readFailed = computed(() => (store.failed && entry.value === undefined) || slow.value);
const waiting = computed(() => loading.value && !isConnected.value && !store.failed);

const engineShown = computed(() => (shown.value.kind === "engine" ? shown.value : null));
const engine = computed(() => engineShown.value?.engine ?? null);
const counts = computed(() =>
  engine.value?.state === "ready" ? countServices(entry.value?.services ?? []) : null,
);
const list = computed(() => engineShown.value?.list ?? { kind: "none" as const });
/** Moteur non prêt, lien bon : la liste précédente est grisée et datée par CETTE mention ; lien coupé, c'est le gabarit. */
const staleList = computed(() => list.value.kind === "stale" && isConnected.value);
const staleAge = computed(() =>
  list.value.kind === "stale" && list.value.since !== null
    ? shortAge(now.value - list.value.since)
    : null,
);

/** Moteur prêt, liste non relisible, lien bon : « Impossible de lire les services », liste grisée et datée. */
const degraded = computed(() => {
  const shownList = list.value;
  return (shownList.kind === "live" || shownList.kind === "empty") && shownList.degraded
    ? shownList.degraded
    : null;
});
/** Lien coupé : le gabarit grise et date déjà tout (un seul mécanisme), la liste n'est pas filtrée une seconde fois. */
const degradedList = computed(() => degraded.value !== null && isConnected.value);
const degradedAge = computed(() =>
  degraded.value?.currentAt != null ? shortAge(now.value - degraded.value.currentAt) : null,
);

function recheck() {
  if (server.value) void store.recheck(server.value.id);
}

function retry() {
  slow.value = false;
  void store.retry();
}

async function openDocs() {
  try {
    await getLinkBridge().openEngineDocs();
  } catch (error) {
    logUiError(error, "services:docs");
  }
}
</script>

<template>
  <div class="svc">
    <!-- La lecture est en échec -->
    <Callout v-if="readFailed" tone="error" role="alert" data-services-failed>
      {{ t("services.readFailed") }}
      <template #actions>
        <HButton variant="secondary" data-services-retry @click="retry">
          {{ t("engine.recheck") }}
        </HButton>
      </template>
    </Callout>

    <!-- Rien à montrer tant que le serveur n'est pas joignable -->
    <EmptyState
      v-else-if="waiting"
      :title="t('services.waitingTitle')"
      :text="t('services.waitingText')"
      heading="h2"
    />

    <!-- Agent trop ancien : ni sujet ni route envoyés, `metrics` continue -->
    <Callout
      v-else-if="shown.kind === 'old-agent'"
      tone="warn"
      data-services-old-agent
    >
      {{ t("engine.oldAgent", { version: shown.agentVersion }) }}
      <template #actions>
        <RouterLink class="svc__link" :to="{ name: 'settings' }">
          {{ t("engine.oldAgentLink") }}
        </RouterLink>
      </template>
    </Callout>

    <template v-else>
      <EnginePanel
        :engine="engine"
        :counts="counts"
        :is-admin="isAdmin"
        :previous="entry?.previous ?? null"
        :recheck="server ? store.recheckOf(server.id) : 'idle'"
        @recheck="recheck"
        @docs="openDocs"
      />

      <!-- Le droit de gérer les services : la raison pour laquelle aucune action n'est proposée (BR-GRANT-014, 016) -->
      <Callout v-if="rightsNotice" class="svc__rights" tone="info" data-services-rights>
        <ServiceRightReason :gate="rights.gate.value" :id="rights.reasonId" @retry="rights.retry()" />
      </Callout>

      <!-- Première détection : des lignes esquissées, jamais une fausse liste -->
      <div
        v-if="shown.kind === 'loading'"
        class="svc__skeleton"
        role="status"
        aria-busy="true"
        :aria-label="t('services.loading')"
        data-services-loading
      >
        <div v-for="row in 8" :key="row" class="svc__skeleton-row">
          <HSkeleton size="md" />
          <HSkeleton size="md" />
          <HSkeleton size="sm" />
          <HSkeleton size="sm" />
        </div>
      </div>

      <template v-else>
        <!-- Moteur prêt mais liste non relisible : même présentation que la liste périmée du moteur non prêt -->
        <Callout v-if="degraded" tone="error" role="alert" data-services-degraded>
          {{ t("services.readFailed") }}
          <template #actions>
            <HButton
              variant="secondary"
              :disabled="!isConnected"
              data-services-degraded-recheck
              @click="recheck"
            >
              {{ t("engine.recheck") }}
            </HButton>
          </template>
        </Callout>

        <template v-if="list.kind === 'live'">
          <p v-if="degradedList && degradedAge !== null" class="svc__stamp" data-services-stale>
            <HIcon name="clock" size="sm" />
            {{ t("services.staleList", { age: degradedAge }) }}
          </p>
          <div :class="['svc__stale', { 'svc__stale--grey': degradedList }]">
            <ServiceTable :services="list.services" />
          </div>
        </template>

        <EmptyState
          v-else-if="list.kind === 'empty' && !degraded"
          :title="t('services.empty')"
          text=""
          heading="h2"
          data-services-empty
        />

        <template v-else-if="list.kind === 'stale'">
          <p v-if="staleList && staleAge !== null" class="svc__stamp" data-services-stale>
            <HIcon name="clock" size="sm" />
            {{ t("services.staleList", { age: staleAge }) }}
          </p>
          <div :class="['svc__stale', { 'svc__stale--grey': staleList }]">
            <ServiceTable :services="list.services" />
          </div>
        </template>
      </template>
    </template>
  </div>
</template>

<style scoped>
.svc {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

/* C5 : le bandeau de raison est une colonne de lecture bornée (`--panel-max`), aligné à gauche, pas toute la page à 2 560. */
.svc__rights {
  align-self: flex-start;
  width: 100%;
  max-width: var(--panel-max);
}

.svc__link {
  color: var(--ac);
  text-decoration: underline;
}

.svc__skeleton {
  display: flex;
  flex-direction: column;
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.svc__skeleton-row {
  display: grid;
  grid-template-columns: var(--svc-columns);
  align-items: center;
  gap: var(--space-3);
  height: var(--row-service);
  padding: 0 var(--space-4);
  border-bottom: var(--border-width) solid var(--bd);
}

.svc__stamp {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  margin: 0;
  color: var(--tx2);
}

.svc__stale {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  min-height: 0;
}

.svc__stale--grey {
  filter: grayscale(var(--grayscale-stale));
  opacity: var(--opacity-stale);
}
</style>
