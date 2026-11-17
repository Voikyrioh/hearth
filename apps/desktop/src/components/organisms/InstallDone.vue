<script setup lang="ts">
import { computed, ref } from "vue";
import { safeText } from "@/audit/text";
import HButton from "@/components/atoms/HButton.vue";
import Callout from "@/components/molecules/Callout.vue";
import GameAddressCard from "@/components/molecules/GameAddressCard.vue";
import GamePortsCard from "@/components/molecules/GamePortsCard.vue";
import ProgressSteps, { type ProgressStep } from "@/components/molecules/ProgressSteps.vue";
import PublicAddressDialog from "@/components/organisms/PublicAddressDialog.vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { useGameView } from "@/composables/useGameView";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import {
  DEPLOY_STEPS,
  DEPLOY_STEPS_WITH_WORLD,
  type FicheDetail,
  getLinkBridge,
  type ServiceOperation,
} from "@/link";
import { stepName } from "@/serviceOps/messages";
import { addressesOf } from "@/services/game";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";

// L'écran de fin d'une installation par fiche (HRT-247, BR-FICHE-066) : le serveur de jeu est créé, il ne reste qu'à
// donner l'adresse aux joueurs. Il dit « en marche » ou, quand la fiche déclare un premier démarrage long, « démarre » avec
// la durée annoncée (la phase du jeu n'est pas encore lue : HRT-197). Les adresses sont celles de la carte d'adresse du
// détail (réseau maison et internet, « Copier »), lues sur l'agent ; les ports à ouvrir sont ceux du service tel que l'agent
// les publie. Rien n'est inventé : une lecture qui échoue se dit, avec « Réessayer », et le reste de l'écran est là. Les
// étapes de l'installation sont repliées. Aucun mot de passe, aucune valeur de réglage n'est ici : la saisie est effacée.
const props = defineProps<{
  serverId: string;
  /** La clé du service créé. */
  serviceKey: string;
  /** Le nom du serveur de jeu, tel qu'il a été installé. */
  name: string;
  /** Le nom du monde que le jeu a créé ; `null` quand la fiche n'en nomme pas ou que le monde est repris. */
  world: string | null;
  detail: FicheDetail;
  /** La dernière image de l'opération d'installation. */
  operation: ServiceOperation;
  withWorld: boolean;
}>();
const emit = defineEmits<{ open: []; close: [] }>();

const servers = useServersStore();
const host = computed(() => servers.byId(props.serverId)?.host ?? "");
const game = useGameView(
  () => props.serverId,
  () => props.serviceKey,
  () => props.operation.status,
);
const view = game.view;
const addresses = computed(() =>
  view.value
    ? addressesOf(
        host.value,
        view.value.ports,
        view.value.publicAddress,
        view.value.lanAddress,
        view.value.detectedAddress,
      )
    : null,
);
const failed = computed(() => game.state.value.kind === "failed");

// L'adresse d'internet des joueurs se renseigne ICI, sans quitter l'écran de fin (revue UX v2, N1) : la modification d'un
// service qui ne change que cette adresse, un acte confirmé, jamais rejoué ; l'agent ne recrée rien.
const { isConnected } = useCurrentServer();
const toasts = useToastsStore();
const action = useServerAction(() => props.serverId);
const addressOpen = ref(false);
async function performAddress(address: string, adminPassword: string | null): Promise<ActReport> {
  const id = props.serverId;
  const key = props.serviceKey;
  const outcome = await action.run(
    () => getLinkBridge().setPublicAddress(id, key, address, adminPassword),
    { unknownMessage: "game.address.unknown", passwordMissingIsRefusal: true },
  );
  if (!outcome) {
    if (action.passwordMissing.value) {
      return { kind: "refused", refusal: { kind: "password_required" } };
    }
    return action.failedReport();
  }
  if (outcome.kind === "saved") {
    toasts.push({
      kind: "success",
      message: t(address === "" ? "game.address.cleared" : "game.address.saved"),
    });
    void game.reload();
    return { kind: "done" };
  }
  if (outcome.kind === "unknown") {
    void game.reload();
    return { kind: "unknown" };
  }
  return { kind: "refused", refusal: outcome.refusal };
}

const timeout = computed(() => props.detail.firstStart?.readyTimeoutSeconds ?? null);
/** Une fiche qui déclare un premier démarrage long : le jeu démarre encore, la phase n'est pas lue. */
const starting = computed(() => timeout.value !== null);
const name = computed(() => safeText(props.name));
const title = computed(() =>
  t(starting.value ? "fiche.done.titleStarting" : "fiche.done.titleRunning", { name: name.value }),
);
const text = computed(() => {
  if (starting.value) {
    return t("fiche.done.startingText", { d: Math.max(1, Math.ceil((timeout.value ?? 0) / 60)) });
  }
  return null;
});
// Tant que le jeu démarre, on n'annonce pas un monde créé : il le sera au premier démarrage (revue UX v2, N3).
const next = computed(() => {
  if (props.world === null) return t("fiche.done.next");
  return t(starting.value ? "fiche.done.nextWorldStarting" : "fiche.done.nextWorld", {
    world: safeText(props.world),
  });
});
/** La fiche déclare une console à distance : Hearth ne la publie jamais, il le dit. */
const consoleUnpublished = computed(() =>
  props.detail.ports.some((port) => port.publish === "never"),
);

const plan = computed<readonly string[]>(() =>
  props.withWorld ? DEPLOY_STEPS_WITH_WORLD : DEPLOY_STEPS,
);
const done = computed(
  () =>
    new Set(props.operation.steps.filter((line) => line.state === "done").map((line) => line.step)),
);
const steps = computed<ProgressStep[]>(() =>
  plan.value.map((key) => ({
    key,
    label: stepName(key) ?? key,
    state: done.value.has(key) || props.operation.status === "succeeded" ? "done" : "later",
  })),
);
const stepsTitle = computed(() =>
  t("fiche.done.stepsTitle", { k: steps.value.filter((step) => step.state === "done").length }),
);
</script>

<template>
  <section class="done" data-install-done>
    <header class="done__head">
      <h2 class="done__title" data-done-title>{{ title }}</h2>
      <p v-if="text" class="done__text" data-done-starting>{{ text }}</p>
      <p class="done__text" data-done-next>{{ next }}</p>
    </header>

    <Callout v-if="failed" tone="warn" role="alert" data-done-read-failed>
      {{ t("fiche.done.readFailed") }}
      <template #actions>
        <HButton variant="secondary" data-done-retry @click="game.reload()">
          {{ t("common.retry") }}
        </HButton>
      </template>
    </Callout>
    <GameAddressCard
      v-if="view"
      :addresses="addresses"
      :stopped="false"
      :can-edit="isConnected"
      :reason="isConnected ? undefined : t('game.address.editNeedsLink')"
      @edit="addressOpen = true"
    />
    <PublicAddressDialog
      :open="addressOpen"
      :server-id="serverId"
      :current="view?.publicAddress ?? null"
      :perform="performAddress"
      @close="addressOpen = false"
    />
    <GamePortsCard v-if="view" :ports="view.ports" />
    <p v-if="view && consoleUnpublished" class="done__text" data-done-console>
      {{ t("fiche.done.console") }}
    </p>

    <details class="done__steps" data-done-steps>
      <summary class="done__steps-title">{{ stepsTitle }}</summary>
      <ProgressSteps :steps="steps" :label="t('deploy.progressSteps')" />
    </details>

    <div class="done__bar" data-done-bar>
      <HButton variant="secondary" size="lg" data-done-close @click="emit('close')">
        {{ t("fiche.done.close") }}
      </HButton>
      <HButton size="lg" data-done-open @click="emit('open')">{{ t("fiche.done.open") }}</HButton>
    </div>
  </section>
</template>

<style scoped>
.done {
  display: flex;
  flex-direction: column;
  gap: var(--space-4);
  min-width: 0;
}

.done__head {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.done__title {
  margin: 0;
  font-size: var(--fs-h3);
  overflow-wrap: anywhere;
}

.done__text {
  margin: 0;
  color: var(--tx-secondary);
}

.done__steps-title {
  color: var(--tx);
  cursor: pointer;
  font-weight: var(--fw-semibold);
}

.done__bar {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-3);
}
</style>
