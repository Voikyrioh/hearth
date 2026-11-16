<script setup lang="ts">
import { computed, watch } from "vue";
import { useRouter } from "vue-router";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import AddPathCard from "@/components/molecules/AddPathCard.vue";
import Callout from "@/components/molecules/Callout.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useFicheCatalog } from "@/composables/useFicheCatalog";
import { useServiceRights } from "@/composables/useServiceRights";
import { deployDoor } from "@/deploy/availability";
import { t } from "@/i18n";
import { useServicesStore } from "@/stores/services";

// « Ajouter un serveur de jeu » (HRT-243, BR-DEPLOY-001, 039) : l'écran d'entrée aux trois chemins, dans cet ordre : le
// Catalogue (conseillé), un programme du choix de l'utilisateur (le formulaire d'image, « Continuer sans fiche »), un
// serveur de jeu déjà présent (l'aide existante). Il ne crée RIEN. La route est gardée comme l'était le formulaire : sans
// le droit de gérer les services, retour à la liste. « Continuer sans fiche » est inactif avec sa raison quand le moteur
// n'est pas prêt ; les deux autres boutons sont toujours actifs. Le pied dit l'état du droit et du moteur.
const router = useRouter();
const { server, isConnected } = useCurrentServer();
const services = useServicesStore();
const rights = useServiceRights();
const serverId = () => server.value?.id ?? "";

watch(
  serverId,
  (id) => {
    if (id) void services.start();
  },
  { immediate: true },
);

const { catalog } = useFicheCatalog(serverId, () => isConnected.value);
const entry = computed(() => (server.value ? services.of(server.value.id) : undefined));
const engine = computed(() => entry.value?.engine ?? null);
const door = computed(() =>
  deployDoor({ engine: engine.value?.state ?? null, gate: rights.gate.value }),
);
const engineReady = computed(() => engine.value?.state === "ready");
const showEngine = computed(() => engine.value !== null && engine.value.state !== "ready");
const serverName = computed(() => safeText(server.value?.name ?? ""));

/** Les jeux prêts à installer : trois noms au plus, par ordre alphabétique (BR-DEPLOY-039). */
const readyNames = computed(() =>
  (catalog.value?.fiches ?? [])
    .filter((card) => card.status === "published" && card.installable)
    .map((card) => safeText(card.name))
    .sort((a, b) => a.localeCompare(b, "fr", { sensitivity: "base" }))
    .slice(0, 3),
);

/** « Minecraft Java, Terraria et Valheim » ; sans catalogue lu, la phrase ne cite aucun nom. */
const namesText = computed(() => {
  const names = readyNames.value;
  if (names.length === 0) return t("addGame.pathCatalogNoNames");
  const last = names[names.length - 1] ?? "";
  const head = names.slice(0, -1);
  const list = head.length === 0 ? last : `${head.join(", ")} ${t("addGame.and")} ${last}`;
  return t("addGame.pathCatalogNames", { names: list });
});

const freeReason = computed(() =>
  engineReady.value
    ? undefined
    : door.value.kind === "disabled"
      ? (door.value.reason ?? undefined)
      : t("deploy.engineNotReady"),
);

function id() {
  return { id: serverId() };
}

function openCatalog() {
  void router.push({ name: "catalogue", params: id() });
}

function openFree() {
  if (engineReady.value) void router.push({ name: "services-deploy", params: id() });
}

function openHelp() {
  void router.push({ name: "existing-server-help", params: id() });
}

function close() {
  void router.push({ name: "services", params: id() });
}
</script>

<template>
  <div class="add" data-add-game-server>
    <div class="add__top">
      <p class="add__intro">{{ t("addGame.intro", { server: serverName }) }}</p>
      <HButton variant="secondary" data-add-close @click="close">
        {{ t("deploy.close") }}
      </HButton>
    </div>

    <EnginePanel
      v-if="showEngine && engine"
      :engine="engine"
      :counts="null"
      :is-admin="server?.role === 'admin'"
      :previous="entry?.previous ?? null"
      :recheck="server ? services.recheckOf(server.id) : 'idle'"
      @recheck="server && services.recheck(server.id)"
    />

    <div class="add__grid">
      <AddPathCard
        :title="t('addGame.pathCatalogTitle')"
        :tag="t('addGame.pathCatalogTag')"
        :text="`${namesText} ${t('addGame.pathCatalogText')}`"
        :note="t('addGame.pathCatalogNote')"
        :button="t('addGame.pathCatalogButton')"
        primary
        data-add-path-catalog
        @go="openCatalog"
      />
      <AddPathCard
        :title="t('addGame.pathFreeTitle')"
        :tag="t('addGame.pathFreeTag')"
        :text="t('addGame.pathFreeText')"
        :note="t('addGame.pathFreeNote')"
        :button="t('addGame.pathFreeButton')"
        :disabled="!engineReady"
        :reason="freeReason"
        data-add-path-free
        @go="openFree"
      />
      <AddPathCard
        :title="t('addGame.pathExistingTitle', { server: serverName })"
        :text="t('addGame.pathExistingText')"
        :note="t('addGame.pathExistingNote')"
        :button="t('addGame.pathExistingButton')"
        data-add-path-existing
        @go="openHelp"
      />
    </div>

    <Callout tone="info" data-add-state>
      <p class="add__state">{{ t("addGame.rightYes", { server: serverName }) }}</p>
      <p class="add__state">
        {{ engineReady ? t("addGame.engineReady") : t("addGame.engineNotReady") }}
      </p>
      <p class="add__state">{{ t("addGame.comeBack") }}</p>
    </Callout>
  </div>
</template>

<style scoped>
.add {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  width: 100%;
  min-width: 0;
}

.add__top {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
}

.add__intro {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-lead);
}

.add__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(var(--card-min), 1fr));
  gap: var(--card-gap);
}

.add__state {
  margin: 0;
}
</style>
