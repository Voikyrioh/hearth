<script setup lang="ts">
import { computed, watch } from "vue";
import { RouterLink, useRouter } from "vue-router";
import HButton from "@/components/atoms/HButton.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import FicheCard from "@/components/molecules/FicheCard.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import { useAgentVersion } from "@/composables/useAgentVersion";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useFicheCatalog } from "@/composables/useFicheCatalog";
import { usePageData } from "@/composables/usePageData";
import { t } from "@/i18n";
import { useServicesStore } from "@/stores/services";

// Le Catalogue (HRT-102, BR-FICHE-001 à 006, 033) : les serveurs de jeu que CET agent propose, en pile verticale simple
// (sans recherche, sans filtre, sans illustration). Visible des deux rôles, jamais journalisé, et lisible sans Internet :
// les fiches sont livrées avec l'agent. La liste dit « Livrée avec Hearth {version} » et rien d'autre sur l'origine
// (BR-FICHE-003) ; il n'y a aucun bouton « Installer » ici (un seul chemin, depuis la fiche). Le moteur de conteneurs
// n'empêche pas de lire : s'il n'est pas prêt, son encart est en tête et la page reste lisible. En pied, les fiches
// que l'agent a écartées (leur contenu n'est pas accepté) et celles que ce client ne sait pas lire. Un agent qui ne
// gère pas les services ne propose aucune fiche et le dit.
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
const refused = computed(() => catalog.value?.refused.length ?? 0);
const unreadable = computed(() => catalog.value?.unreadable ?? 0);

function open(id: string) {
  void router.push({ name: "fiche", params: { id: serverId(), fiche: id } });
}
</script>

<template>
  <div class="cat" data-catalogue>
    <p class="cat__subtitle">{{ t("catalogue.subtitle") }}</p>

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
        <HSkeleton v-for="row in 2" :key="row" size="lg" />
      </div>

      <template v-else>
        <EmptyState
          v-if="fiches.length === 0"
          :title="t('catalogue.emptyTitle')"
          :text="t('catalogue.emptyText')"
          heading="h2"
          data-catalogue-empty
        />
        <section v-else class="cat__list" :aria-label="t('catalogue.label')" data-catalogue-list>
          <FicheCard
            v-for="card in fiches"
            :key="card.id"
            :card="card"
            :agent-version="agentVersion"
            @open="open(card.id)"
          />
        </section>
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

.cat__list,
.cat__skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
}

.cat__link {
  color: var(--ac);
  text-decoration: underline;
}
</style>
