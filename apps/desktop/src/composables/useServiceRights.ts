import { computed, useId, watch } from "vue";
import { useCurrentServer } from "@/composables/useCurrentServer";
import { canAct, type RightsGate, rightsGate } from "@/services/rights";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";

/**
 * Le droit « gérer les services » du serveur affiché, prêt pour les boutons d'action (HRT-78, BR-GRANT-011,
 * BR-GRANT-014). Lit le droit à l'ouverture de la section et à chaque reconnexion ; `gate` dit pourquoi un bouton
 * est inactif, `canAct` s'il peut agir, `reasonId` est à passer à `ServiceRightReason` et au `describedBy` de
 * chaque bouton qu'il bloque. Les boutons d'action de service (démarrer, arrêter, déployer, adopter…) vivent dans
 * les tickets suivants : ils consomment ceci sans rien recalculer.
 */
export function useServiceRights() {
  const { server, state, isAdmin } = useCurrentServer();
  const store = useServiceRightsStore();
  const services = useServicesStore();
  const reasonId = useId();

  const serverId = computed(() => server.value?.id ?? "");
  const rights = computed(() => store.of(serverId.value));
  const engineState = computed(() =>
    serverId.value ? (services.of(serverId.value)?.engine?.state ?? null) : null,
  );
  const gate = computed<RightsGate>(() =>
    rightsGate({ state: rights.value, isAdmin: isAdmin.value, engine: engineState.value }),
  );

  // À l'ouverture de la section (ou changement de serveur), puis à chaque retour du lien.
  watch(
    serverId,
    (id) => {
      if (id) void store.load(id);
    },
    { immediate: true },
  );
  watch(state, (now, before) => {
    if (now === "connected" && before !== "connected" && serverId.value) {
      void store.load(serverId.value);
    }
  });

  // Un autre compte se connecte à ce serveur : le droit de l'ancien est oublié par le magasin, celui-ci se relit.
  watch(
    () => server.value?.username ?? "",
    (now, before) => {
      if (now !== before && serverId.value && state.value === "connected") {
        void store.load(serverId.value);
      }
    },
  );

  return {
    gate,
    canAct: computed(() => canAct(gate.value)),
    reasonId,
    /** L'agent annonce les réglages de jeu (capacité `game-settings`) : l'onglet « Réglages » peut exister. */
    gameSettings: computed(() => rights.value?.rights?.gameSettings ?? false),
    /** Le compte connecté, tel que l'agent le connaît (à valider avant tout affichage). */
    account: computed(() => rights.value?.rights?.account ?? ""),
    /** Relit le droit tout de suite (« Vérifier à nouveau » sous l'échec de lecture). */
    retry: () => (serverId.value ? store.load(serverId.value) : Promise.resolve()),
    /** Le serveur a refusé un acte faute du droit : relire (BR-GRANT-011). */
    noteRefusal: () => (serverId.value ? store.noteRefusal(serverId.value) : Promise.resolve()),
  };
}
