<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import ServiceRightReason from "@/components/molecules/ServiceRightReason.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import ServiceDetail from "@/components/organisms/ServiceDetail.vue";
import ServiceTable from "@/components/organisms/ServiceTable.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useNow } from "@/composables/useNow";
import { usePageData } from "@/composables/usePageData";
import { useServiceRights } from "@/composables/useServiceRights";
import { logUiError } from "@/errors/report";
import { t } from "@/i18n";
import { getLinkBridge, type ServiceInfo } from "@/link";
import { countServices } from "@/services/counts";
import { openedService } from "@/services/detail";
import { shortAge } from "@/services/format";
import { show } from "@/services/view";
import { useServicesStore } from "@/stores/services";

// La section « Services » d'un serveur, en LECTURE SEULE (HRT-62) : l'état du moteur d'abord (BR-ENGINE-001),
// puis la liste, à jour en direct. Les deux rôles voient la même chose ; seule l'aide à recopier est réservée
// à l'administrateur (BR-ENGINE-016). Aucun bouton d'action sur un service. Quand le lien n'est pas
// « Connecté », le gabarit du serveur grise et date la page (`StaleSurface`) : un seul mécanisme à la fois,
// donc la mention « Liste du moteur d'il y a… » n'apparaît que lien bon (BR-SVC-017, 018).
// Le détail d'un service (HRT-63) s'ouvre à côté de la liste : le service ouvert est dans l'adresse
// (`?service={clé}`, la clé STABLE, jamais le nom), donc un retour arrière ou un rechargement le retrouve. Il
// partage le grisé et la datation de la liste (même conteneur) : le détail ouvert ne ment jamais sur son âge.
const { server, isConnected, isAdmin } = useCurrentServer();
const store = useServicesStore();
const now = useNow();
// Le droit « gérer les services » du compte connecté, lu à l'ouverture et à chaque reconnexion (HRT-78,
// BR-GRANT-011). Aucun bouton d'action de service n'existe encore dans cette section : ceux des tickets suivants
// consommeront `rights.gate`, `rights.reasonId` et `ServiceRightReason` ; en attendant la raison est dite une fois,
// sous la carte du moteur, pour un compte qui ne peut pas agir (BR-GRANT-012, 014, 016).
const rights = useServiceRights();
const route = useRoute();
const router = useRouter();

/** Le service dont le détail est ouvert : sa clé stable, dans l'adresse. Une valeur illisible vaut « aucun ». */
const selectedKey = computed(() => {
  const raw = route.query.service;
  const value = Array.isArray(raw) ? raw[0] : raw;
  return typeof value === "string" && value !== "" && value.length <= 300 ? value : null;
});
/** Le détail a été ouvert par un geste dans cette fenêtre (pas par l'adresse) : le focus va à son titre. */
const openedByGesture = ref(false);
const table = ref<InstanceType<typeof ServiceTable> | null>(null);

function openDetail(key: string) {
  openedByGesture.value = true;
  void router.push({ query: { ...route.query, service: key } });
}

async function closeDetail() {
  const key = selectedKey.value;
  const { service: _closed, ...rest } = route.query;
  await router.push({ query: rest });
  openedByGesture.value = false;
  await nextTick();
  if (key) void table.value?.focusKey(key);
}

// La raison d'un droit manquant est dite une fois : sous la carte du moteur, ou dans la section des variables quand
// un détail est ouvert (même identifiant, jamais deux fois). Lien coupé : le bandeau du gabarit dit déjà tout, pas de
// seconde carte (HRT-137).
const rightsNotice = computed(
  () =>
    isConnected.value &&
    selectedKey.value === null &&
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

/** Les services de la liste montrée (vivante ou grisée), vide si aucune n'est connue. */
const listServices = computed<readonly ServiceInfo[]>(() =>
  list.value.kind === "live" || list.value.kind === "stale" ? list.value.services : [],
);
/** La liste est connue (même vide) : sinon on ne peut pas dire qu'un service a disparu. */
const listKnown = computed(() => list.value.kind !== "none");
/** La dernière version vue du service ouvert : le détail reste lisible s'il disparaît (BR-SVC-016). */
const lastSeen = ref<ServiceInfo | null>(null);
watch(
  [selectedKey, listServices],
  ([key, services]) => {
    if (key === null) {
      lastSeen.value = null;
      return;
    }
    const found = services.find((service) => service.key === key);
    if (found) lastSeen.value = found;
    else if (lastSeen.value && lastSeen.value.key !== key) lastSeen.value = null;
  },
  { immediate: true },
);
const opened = computed(() =>
  selectedKey.value === null
    ? null
    : openedService(listServices.value, selectedKey.value, listKnown.value, lastSeen.value),
);
const engineReady = computed(() => engine.value?.state === "ready");

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

        <!-- La liste (vivante ou grisée) et, à côté, le détail du service ouvert -->
        <template
          v-if="list.kind === 'live' || list.kind === 'stale' || (list.kind === 'empty' && opened)"
        >
          <p
            v-if="list.kind === 'live' && degradedList && degradedAge !== null"
            class="svc__stamp"
            data-services-stale
          >
            <HIcon name="clock" size="sm" />
            {{ t("services.staleList", { age: degradedAge }) }}
          </p>
          <p
            v-else-if="list.kind === 'stale' && staleList && staleAge !== null"
            class="svc__stamp"
            data-services-stale
          >
            <HIcon name="clock" size="sm" />
            {{ t("services.staleList", { age: staleAge }) }}
          </p>
          <div
            :class="[
              'svc__work',
              {
                'svc__work--open': opened,
                'svc__work--grey':
                  (list.kind === 'live' && degradedList) || (list.kind === 'stale' && staleList),
              },
            ]"
            data-services-work
          >
            <div class="svc__list">
              <ServiceTable
                v-if="list.kind !== 'empty'"
                ref="table"
                :services="listServices"
                :selected-key="selectedKey"
                @open="openDetail"
              />
              <EmptyState
                v-else
                :title="t('services.empty')"
                :text="t('services.emptyText')"
                heading="h2"
                data-services-empty
              />
            </div>
            <div v-if="opened && server" class="svc__detail">
              <ServiceDetail
                :key="selectedKey ?? ''"
                :server-id="server.id"
                :opened="opened"
                :gate="rights.gate.value"
                :reason-id="rights.reasonId"
                :connected="isConnected"
                :engine-ready="engineReady"
                :focus-title="openedByGesture"
                @back="closeDetail"
                @open-other="openDetail"
                @retry-rights="rights.retry()"
              />
            </div>
          </div>
        </template>

        <EmptyState
          v-else-if="list.kind === 'empty' && !degraded"
          :title="t('services.empty')"
          :text="t('services.emptyText')"
          heading="h2"
          data-services-empty
        />
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

/* La liste et le détail à côté (HRT-63) : sous 960 px de zone le détail REMPLACE la liste ; au-dessus, la liste passe
   en volet compact et le détail, borné par `--detail-max`, prend le reste. Une seule barre de défilement par volet. */
.svc__work {
  display: flex;
  flex: 1 1 0;
  gap: var(--space-4);
  min-height: 0;
  min-width: 0;
  container: svcwork / inline-size;
}

.svc__list {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.svc__work--open .svc__list {
  flex: 0 1 var(--list-compact-width);
  min-width: 0;
}

.svc__detail {
  display: flex;
  flex: 1 1 0;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  max-width: var(--detail-max);
}

.svc__work--grey {
  filter: grayscale(var(--grayscale-stale));
  opacity: var(--opacity-stale);
}

@container svcwork (max-width: 959px) {
  .svc__work--open .svc__list {
    display: none;
  }

  .svc__work--open .svc__detail {
    max-width: none;
  }
}
</style>
