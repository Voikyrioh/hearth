<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { onBeforeRouteLeave, type RouteLocationNormalized, useRoute, useRouter } from "vue-router";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import StepTrail from "@/components/molecules/StepTrail.vue";
import DeployProgressDialog from "@/components/organisms/DeployProgressDialog.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import FicheSummaryDialog from "@/components/organisms/FicheSummaryDialog.vue";
import ImposedDialog from "@/components/organisms/ImposedDialog.vue";
import InstallAnswers from "@/components/organisms/InstallAnswers.vue";
import InstallResources from "@/components/organisms/InstallResources.vue";
import ServiceLifecycleDialog from "@/components/organisms/ServiceLifecycleDialog.vue";
import { useAgentVersion } from "@/composables/useAgentVersion";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useDeployFlow } from "@/composables/useDeployFlow";
import { useFicheInstall } from "@/composables/useFicheInstall";
import { useFicheLinks } from "@/composables/useFicheLinks";
import { useNeedsLink } from "@/composables/useNeedsLink";
import { useServiceLifecycle } from "@/composables/useServiceLifecycle";
import { useServiceRights } from "@/composables/useServiceRights";
import { deployDoor } from "@/deploy/availability";
import { minimumText } from "@/fiche/messages";
import { t } from "@/i18n";
import { resourcesIssue } from "@/resources";
import { useFicheInstallStore } from "@/stores/ficheInstall";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServicesStore } from "@/stores/services";

// Installer un serveur de jeu depuis une fiche (HRT-102, BR-FICHE-012 à 036) : trois étapes. « Réponses » (variante, nom
// du service, questions de la fiche, ports) et « Ressources » (curseurs, avertissements chiffrés, place) sont des PAGES ;
// « Vérifiez avant de lancer » est la fenêtre large qui porte la confirmation du socle, le mot de passe et le contrat.
// Revenir à une étape précédente ne perd rien. Le parcours qui suit (consultation fraîche, récapitulatif, mot de passe à
// chaque fois, progression, issue, effacement de la saisie) est EXACTEMENT celui d'un déploiement (`useDeployFlow`) : il
// n'est pas recopié. Une machine trop petite pour un minimum MESURÉ le dit avant toute saisie et désactive « Continuer »,
// sans case pour passer outre. La saisie, mot de passe de jeu compris, vit en mémoire seulement et s'efface à l'issue ou à
// l'abandon (BR-FICHE-034). L'adresse `?proposees=1` (« Installer avec les valeurs proposées ») ouvre la fenêtre aussitôt
// que la fiche et les bornes de la machine sont lues ; un champ à corriger la ramène à la page.
const route = useRoute();
const router = useRouter();
const { server, isConnected } = useCurrentServer();
const services = useServicesStore();
const operations = useServiceOpsStore();
const rights = useServiceRights();
const drafts = useFicheInstallStore();
const serverId = () => server.value?.id ?? "";
const ficheId = () => (typeof route.params.fiche === "string" ? route.params.fiche : "");

watch(
  serverId,
  (id) => {
    if (id) {
      void services.start();
      void operations.start();
    }
  },
  { immediate: true },
);

const entry = computed(() => (server.value ? services.of(server.value.id) : undefined));
const engine = computed(() => entry.value?.engine ?? null);
const door = computed(() =>
  deployDoor({ engine: engine.value?.state ?? null, gate: rights.gate.value }),
);
const usable = computed(() => door.value.kind === "open");
/** Les questions au serveur ne partent que lien bon, moteur prêt et droit accordé. */
const ready = computed(() => isConnected.value && usable.value);

const install = useFicheInstall(
  serverId,
  ficheId,
  () => isConnected.value,
  () => ready.value,
);
const agentVersion = useAgentVersion(serverId);
const answers = ref<InstanceType<typeof InstallAnswers> | null>(null);

let allowLeave = false;
const flow = useDeployFlow({
  serverId,
  source: install.flowSource,
  navigate: (to) => {
    allowLeave = true;
    void router.push(to);
  },
  focusField: () => undefined,
  focusQuestion: (id) => answers.value?.focusQuestion(id),
});

// ── Le pied : pourquoi « Continuer » est inactif ────────────────────────────────────────────────
const linkReason = useNeedsLink(() => true);
const detail = install.detail;
const draft = install.draft;

const resourcesBad = computed(() =>
  install.capacity.value === null || draft.value?.resources == null
    ? false
    : resourcesIssue(install.capacity.value, draft.value.resources) !== null,
);
const serverBlocks = computed(() => {
  const answer = install.check.value;
  return answer !== null && (answer.name === "taken" || answer.name === "being_deleted");
});

interface Reason {
  text: string;
  fixable: boolean;
}

/** Le motif du blocage par priorité : le lien, le moteur ou le droit, la fiche, la machine, la saisie. */
const reason = computed<Reason | null>(() => {
  if (linkReason.value) return { text: linkReason.value, fixable: false };
  if (door.value.kind === "disabled" && door.value.reason) {
    return { text: door.value.reason, fixable: false };
  }
  if (rights.gate.value.kind === "denied") return { text: t("rights.lost"), fixable: false };
  if (!detail.value || !draft.value) return { text: t("fiche.loadingReason"), fixable: false };
  if (!detail.value.card.installable) return { text: t("fiche.unavailableReason"), fixable: false };
  if (detail.value.unreadable > 0) return { text: t("fiche.unreadableReason"), fixable: false };
  if (install.tooSmallVerdict.value) return { text: t("fiche.tooSmallReason"), fixable: false };
  if (install.step.value === 1) {
    return install.answersInvalid.value || serverBlocks.value
      ? { text: t("fiche.fixAnswers"), fixable: true }
      : null;
  }
  if (install.capacity.value === null) return { text: t("deploy.capacityPending"), fixable: false };
  if (resourcesBad.value) return { text: t("fiche.fixAnswers"), fixable: false };
  if (install.disk.value.kind === "short_measured") {
    return { text: t("fiche.noSpaceReason"), fixable: false };
  }
  return null;
});
const reasonId = "install-reason";

async function goToFirstIssue() {
  install.touchAll();
  await nextTick();
  const target = document.querySelector<HTMLElement>(
    '[data-install-answers] [aria-invalid="true"]',
  );
  target?.focus();
  target?.scrollIntoView?.({ block: "center" });
}

async function next() {
  if (install.step.value === 1) {
    if (install.next()) await nextTick();
    return;
  }
  await flow.openSummary();
}

// ── « Installer avec les valeurs proposées » ───────────────────────────────────────────────────
let proposedDone = false;
watch(
  () => [draft.value, draft.value?.resources, ready.value] as const,
  async ([current, resources, on]) => {
    if (proposedDone || route.query.proposees !== "1") return;
    if (!current || !resources || !on) return;
    proposedDone = true;
    await nextTick();
    await flow.openSummary();
  },
  { immediate: true },
);

// ── Fermer, abandonner ──────────────────────────────────────────────────────────────────────────
const abandonOpen = ref(false);
let pendingLeave: RouteLocationNormalized | null = null;
const edited = () => drafts.edited(serverId(), ficheId());
const back = () => ({ name: "fiche" as const, params: { id: serverId(), fiche: ficheId() } });

onBeforeRouteLeave((to) => {
  if (allowLeave || flow.launch.value !== null) return true;
  if (!edited()) return true;
  pendingLeave = to;
  abandonOpen.value = true;
  return false;
});

function requestClose() {
  if (edited()) {
    pendingLeave = null;
    abandonOpen.value = true;
    return;
  }
  allowLeave = true;
  void router.push(back());
}

function abandon() {
  install.wipe();
  abandonOpen.value = false;
  allowLeave = true;
  void router.push(pendingLeave ?? back());
}

function keepTyping() {
  abandonOpen.value = false;
  pendingLeave = null;
}

/** Échap hors d'un champ ferme la page ; dans un champ ou une fenêtre ouverte, rien. */
function onKey(event: KeyboardEvent) {
  if (event.key !== "Escape" || document.querySelector("dialog[open]")) return;
  const target = event.target;
  if (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName))
  ) {
    return;
  }
  requestClose();
}
onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKey);
  // Quitter la page efface la saisie de la mémoire (BR-FICHE-034).
  install.wipe();
});

// Le moteur devient inutilisable sous la page : retour à la fiche, qui explique. Un droit retiré laisse la page remplie
// et le dit : le serveur a refusé, la saisie n'est pas perdue (BR-FICHE-035).
watch(door, (now) => {
  if (now.kind === "hidden" && flow.launch.value === null && entry.value !== undefined) {
    allowLeave = true;
    void router.replace(back());
  }
});

// ── Arrêter un service qui boucle (BR-DEPLOY-020) ───────────────────────────────────────────────
const lifecycle = useServiceLifecycle(serverId, () => entry.value?.services ?? []);
function stopLooping() {
  const found = flow.service.value;
  if (found) void lifecycle.request(found, "stop");
}

// ── Les adresses de la fiche, ouvertes dans le navigateur de l'utilisateur ──────────────────────
const links = useFicheLinks(serverId, ficheId);

const imposedOpen = ref(false);
const refusalVisible = computed(() => flow.refusal.value !== null);
const steps = computed(() => [
  t("fiche.stepAnswers"),
  t("fiche.stepResources"),
  t("fiche.stepVerify"),
]);
const progressOpen = computed(() => flow.launch.value !== null);
const name = computed(() => safeText(detail.value?.card.name ?? ""));
</script>

<template>
  <div class="inst" data-install-page>
    <Teleport defer to="#header-actions">
      <HButton variant="secondary" data-install-close @click="requestClose">
        {{ t("fiche.close") }}
      </HButton>
    </Teleport>

    <EnginePanel
      v-if="engine && engine.state !== 'ready'"
      :engine="engine"
      :counts="null"
      :is-admin="server?.role === 'admin'"
      :previous="entry?.previous ?? null"
      :recheck="server ? services.recheckOf(server.id) : 'idle'"
      @recheck="server && services.recheck(server.id)"
    />

    <Callout v-if="install.phase.value === 'failed' && !detail" tone="error" role="alert" data-install-failed>
      {{ t("fiche.readFailed") }}
      <template #actions>
        <HButton variant="secondary" @click="install.reload()">{{ t("common.retry") }}</HButton>
      </template>
    </Callout>

    <EmptyState
      v-else-if="install.phase.value === 'unknown'"
      :title="t('fiche.notFoundTitle')"
      :text="t('fiche.notFoundText')"
      heading="h2"
      data-install-unknown
    />

    <div v-else-if="!detail || !draft" class="inst__skeleton" role="status" aria-busy="true" :aria-label="t('fiche.loading')" data-install-loading>
      <HSkeleton v-for="row in 2" :key="row" size="lg" />
    </div>

    <template v-else>
      <h2 class="inst__title" data-install-title>{{ t("fiche.installTitle", { name }) }}</h2>
      <StepTrail :steps="steps" :current="install.step.value - 1" :label="t('fiche.stepsLabel')" />

      <Callout v-if="refusalVisible" tone="error" role="alert" data-install-refusal>
        {{ flow.refusal.value?.text }}
      </Callout>
      <Callout
        v-if="install.tooSmallVerdict.value"
        tone="error"
        role="alert"
        data-install-too-small
      >
        {{ minimumText(install.tooSmallVerdict.value) }}
      </Callout>

      <InstallAnswers
        v-show="install.step.value === 1"
        ref="answers"
        :install="install"
        @link="(kind, key) => links.open(kind, key)"
      />
      <InstallResources v-if="install.step.value === 2" :install="install" />

      <div class="inst__bar" data-install-bar>
        <div class="inst__reason">
          <p v-if="reason" :id="reasonId" class="inst__reason-text" data-install-reason>
            {{ reason.text }}
          </p>
          <HButton
            v-if="reason?.fixable"
            variant="ghost"
            size="sm"
            data-install-first-issue
            @click="goToFirstIssue"
          >
            {{ t("deploy.goToField") }}
          </HButton>
        </div>
        <div class="inst__buttons">
          <HButton v-if="install.step.value === 2" variant="secondary" size="lg" data-install-back @click="install.back()">
            {{ t("fiche.previous") }}
          </HButton>
          <HButton
            size="lg"
            data-install-next
            :disabled="reason !== null"
            :busy="flow.opening.value"
            :described-by="reason ? reasonId : undefined"
            @click="next"
          >
            {{ t("fiche.next") }}
          </HButton>
        </div>
      </div>

      <FicheSummaryDialog
        v-if="server"
        :open="flow.summaryOpen.value"
        :server-id="server.id"
        :detail="detail"
        :draft="draft"
        :questions="install.questions.value"
        :extras="install.extras.value"
        :capacity="install.capacity.value"
        :check="flow.summaryCheck.value"
        :force-low-disk="flow.forceLowDisk.value"
        :agent-version="agentVersion"
        :perform="flow.perform"
        @close="flow.closeSummary()"
        @agree="install.agreed.value = $event"
        @imposed="imposedOpen = true"
      />
      <ImposedDialog :open="imposedOpen" :settings="detail.imposed" @close="imposedOpen = false" />
    </template>

    <DeployProgressDialog
      v-if="server && flow.launch.value"
      installing
      :open="progressOpen"
      :server-id="server.id"
      :name="flow.launch.value.name"
      :operation="flow.operation.value"
      :ended="flow.ended.value"
      :unknown="flow.launch.value.unknown"
      :service="flow.service.value"
      :perform-cancel="flow.performCancel"
      @background="flow.background()"
      @modify="flow.modify()"
      @logs="flow.seeLogs()"
      @stop="stopLooping"
      @close="flow.close()"
    />

    <ConfirmDialog
      :open="abandonOpen"
      :title="t('deploy.abandonTitle')"
      :message="t('deploy.abandonText')"
      :confirm-label="t('deploy.abandon')"
      :cancel-label="t('deploy.keepTyping')"
      destructive
      @confirm="abandon"
      @cancel="keepTyping"
    />

    <ServiceLifecycleDialog
      v-if="server && lifecycle.pending.value"
      :open="lifecycle.open.value"
      :server-id="server.id"
      :gesture="lifecycle.pending.value.gesture"
      :service="lifecycle.pending.value.service"
      :current="lifecycle.current.value"
      :locked="false"
      :perform="lifecycle.perform"
      @close="lifecycle.cancel()"
    />
  </div>
</template>

<style scoped>
.inst {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  width: 100%;
  max-width: var(--column-max);
  min-width: 0;
}

.inst__skeleton {
  display: flex;
  flex-direction: column;
  gap: var(--card-gap);
}

.inst__title {
  margin: 0;
  font-family: var(--font-title);
  font-size: var(--fs-h3);
  font-weight: var(--fw-semibold);
  overflow-wrap: anywhere;
}

.inst__bar {
  position: sticky;
  bottom: 0;
  z-index: var(--z-raised);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.inst__reason {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
  min-width: 0;
}

.inst__reason-text {
  margin: 0;
  color: var(--tx-secondary);
}

.inst__buttons {
  display: flex;
  gap: var(--space-3);
}
</style>
