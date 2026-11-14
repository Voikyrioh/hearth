<script setup lang="ts">
import { computed } from "vue";
import GameAddressCard from "@/components/molecules/GameAddressCard.vue";
import GamePasswordCard from "@/components/molecules/GamePasswordCard.vue";
import GamePortsCard from "@/components/molecules/GamePortsCard.vue";
import GameStopCard from "@/components/molecules/GameStopCard.vue";
import { useGameView } from "@/composables/useGameView";
import type { ServiceInfo } from "@/link";
import { addressOf } from "@/services/game";
import type { RightsGate } from "@/services/rights";
import { useServersStore } from "@/stores/servers";

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
}>();
const emit = defineEmits<{ retryRights: [] }>();

const servers = useServersStore();
const candidate = computed(
  () =>
    props.supported &&
    props.connected &&
    !props.gone &&
    props.service.origin === "hearth" &&
    props.service.adopted === null &&
    props.service.state !== "deploying",
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
const host = computed(() => servers.byId(props.serverId)?.host ?? "");
const address = computed(() => (view.value ? addressOf(host.value, view.value.ports) : null));
const stopped = computed(
  () => props.service.state === "stopped" || props.service.state === "error",
);
</script>

<template>
  <template v-if="view">
    <GameAddressCard :address="address" :stopped="stopped" />
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
