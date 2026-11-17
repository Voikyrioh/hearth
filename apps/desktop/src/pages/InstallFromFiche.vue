<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import HSkeleton from "@/components/atoms/HSkeleton.vue";
import Callout from "@/components/molecules/Callout.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import EmptyState from "@/components/molecules/EmptyState.vue";
import InstallAside from "@/components/molecules/InstallAside.vue";
import StepTrail from "@/components/molecules/StepTrail.vue";
import DeployProgressDialog from "@/components/organisms/DeployProgressDialog.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import FicheReview from "@/components/organisms/FicheReview.vue";
import ImposedDialog from "@/components/organisms/ImposedDialog.vue";
import InstallAnswers from "@/components/organisms/InstallAnswers.vue";
import InstallDone from "@/components/organisms/InstallDone.vue";
import InstallResources from "@/components/organisms/InstallResources.vue";
import ServiceLifecycleDialog from "@/components/organisms/ServiceLifecycleDialog.vue";
import { useAgentVersion } from "@/composables/useAgentVersion";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useDeployFlow } from "@/composables/useDeployFlow";
import { useFicheInstall } from "@/composables/useFicheInstall";
import { useFicheLinks } from "@/composables/useFicheLinks";
import { useLeaveGuard } from "@/composables/useLeaveGuard";
import { useNeedsLink } from "@/composables/useNeedsLink";
import { usePageTitle } from "@/composables/usePageTitle";
import { useServiceLifecycle } from "@/composables/useServiceLifecycle";
import { useServiceRights } from "@/composables/useServiceRights";
import { deployDoor } from "@/deploy/availability";
import { shownHostPort } from "@/fiche/draft";
import { memorySentence } from "@/fiche/memory";
import { minimumText } from "@/fiche/messages";
import { packIssueText } from "@/fiche/pack";
import { createdWorld } from "@/fiche/review";
import { notEnoughRoom } from "@/fiche/world";
import { t } from "@/i18n";
import type { ServiceOperation } from "@/link";
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

const edited = () => drafts.edited(serverId(), ficheId());
// La copie d'un monde est une étape du plan : retenue quand le récapitulatif s'ouvre, la saisie étant effacée au lancement.
const resumedWorld = ref(false);
const back = () => ({ name: "fiche" as const, params: { id: serverId(), fiche: ficheId() } });
const guard = useLeaveGuard({
  dirty: edited,
  launching: () => flow.launch.value !== null,
  home: back,
  wipe: () => install.wipe(),
});
// L'installation réussie : la page montre l'écran de fin au lieu de ramener à la liste (HRT-247, BR-FICHE-066). Ce qu'il
// dit (nom du serveur de jeu, monde créé, fiche) est retenu quand la vérification s'affiche : la saisie est effacée au
// lancement.
const finished = ref<{
  key: string;
  name: string;
  world: string | null;
  detail: NonNullable<typeof install.detail.value>;
  operation: ServiceOperation;
  withWorld: boolean;
} | null>(null);
const toRemember = ref<{ world: string | null } | null>(null);
const flow = useDeployFlow({
  serverId,
  source: install.flowSource,
  installing: true,
  navigate: (to) => void guard.leave(to),
  focusField: () => undefined,
  focusQuestion: (id) => answers.value?.focusQuestion(id),
  onSucceeded: (launch, operation) => {
    const current = install.detail.value;
    if (!current || launch.serviceKey === null) return "leave";
    finished.value = {
      key: launch.serviceKey,
      name: launch.name,
      world: toRemember.value?.world ?? null,
      detail: current,
      operation,
      withWorld: resumedWorld.value,
    };
    return "stay";
  },
});

watch(
  () => flow.reviewing.value,
  (open) => {
    if (!open) return;
    resumedWorld.value = draft.value?.world.resume === true;
    toRemember.value = {
      world: detail.value && draft.value ? createdWorld(detail.value, draft.value) : null,
    };
  },
);

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

type IssueTarget =
  | { kind: "name" }
  | { kind: "question"; id: string }
  | { kind: "port"; id: string };

/** Le premier champ en défaut, dans l'ordre de la page : le nom, les questions, les ports. */
const firstIssue = computed<{ label: string; target: IssueTarget } | null>(() => {
  if (install.nameProblem.value !== null) {
    return { label: t("fiche.cardName"), target: { kind: "name" } };
  }
  for (const question of install.questions.value) {
    if (install.answerProblems.value.has(question.id)) {
      return { label: question.label, target: { kind: "question", id: question.id } };
    }
  }
  for (const port of install.published.value) {
    if (install.portProblems.value.has(port.id)) {
      return { label: t("fiche.portFieldLabel"), target: { kind: "port", id: port.id } };
    }
  }
  return null;
});

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
  if (install.worldImport.inspection.value.kind === "ready") {
    // Plus de place que le disque n'en a : aucune case ne permet de passer outre (BR-MONDE-012).
    const measured = install.worldImport.inspection.value.inspection;
    if (draft.value.world.resume && notEnoughRoom(measured)) {
      return { text: t("fiche.world.noRoomReason"), fixable: false };
    }
  }
  if (install.step.value === 1) {
    // Le vrai motif d'un pack pas encore prêt : attendre la fin de l'envoi n'est pas « corriger un champ » (Nora, 16/11).
    const pack = install.packProblem.value;
    if (pack === "sending" || pack === "unsent" || pack === "unread") {
      return { text: packIssueText(pack), fixable: false };
    }
    if (!install.answersInvalid.value && !serverBlocks.value) return null;
    // Le premier champ en défaut est nommé (HRT-244) ; sans champ à nommer (monde, pack, nom pris), la phrase générale.
    const first = firstIssue.value;
    return {
      text: first ? t("fiche.fixField", { label: safeText(first.label) }) : t("fiche.fixAnswers"),
      fixable: true,
    };
  }
  if (install.capacity.value === null) return { text: t("deploy.capacityPending"), fixable: false };
  if (resourcesBad.value) return { text: t("fiche.fixAnswers"), fixable: false };
  if (install.disk.value.kind === "short_measured") {
    return { text: t("fiche.noSpaceReason"), fixable: false };
  }
  return null;
});
const reasonId = "install-reason";

/** Ramène à l'étape « Réponses » (le champ y est) puis met le curseur dans la question que le refus désigne. */
async function goToRefusedQuestion() {
  const question = flow.refusal.value?.question;
  if (!question) return;
  if (install.step.value !== 1) install.back();
  await nextTick();
  await nextTick();
  answers.value?.focusQuestion(question);
}

async function goToFirstIssue() {
  install.touchAll();
  await nextTick();
  const first = firstIssue.value;
  if (first?.target.kind === "question") {
    // Un réglage avancé refusé déplie son bloc avant de recevoir le curseur (BR-FICHE-065).
    if (await answers.value?.focusQuestion(first.target.id)) return;
  }
  if (first?.target.kind === "name") {
    answers.value?.focusName();
    return;
  }
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
  await flow.openReview();
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
    await flow.openReview();
  },
  { immediate: true },
);

// ── Le curseur dans le premier champ (revue UX 2026-10-24, constat 16) ───────────────────────────
// Comme le formulaire de déploiement : on arrive, on tape. Pas pour « Installer avec les valeurs proposées », dont la
// fenêtre prend le focus.
let focusedFirst = false;
watch(
  () => [detail.value, draft.value] as const,
  async ([loaded, current]) => {
    if (focusedFirst || !loaded || !current || route.query.proposees === "1") return;
    focusedFirst = true;
    await nextTick();
    answers.value?.focusName();
  },
  { immediate: true },
);

// ── Fermer, abandonner ──────────────────────────────────────────────────────────────────────────
/**
 * « Retour » ou Échap sur le récapitulatif ouvert par « Installer avec les valeurs proposées » ramène à la fiche, d'où on
 * vient, tant que rien n'a été saisi : le formulaire en trois étapes n'a jamais été demandé (revue UX 2026-10-24,
 * constat 20). Une saisie déjà commencée, ou un refus qui a ramené à la page, reste sur la page.
 */
const proposedEntry = route.query.proposees === "1";
function leaveReview() {
  if (proposedEntry && !edited()) {
    install.wipe();
    void guard.leave(back());
    return;
  }
  flow.leaveReview();
}

/** « Modifier les réglages » depuis la vérification : retour à la première étape, la saisie intacte. */
function editSettings() {
  flow.leaveReview();
  while (install.step.value > 1) install.back();
}

onBeforeUnmount(() => {
  // Quitter la page efface la saisie de la mémoire (BR-FICHE-034).
  install.wipe();
});

// Le moteur devient inutilisable sous la page : retour à la fiche, qui explique. Un droit retiré laisse la page remplie
// et le dit : le serveur a refusé, la saisie n'est pas perdue (BR-FICHE-035).
watch(door, (now) => {
  if (now.kind === "hidden" && flow.launch.value === null && entry.value !== undefined) {
    void guard.leave(back(), true);
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

/** « Ouvrir le serveur de jeu » : le détail du service créé. « Fermer » : la liste des services. */
function openDone() {
  const key = finished.value?.key;
  void guard.leave({
    name: "services",
    params: { id: serverId() },
    query: key ? { service: key } : {},
  });
}
function closeDone() {
  void guard.leave({ name: "services", params: { id: serverId() } });
}
const name = computed(() => safeText(detail.value?.card.name ?? ""));

// L'encart de résumé (HRT-244) : des phrases composées par les règles pures de la fiche, jamais calculées dans le composant.
const serverName = computed(() => safeText(server.value?.name ?? ""));
const asideMemory = computed(() =>
  memorySentence(
    detail.value?.card.memory ?? null,
    install.capacity.value?.memoryBytes ?? null,
    serverName.value,
    "card",
  ),
);
const asideAddress = computed(() => {
  const current = draft.value;
  const host = server.value?.host;
  if (!current || !host) return null;
  const port =
    install.published.value.find((entry) => entry.role === "game") ?? install.published.value[0];
  if (!port) return null;
  const shown = host.includes(":") && !host.startsWith("[") ? `[${host}]` : host;
  return `${safeText(shown)}:${shownHostPort(current, port)}`;
});
// Un seul titre, à chaque étape : « Installer Terraria » (revue UX 2026-10-24, constat 17).
usePageTitle(() => (detail.value ? t("fiche.installTitle", { name: name.value }) : null));
</script>

<template>
  <div class="inst" data-install-page>
    <Teleport v-if="!finished" defer to="#header-actions">
      <HButton variant="secondary" data-install-close @click="guard.requestClose">
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

    <InstallDone
      v-if="finished && server"
      :server-id="server.id"
      :service-key="finished.key"
      :name="finished.name"
      :world="finished.world"
      :detail="finished.detail"
      :operation="finished.operation"
      :with-world="finished.withWorld"
      @open="openDone"
      @close="closeDone"
    />

    <Callout
      v-else-if="install.phase.value === 'failed' && !detail"
      tone="error"
      role="alert"
      data-install-failed
    >
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
      <StepTrail
        :steps="steps"
        :current="flow.reviewing.value ? 2 : install.step.value - 1"
        :label="t('fiche.stepsLabel')"
      />

      <Callout v-if="refusalVisible" tone="error" role="alert" data-install-refusal>
        {{ flow.refusal.value?.text }}
        <template v-if="flow.refusal.value?.question" #actions>
          <HButton variant="secondary" size="sm" data-install-refusal-goto @click="goToRefusedQuestion">
            {{ t("fiche.goToQuestion") }}
          </HButton>
        </template>
      </Callout>
      <Callout
        v-if="install.tooSmallVerdict.value"
        tone="error"
        role="alert"
        data-install-too-small
      >
        {{ minimumText(install.tooSmallVerdict.value) }}
      </Callout>

      <div v-show="!flow.reviewing.value" class="inst__stage">
      <div class="inst__cols">
        <div class="inst__main">
          <InstallAnswers
            v-show="install.step.value === 1"
            ref="answers"
            :install="install"
            @link="(kind, key) => links.open(kind, key)"
          />
          <InstallResources v-if="install.step.value === 2" :install="install" />
        </div>
        <InstallAside
          v-if="install.step.value <= 2"
          class="inst__aside"
          :heading="t('fiche.aside.heading', { game: name, server: serverName })"
          :summary="safeText(detail.card.summary)"
          :memory="asideMemory"
          :address="asideAddress"
        />
      </div>

      <div class="inst__bar" data-install-bar>
        <div class="inst__left">
          <!-- UN seul « Retour » par étape : celui de l'étape d'avant ; « Retour à la fiche » seulement à la première. -->
          <HButton
            v-if="install.step.value === 1"
            variant="ghost"
            size="lg"
            data-install-to-fiche
            @click="guard.requestClose"
          >
            {{ t("fiche.backToFiche") }}
          </HButton>
          <HButton
            v-else
            variant="ghost"
            size="lg"
            data-install-back
            @click="install.back()"
          >
            {{ t("fiche.previous") }}
          </HButton>
        </div>
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
            {{ t("fiche.goToQuestion") }}
          </HButton>
        </div>
        <div class="inst__buttons">
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
      </div>

      <FicheReview
        v-if="server && flow.reviewing.value"
        :server-id="server.id"
        :server-name="serverName"
        :host="server.host"
        :detail="detail"
        :draft="draft"
        :questions="install.questions.value"
        :extras="install.extras.value"
        :world-inspection="
          install.worldImport.inspection.value.kind === 'ready'
            ? install.worldImport.inspection.value.inspection
            : null
        "
        :capacity="install.capacity.value"
        :check="flow.reviewCheck.value"
        :force-low-disk="flow.forceLowDisk.value"
        :agent-version="agentVersion"
        :perform="flow.perform"
        :inert="flow.launch.value !== null"
        @leave="leaveReview"
        @edit="editSettings"
        @agree="install.agreed.value = $event"
        @imposed="imposedOpen = true"
      />
      <ImposedDialog :open="imposedOpen" :settings="detail.imposed" @close="imposedOpen = false" />
    </template>

    <DeployProgressDialog
      v-if="server && flow.launch.value"
      installing
      :with-world="resumedWorld"
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
      :open="guard.abandonOpen.value"
      :title="t('deploy.abandonTitle')"
      :message="t('deploy.abandonText')"
      :confirm-label="t('deploy.abandon')"
      :cancel-label="t('deploy.keepTyping')"
      destructive
      @confirm="guard.abandon"
      @cancel="guard.keepTyping"
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

.inst__stage {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.inst__cols {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  gap: var(--space-4);
}

.inst__main {
  flex: 3 1 var(--form-min);
  min-width: 0;
}

.inst__aside {
  /* Il reste visible pendant la saisie (adresse prévue, mémoire) : il suit le défilement. */
  position: sticky;
  top: var(--space-4);
  align-self: flex-start;
  flex: 1 1 var(--card-min);
}

.inst__bar {
  position: sticky;
  bottom: 0;
  z-index: var(--z-raised);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: var(--space-3) var(--space-4);
  border-radius: var(--radius-card);
  background: var(--card);
  box-shadow: var(--card-edge);
}

.inst__left {
  display: flex;
  align-items: center;
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
  flex-wrap: wrap;
  gap: var(--space-3);
}
</style>
