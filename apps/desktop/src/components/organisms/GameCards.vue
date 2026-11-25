<script setup lang="ts">
import { computed, ref } from "vue";
import GameAddressCard from "@/components/molecules/GameAddressCard.vue";
import GamePasswordCard from "@/components/molecules/GamePasswordCard.vue";
import GamePortsCard from "@/components/molecules/GamePortsCard.vue";
import GameStopCard from "@/components/molecules/GameStopCard.vue";
import PlayersOnlineCard from "@/components/molecules/PlayersOnlineCard.vue";
import PublicAddressDialog from "@/components/organisms/PublicAddressDialog.vue";
import { useGameView } from "@/composables/useGameView";
import { useNow } from "@/composables/useNow";
import { useOnlinePlayers } from "@/composables/useOnlinePlayers";
import type { ActReport } from "@/composables/useReauth";
import { useServerAction } from "@/composables/useServerAction";
import { t } from "@/i18n";
import { getLinkBridge, type ServiceInfo } from "@/link";
import { addressesOf, mayBeGame } from "@/services/game";
import { canAct, type RightsGate } from "@/services/rights";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";

// Les quatre cartes de plus d'un serveur de jeu, AU-DESSUS de « Général » (HRT-104, design lots suivants 1.6) : l'adresse
// à donner aux joueurs, les ports à ouvrir, le mot de passe du serveur et la façon dont le jeu s'arrête. Elles n'existent
// que pour un service créé depuis une fiche (BR-JEU-001) : un service adopté, déployé à la main ou en cours de
// déploiement n'en montre aucune, même s'il fait tourner le même jeu. La liste des services ne dit pas qu'un service
// vient d'une fiche : la vue est lue à l'ouverture du détail (`GET /services/{key}/game`), qui répond « pas un jeu » pour
// un service déployé à la main. La relecture suit l'état du service, ses ports et l'opération qui le tient. En cas
// d'échec de lecture, rien n'est inventé : aucune carte, et la prochaine relecture réessaie.
const props = defineProps<{
  serverId: string;
  service: ServiceInfo;
  gate: RightsGate;
  /** L'`id` de la raison du droit que le détail écrit déjà, une seule fois (BR-GRANT-014). */
  reasonId: string;
  connected: boolean;
  engineReady: boolean;
  gone: boolean;
  /** L'agent sert les fonctions de jeu (`game-settings` annoncée) : sinon la vue n'est même pas demandée. */
  supported: boolean;
  /** L'agent annonce `game-console` : la carte des joueurs connectés peut exister (HRT-291). */
  consoleServed: boolean;
  /** L'onglet « Console » existe : « Écrire aux joueurs dans la console » est proposé (HRT-291). */
  consoleOpen?: boolean;
}>();
const emit = defineEmits<{ retryRights: []; openConsole: [] }>();

const servers = useServersStore();
const candidate = computed(
  () => props.supported && props.connected && !props.gone && mayBeGame(props.service),
);
const game = useGameView(
  () => props.serverId,
  () => (candidate.value ? props.service.key : null),
  () =>
    [
      props.service.state,
      props.service.ports.map((port) => `${port.hostPort}/${port.containerPort}`).join(","),
      props.service.busy?.kind ?? "",
    ].join("|"),
);

const view = game.view;

// Les joueurs connectés (HRT-291, BR-CONSOLE-011) : lus seulement pour un serveur de jeu dont l'agent parle à la console,
// relus toutes les 30 secondes tant que la vue d'ensemble est ouverte. Sans console parlée : aucune carte.
const clock = useNow();
const players = useOnlinePlayers(
  () => props.serverId,
  () => (props.consoleServed && view.value ? props.service.key : null),
);
const toasts = useToastsStore();
const action = useServerAction(() => props.serverId);

// ── L'adresse des joueurs (HRT-215, BR-JEU-022) ─────────────────────────────────────────────────
// La modification d'un service qui ne change que cette adresse : un acte confirmé, jamais couvert par l'élévation, jamais
// rejoué ; l'agent ne recrée rien. La carte relit la vue à la réussite (et au résultat inconnu : la vue dit ce qui s'est passé).
const addressOpen = ref(false);
const canEditAddress = computed(
  () => canAct(props.gate) && props.connected && props.engineReady && !props.gone,
);

async function performAddress(address: string, adminPassword: string | null): Promise<ActReport> {
  const id = props.serverId;
  const key = props.service.key;
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
const host = computed(() => servers.byId(props.serverId)?.host ?? "");
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
/** Pourquoi « Modifier l'adresse d'internet » est inactif ; jamais un bouton grisé sans un mot (revue UX v2, N1). */
const editReason = computed(() => {
  if (canEditAddress.value) return undefined;
  if (!props.connected) return t("game.address.editNeedsLink");
  if (!canAct(props.gate)) return t("game.address.editNeedsRight");
  if (!props.engineReady) return t("game.address.editNeedsEngine");
  return undefined;
});
const stopped = computed(
  () => props.service.state === "stopped" || props.service.state === "error",
);
</script>

<template>
  <template v-if="view">
    <PlayersOnlineCard
      :state="players.state.value"
      :now="clock"
      :refreshing="players.refreshing.value"
      :link-label="consoleOpen ? t('players.writeToPlayers') : undefined"
      @refresh="players.refresh()"
      @open-console="emit('openConsole')"
    />
    <GameAddressCard
      :addresses="addresses"
      :stopped="stopped"
      :can-edit="canEditAddress"
      :reason="editReason"
      @edit="addressOpen = true"
    />
    <PublicAddressDialog
      :open="addressOpen"
      :server-id="serverId"
      :current="view.publicAddress"
      :perform="performAddress"
      @close="addressOpen = false"
    />
    <GamePortsCard :ports="view.ports" />
    <GamePasswordCard
      :server-id="serverId"
      :service-key="service.key"
      :has-password="view.hasPassword"
      :gate="gate"
      :reason-id="reasonId"
      :connected="connected"
      :engine-ready="engineReady"
      :gone="gone"
      @retry-rights="emit('retryRights')"
    />
    <GameStopCard :view="view" />
  </template>
</template>
