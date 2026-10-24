<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import HButton from "@/components/atoms/HButton.vue";
import ConfirmDialog from "@/components/molecules/ConfirmDialog.vue";
import DeployForm from "@/components/organisms/DeployForm.vue";
import DeployProgressDialog from "@/components/organisms/DeployProgressDialog.vue";
import DeploySummaryDialog from "@/components/organisms/DeploySummaryDialog.vue";
import EnginePanel from "@/components/organisms/EnginePanel.vue";
import ServiceLifecycleDialog from "@/components/organisms/ServiceLifecycleDialog.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useDeployFlow } from "@/composables/useDeployFlow";
import { useDeployForm } from "@/composables/useDeployForm";
import { useLeaveGuard } from "@/composables/useLeaveGuard";
import { useNeedsLink } from "@/composables/useNeedsLink";
import { useServiceLifecycle } from "@/composables/useServiceLifecycle";
import { useServiceRights } from "@/composables/useServiceRights";
import { deployDoor, submitReason } from "@/deploy/availability";
import { isFilled } from "@/deploy/draft";
import { checkBlocker } from "@/deploy/messages";
import { t } from "@/i18n";
import type { DeployField } from "@/link";
import { resourcesIssue } from "@/resources";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServicesStore } from "@/stores/services";

// La page « Déployer un service » (HRT-69, BR-DEPLOY-001 à 009, 014 à 016, 020, 027 à 031, 033) : le formulaire en
// page (pas une fenêtre : cinq sections, jusqu'à 100 variables), puis le récapitulatif et la progression en fenêtres.
// La route est gardée (voir `router`) : un compte sans le droit de gérer les services et un moteur inutilisable sont
// renvoyés vers la liste ; un moteur qui n'est pas « prêt » laisse le formulaire s'ouvrir et l'explique (BR-DEPLOY-027).
// La saisie vit en mémoire seulement (`stores/deployForm.ts`) : jamais dans l'adresse ni sur le disque, effacée au
// lancement réussi et à l'abandon. Fermer un formulaire rempli demande « Abandonner votre saisie ? ».
const { server, isConnected } = useCurrentServer();
const services = useServicesStore();
const operations = useServiceOpsStore();
const rights = useServiceRights();
const serverId = () => server.value?.id ?? "";

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

const form = useDeployForm(serverId, () => ready.value);
const formRef = ref<InstanceType<typeof DeployForm> | null>(null);

function focusField(field: DeployField) {
  formRef.value?.focusField(field);
}
const guard = useLeaveGuard({
  dirty: () => isFilled(form.draft.value),
  launching: () => flow.launch.value !== null,
  home: () => ({ name: "services", params: { id: serverId() } }),
  wipe: () => form.wipe(),
});
const flow = useDeployFlow({
  serverId,
  form,
  navigate: (to) => void guard.leave(to),
  focusField,
});

// ── Le pied : pourquoi « Déployer » est inactif ─────────────────────────────────────────────────
const linkReason = useNeedsLink(() => true);
const serverBlocks = computed(() => {
  const answer = form.check.value;
  return answer !== null && checkBlocker(answer) !== null;
});
const resourcesBad = computed(() =>
  form.capacity.value === null
    ? false
    : resourcesIssue(
        form.capacity.value,
        form.draft.value.resources ?? { memoryBytes: null, cpuMillis: null },
      ) !== null,
);
const reason = computed(() =>
  submitReason({
    link: linkReason.value,
    door: door.value,
    rightsLost: rights.gate.value.kind === "denied",
    capacityReady: form.capacity.value !== null,
    pristine: !isFilled(form.draft.value),
    invalid: form.issues.value.any || serverBlocks.value || resourcesBad.value,
  }),
);

// ── Fermer, abandonner ──────────────────────────────────────────────────────────────────────────
onBeforeUnmount(() => {
  // Quitter la page efface la saisie de la mémoire (BR-DEPLOY-029).
  form.wipe();
});

// Le moteur devient inutilisable sous le formulaire : retour à la liste, qui explique (BR-DEPLOY-027). Un droit
// retiré, lui, laisse le formulaire rempli et le dit : le serveur a refusé, la saisie n'est pas perdue (BR-DEPLOY-030).
watch(door, (now) => {
  if (now.kind === "hidden") {
    if (flow.launch.value === null && entry.value !== undefined) {
      void guard.leave({ name: "services", params: { id: serverId() } }, true);
    }
  }
});

// ── Arrêter un service qui boucle (BR-DEPLOY-020) ────────────────────────────────────────────────
const lifecycle = useServiceLifecycle(serverId, () => entry.value?.services ?? []);
function stopLooping() {
  const found = flow.service.value;
  if (found) void lifecycle.request(found, "stop");
}

const progressOpen = computed(() => flow.launch.value !== null);
</script>

<template>
  <div class="dep" data-deploy-page>
    <Teleport defer to="#header-actions">
      <HButton variant="secondary" data-deploy-close-page @click="guard.requestClose">
        {{ t("deploy.close") }}
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

    <DeployForm
      ref="formRef"
      :form="form"
      :refusal="flow.refusal.value"
      :reason="reason"
      :busy="flow.opening.value"
      @submit="flow.openSummary()"
    />

    <DeploySummaryDialog
      v-if="server"
      :open="flow.summaryOpen.value"
      :server-id="server.id"
      :draft="form.draft.value"
      :check="flow.summaryCheck.value"
      :force-low-disk="flow.forceLowDisk.value"
      :perform="flow.perform"
      @close="flow.closeSummary()"
    />

    <DeployProgressDialog
      v-if="server && flow.launch.value"
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
.dep {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}
</style>
