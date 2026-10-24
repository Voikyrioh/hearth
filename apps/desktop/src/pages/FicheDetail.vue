<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { RouterLink, useRoute, useRouter } from "vue-router";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HIcon from "@/components/atoms/HIcon.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import ServiceRightReason from "@/components/molecules/ServiceRightReason.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import FicheSheet from "@/components/organisms/FicheSheet.vue";
import ImposedDialog from "@/components/organisms/ImposedDialog.vue";
import { useAgentVersion } from "@/composables/useAgentVersion";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useEngineCapacity } from "@/composables/useEngineCapacity";
import { useFicheDetail } from "@/composables/useFicheDetail";
import { useFicheLinks } from "@/composables/useFicheLinks";
import { usePageData } from "@/composables/usePageData";
import { useServiceRights } from "@/composables/useServiceRights";
import { deployDoor } from "@/deploy/availability";
import { minimumVerdicts, tooSmall } from "@/fiche/fit";
import { minimumText } from "@/fiche/messages";
import { t } from "@/i18n";
import { useServicesStore } from "@/stores/services";

// La fiche d'un jeu (HRT-102, BR-FICHE-003, 004, 006, 008, 012, 021, 033, 036) : tout ce que la fiche offre, lu dans la
// fiche que l'agent propose, avec la preuve de chaque chiffre. Aucune ligne « compte exigé » (une fiche n'en demande
// jamais). « Installer » est le seul bouton principal : actif pour un compte qui a le droit de gérer les services, lien
// « Connecté », moteur « Prêt » et machine assez grande ; sinon visible et désactivé avec la raison écrite dessous
// (moteur inutilisable : absent). « Installer avec les valeurs proposées » ouvre directement « Vérifiez avant de
// lancer ». Une machine trop petite pour un minimum MESURÉ désactive les deux sans case pour passer outre. Le contrat
// et la source d'un chiffre de l'éditeur s'ouvrent dans le navigateur de l'utilisateur, jamais dans Hearth.
const route = useRoute();
const router = useRouter();
const { server, isConnected } = useCurrentServer();
const services = useServicesStore();
const rights = useServiceRights();
const serverId = () => server.value?.id ?? "";
const ficheId = () => (typeof route.params.fiche === "string" ? route.params.fiche : "");

watch(
  serverId,
  (id) => {
    if (id) void services.start();
  },
  { immediate: true },
);

const { detail, phase, reload } = useFicheDetail(serverId, ficheId, () => isConnected.value);
const agentVersion = useAgentVersion(serverId);
usePageData(() => detail.value !== null);

const entry = computed(() => (server.value ? services.of(server.value.id) : undefined));
const engine = computed(() => entry.value?.engine ?? null);
const door = computed(() =>
  deployDoor({ engine: engine.value?.state ?? null, gate: rights.gate.value }),
);
const limits = useEngineCapacity(
  serverId,
  () => isConnected.value && engine.value?.state === "ready",
  () => undefined,
);
const capacity = limits.capacity;

const tooSmallVerdict = computed(() =>
  detail.value ? tooSmall(minimumVerdicts(detail.value, capacity.value)) : null,
);
const unreadable = computed(() => (detail.value?.unreadable ?? 0) > 0);
const installable = computed(() => detail.value?.card.installable === true);

/** Pourquoi « Installer » est inactif (texte écrit dessous) ; `null` : il est actif. Le droit est dit par `ServiceRightReason`. */
const blocked = computed(() => {
  if (door.value.kind === "disabled" && door.value.reason) return door.value.reason;
  if (!installable.value) return t("fiche.notInstallable");
  if (unreadable.value) return t("fiche.unreadableReason");
  if (tooSmallVerdict.value) return t("fiche.tooSmallReason");
  return null;
});
const disabled = computed(() => door.value.kind !== "open" || blocked.value !== null);
const rightsReason = computed(() => door.value.kind === "disabled" && door.value.byRights);
const showButtons = computed(() => door.value.kind !== "hidden");

function install(proposed: boolean) {
  void router.push({
    name: "fiche-install",
    params: { id: serverId(), fiche: ficheId() },
    query: proposed ? { proposees: "1" } : {},
  });
}

const links = useFicheLinks(serverId, ficheId);
const openLink = (kind: "agreement" | "token" | "evidence", key: string) => links.open(kind, key);

const imposedOpen = ref(false);
const delivered = computed(() =>
  agentVersion.value === null
    ? t("catalogue.deliveredWithBare")
    : t("catalogue.deliveredWith", { version: safeText(agentVersion.value) }),
);
</script>

<template>
  <div class="fiche" data-fiche-page>
    <RouterLink class="fiche__back" :to="{ name: 'catalogue', params: { id: serverId() } }" data-fiche-back>
      <HIcon name="arrow-left" size="sm" />
      {{ t("fiche.back") }}
    </RouterLink>

    <EnginePanel
      v-if="engine && engine.state !== 'ready'"
      :engine="engine"
      :counts="null"
      :is-admin="server?.role === 'admin'"
      :previous="entry?.previous ?? null"
      :recheck="server ? services.recheckOf(server.id) : 'idle'"
      @recheck="server && services.recheck(server.id)"
    />

    <Callout v-if="phase === 'failed' && detail === null" tone="error" role="alert" data-fiche-failed>
      {{ t("fiche.readFailed") }}
      <template #actions>
        <HButton variant="secondary" data-fiche-retry @click="reload()">{{ t("common.retry") }}</HButton>
      </template>
    </Callout>

    <EmptyState
      v-else-if="phase === 'unknown'"
      :title="t('fiche.notFoundTitle')"
      :text="t('fiche.notFoundText')"
      heading="h2"
      data-fiche-unknown
    />

    <div v-else-if="detail === null" class="fiche__skeleton" role="status" aria-busy="true" :aria-label="t('fiche.loading')" data-fiche-loading>
      <HSkeleton v-for="row in 2" :key="row" size="lg" />
    </div>

    <template v-else>
      <header class="fiche__head">
        <div class="fiche__title">
          <h2 class="fiche__name" data-fiche-name>{{ safeText(detail.card.name) }}</h2>
          <p class="fiche__summary">{{ safeText(detail.card.summary) }}</p>
          <p class="fiche__delivered" data-fiche-delivered>{{ delivered }}</p>
        </div>
        <div v-if="showButtons" class="fiche__actions">
          <HButton
            size="md"
            data-fiche-install
            :needs-link="true"
            :disabled="disabled"
            :hint="blocked ?? undefined"
            :described-by="rightsReason ? rights.reasonId : undefined"
            @click="install(false)"
          >
            {{ t("fiche.install") }}
          </HButton>
          <HButton
            variant="secondary"
            size="md"
            data-fiche-install-proposed
            :needs-link="true"
            :disabled="disabled"
            :hint="blocked ?? undefined"
            :described-by="rightsReason ? rights.reasonId : undefined"
            @click="install(true)"
          >
            {{ t("fiche.installProposed") }}
          </HButton>
        </div>
      </header>

      <p v-if="blocked && !rightsReason" class="fiche__reason" data-fiche-reason>{{ blocked }}</p>
      <Callout v-if="rightsReason" tone="info" data-fiche-rights>
        <ServiceRightReason :gate="rights.gate.value" :id="rights.reasonId" @retry="rights.retry()" />
      </Callout>

      <Callout v-if="tooSmallVerdict" tone="error" role="alert" data-fiche-too-small>
        {{ minimumText(tooSmallVerdict) }}
      </Callout>
      <Callout v-if="!installable" tone="info" data-fiche-not-installable>
        {{ t("fiche.notInstallable") }}
      </Callout>
      <Callout v-if="unreadable" tone="warn" data-fiche-unreadable>{{ t("fiche.unreadable") }}</Callout>
      <Callout tone="info" data-fiche-trust>{{ t("fiche.trust") }}</Callout>

      <FicheSheet
        :detail="detail"
        :capacity="capacity"
        @source="openLink('evidence', $event)"
        @agreement="openLink('agreement', $event)"
        @imposed="imposedOpen = true"
      />

      <p class="fiche__digest" data-fiche-digest>
        {{ t("fiche.digest") }} :
        <span class="fiche__mono">{{ detail.digest }}</span>
      </p>

      <ImposedDialog :open="imposedOpen" :settings="detail.imposed" @close="imposedOpen = false" />
    </template>
  </div>
</template>

<style scoped>
.fiche {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
  width: 100%;
  max-width: var(--column-max);
  min-width: 0;
}

.fiche__back {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  align-self: flex-start;
  color: var(--ac);
  text-decoration: underline;
}

.fiche__skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
}

.fiche__head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--space-4);
}

.fiche__title {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.fiche__name {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h1);
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.fiche__summary {
  margin: 0;
  color: var(--tx-secondary);
}

.fiche__delivered {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
}

.fiche__actions {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.fiche__reason {
  margin: 0;
  color: var(--tx-secondary);
}

.fiche__digest {
  margin: 0;
  color: var(--tx-secondary);
  font-size: var(--fs-small);
  overflow-wrap: anywhere;
}

.fiche__mono {
  font-family: var(--font-mono);
}
</style>
