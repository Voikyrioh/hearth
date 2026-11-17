<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import { safeText } from "@/audit/text";
import { type CatalogFilter, counts, hasSoon, sectionsOf } from "@/catalogue/sections";
import HButton from "@/components/atoms/HButton.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import CatalogToolbar from "@/components/molecules/CatalogToolbar.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import FicheCard from "@/components/molecules/FicheCard.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import { useAgentVersion } from "@/composables/useAgentVersion";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useEngineCapacity } from "@/composables/useEngineCapacity";
import { useFicheCatalog } from "@/composables/useFicheCatalog";
import { usePageData } from "@/composables/usePageData";
import { t } from "@/i18n";
import { useServicesStore } from "@/stores/services";

// Le Catalogue (HRT-102, HRT-241, BR-FICHE-001 à 006, 033, 063, 064) : les serveurs de jeu que CET agent liste, en grille
// illustrée rangée en deux sections, « Prêts à installer » puis « Bientôt dans Hearth », avec recherche et filtres (ni l'un ni
// l'autre n'est gardé en quittant la page). Visible des deux rôles, jamais journalisé, et lisible sans Internet : les
// fiches sont livrées avec l'agent. Une carte « Bientôt » n'a aucun bouton ni clic. Le moteur de conteneurs n'empêche
// pas de lire : s'il n'est pas prêt, son encart est en tête et la page reste lisible. En pied, les fiches que l'agent a
// écartées (leur contenu n'est pas accepté) et celles que ce client ne sait pas lire. Aucune logique réseau ici.
const router = useRouter();
const { server, isConnected } = useCurrentServer();
const services = useServicesStore();
const serverId = () => server.value?.id ?? "";

watch(
  serverId,
  (id) => {
    if (id) void services.start();
  },
  { immediate: true },
);

const { catalog, phase, reload } = useFicheCatalog(serverId, () => isConnected.value);
const agentVersion = useAgentVersion(serverId);
usePageData(() => catalog.value !== null);

const entry = computed(() => (server.value ? services.of(server.value.id) : undefined));
const engine = computed(() => entry.value?.engine ?? null);
const oldAgent = computed(() => entry.value?.support === "unsupported");
const showEngine = computed(() => engine.value !== null && engine.value.state !== "ready");

const fiches = computed(() => catalog.value?.fiches ?? []);
const query = ref("");
const filter = ref<CatalogFilter>("all");
const sections = computed(() => sectionsOf(fiches.value, query.value, filter.value));
const numbers = computed(() => counts(fiches.value, query.value));
const showSoon = computed(() => hasSoon(fiches.value));
// Le filtre « Bientôt » disparaît avec la dernière fiche qui attend : on revient à « Tous les jeux ».
watch(showSoon, (on) => {
  if (!on && filter.value === "soon") filter.value = "all";
});
const limits = useEngineCapacity(
  serverId,
  () => isConnected.value && engine.value?.state === "ready",
  () => undefined,
);
const capacityBytes = computed(() => limits.capacity.value?.memoryBytes ?? null);
const serverName = computed(() => safeText(server.value?.name ?? ""));
// Un jeu qui existe mais que le filtre cache : on le dit et on propose de tout afficher (jamais « il n'existe pas »).
const elsewhere = computed(
  () =>
    filter.value !== "all" &&
    query.value.trim() !== "" &&
    counts(fiches.value, query.value).all > 0,
);
const nothingShown = computed(() => sections.value.ready.length + sections.value.soon.length === 0);
const footer = computed(() =>
  agentVersion.value === null
    ? t("catalogue.footerBare")
    : t("catalogue.footer", { version: safeText(agentVersion.value) }),
);
const refused = computed(() => catalog.value?.refused.length ?? 0);
const unreadable = computed(() => catalog.value?.unreadable ?? 0);

function haveServer() {
  void router.push({ name: "existing-server-help", params: { id: serverId() } });
}

function open(id: string) {
  void router.push({ name: "fiche", params: { id: serverId(), fiche: id } });
}
</script>

<template>
  <div class="cat" data-catalogue>
    <p class="cat__subtitle">{{ t("catalogue.subtitle", { server: serverName }) }}</p>

    <EnginePanel
      v-if="showEngine && engine"
      :engine="engine"
      :counts="null"
      :is-admin="server?.role === 'admin'"
      :previous="entry?.previous ?? null"
      :recheck="server ? services.recheckOf(server.id) : 'idle'"
      @recheck="server && services.recheck(server.id)"
    />

    <Callout v-if="oldAgent" tone="warn" data-catalogue-old-agent>
      {{ t("catalogue.oldAgent") }}
      <template #actions>
        <RouterLink class="cat__link" :to="{ name: 'settings' }">
          {{ t("catalogue.oldAgentLink") }}
        </RouterLink>
      </template>
    </Callout>

    <template v-else>
      <Callout v-if="phase === 'failed' && catalog === null" tone="error" role="alert" data-catalogue-failed>
        {{ t("catalogue.readFailed") }}
        <template #actions>
          <HButton variant="secondary" data-catalogue-retry @click="reload()">
            {{ t("common.retry") }}
          </HButton>
        </template>
      </Callout>

      <EmptyState
        v-else-if="catalog === null && !isConnected"
        :title="t('catalogue.waitingTitle')"
        :text="t('catalogue.waitingText')"
        heading="h2"
        data-catalogue-waiting
      />

      <div
        v-else-if="catalog === null"
        class="cat__skeleton"
        role="status"
        aria-busy="true"
        :aria-label="t('catalogue.loading')"
        data-catalogue-loading
      >
        <HSkeleton v-for="row in 6" :key="row" size="lg" />
      </div>

      <template v-else>
        <EmptyState
          v-if="fiches.length === 0"
          :title="t('catalogue.emptyTitle')"
          :text="t('catalogue.emptyText')"
          heading="h2"
          data-catalogue-empty
        />
        <template v-else>
          <CatalogToolbar
            v-model:query="query"
            v-model:filter="filter"
            :counts="numbers"
            :show-soon="showSoon"
            @have-server="haveServer"
          />
          <Callout v-if="nothingShown" tone="info" data-catalogue-no-match>
            {{
              query.trim() === ""
                ? t("catalogue.emptyList")
                : elsewhere
                  ? t("catalogue.noMatchHere", { text: safeText(query.trim()) })
                  : t("catalogue.noMatch", { text: safeText(query.trim()) })
            }}
            <template v-if="elsewhere" #actions>
              <HButton variant="secondary" size="sm" data-catalogue-show-all @click="filter = 'all'">
                {{ t("catalogue.showAll") }}
              </HButton>
            </template>
          </Callout>
          <div v-else class="cat__sections" data-catalogue-list>
            <section
              v-if="sections.ready.length > 0"
              class="cat__section"
              :aria-label="t('catalogue.readyTitle')"
              data-catalogue-ready
            >
              <div class="cat__titles">
                <h2 class="cat__title">{{ t("catalogue.readyTitle") }}</h2>
                <p class="cat__text">{{ t("catalogue.readyText") }}</p>
              </div>
              <div class="cat__grid">
                <FicheCard
                  v-for="card in sections.ready"
                  :key="card.id"
                  :card="card"
                  :server-id="serverId()"
                  :server-name="serverName"
                  :capacity-bytes="capacityBytes"
                  @open="open(card.id)"
                />
              </div>
            </section>
            <section
              v-if="sections.soon.length > 0"
              class="cat__section"
              :aria-label="t('catalogue.soonTitle')"
              data-catalogue-soon
            >
              <div class="cat__titles">
                <h2 class="cat__title">{{ t("catalogue.soonTitle") }}</h2>
                <p class="cat__text">{{ t("catalogue.soonText") }}</p>
              </div>
              <div class="cat__grid">
                <FicheCard
                  v-for="card in sections.soon"
                  :key="card.id"
                  :card="card"
                  :server-id="serverId()"
                  :server-name="serverName"
                  :capacity-bytes="capacityBytes"
                  @open="open(card.id)"
                />
              </div>
            </section>
          </div>
          <p class="cat__footer" data-catalogue-footer>{{ footer }}</p>
        </template>
        <Callout v-if="refused > 0" tone="warn" data-catalogue-refused>
          {{ t("catalogue.refused", { n: refused }) }}
        </Callout>
        <Callout v-if="unreadable > 0" tone="warn" data-catalogue-unreadable>
          {{ t("catalogue.unreadable", { n: unreadable }) }}
        </Callout>
        <Callout v-if="phase === 'failed'" tone="warn" data-catalogue-stale>
          {{ t("catalogue.readFailed") }}
          <template #actions>
            <HButton variant="secondary" size="sm" @click="reload()">{{ t("common.retry") }}</HButton>
          </template>
        </Callout>
      </template>
    </template>
  </div>
</template>

<style scoped>
.cat {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  width: 100%;
  max-width: var(--column-max);
  min-width: 0;
}

.cat__subtitle {
  margin: 0;
  color: var(--tx-secondary);
}

.cat__sections {
  display: flex;
  flex-direction: column;
  gap: var(--space-5);
}

.cat__section {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

.cat__titles {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--space-1) var(--space-3);
}

.cat__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
}

.cat__text,
.cat__footer {
  margin: 0;
  color: var(--tx-secondary);
}

.cat__footer {
  font-size: var(--fs-small);
}

.cat__grid,
.cat__skeleton {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(var(--card-min), 1fr));
  gap: var(--card-gap);
}

.cat__link {
  color: var(--ac);
  text-decoration: underline;
}
</style>
