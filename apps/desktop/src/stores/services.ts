import { defineStore } from "pinia";
import { ref, shallowReactive, watch } from "vue";
import { logUiError } from "@/errors/report";
import { getLinkBridge, type ServicesEvent, type Unsubscribe } from "@/link";
import type { ServerServices } from "@/services/view";
import { useServersStore } from "./servers";

/** Nouvelle tentative d'abonnement aux services : espacement fixe, tant que l'écran est suivi. */
export const SERVICES_RETRY_MS = 5000;

/** Un « Vérifier à nouveau » en cours, ou qui a échoué (le dire sous la carte). */
export type RecheckState = "idle" | "busy" | "failed";

/**
 * Le moteur de conteneurs et les services de chaque serveur, alimentés par le pont (`onServices`). La
 * liaison fusionne les lots et rend toujours un état COMPLET : ce magasin ne garde que le dernier, avec ce qu'il
 * faut pour dire ce qui était vrai avant (l'état précédent du moteur, la dernière liste vue prêt). Aucune règle
 * de moteur ni d'état de service ici : l'agent les décide, `services/view.ts` ne choisit que ce qui s'affiche.
 *
 * Les événements reçus alors que le lien n'est pas « Connecté » n'existent pas : la liaison n'en émet plus.
 * C'est `StaleSurface` (gabarit du serveur) qui marque ce qui est affiché comme périmé (BR-SVC-017).
 */
export const useServicesStore = defineStore("services", () => {
  const entries = shallowReactive(new Map<string, ServerServices>());
  const recheckStates = shallowReactive(new Map<string, RecheckState>());
  /** L'abonnement a échoué (la lecture de la liste est en échec, nouvelle tentative en cours). */
  const failed = ref(false);
  /** Le premier abonnement est terminé : sans événement, on sait que « rien n'est arrivé ». */
  const settled = ref(false);
  let unsubscribe: Unsubscribe | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let following = false;
  let pending: Promise<void> | null = null;

  function of(serverId: string): ServerServices | undefined {
    return entries.get(serverId);
  }

  function recheckOf(serverId: string): RecheckState {
    return recheckStates.get(serverId) ?? "idle";
  }

  /** Applique un événement de la liaison. Un `seq` qui n'est pas plus grand que le dernier est écarté (rejeu, doublon). */
  function apply(event: ServicesEvent, now: number = Date.now()): void {
    const known = entries.get(event.serverId);
    if (known && event.seq <= known.seq) return;
    if (event.kind === "unsupported") {
      entries.set(event.serverId, {
        serverId: event.serverId,
        seq: event.seq,
        support: "unsupported",
        agentVersion: event.agentVersion,
        engine: null,
        services: [],
        previous: null,
        lastReady: null,
        receivedAt: now,
        engineReceivedAt: null,
        unreadable: 0,
      });
      return;
    }
    const before = known?.engine ?? null;
    // L'état d'avant, tel qu'il a été vu pour la dernière fois : le dernier événement qui le portait.
    const previous =
      before && before.state !== event.engine.state
        ? { state: before.state, at: known?.receivedAt ?? now }
        : (known?.previous ?? null);
    const lastReady =
      event.engine.state === "ready"
        ? { engineName: event.engine.name, services: event.services, at: event.engineReceivedAt }
        : (known?.lastReady ?? null);
    entries.set(event.serverId, {
      serverId: event.serverId,
      seq: event.seq,
      support: "supported",
      agentVersion: null,
      engine: event.engine,
      services: event.services,
      previous,
      lastReady,
      receivedAt: now,
      // Le repère de réception est celui de la LIAISON (elle seule sait quand le message est arrivé, y
      // compris pour un état rejoué ou relu du disque) : jamais l'heure à laquelle ce magasin le traite.
      engineReceivedAt: event.engineReceivedAt,
      unreadable: event.unreadable,
    });
  }

  async function subscribe(): Promise<void> {
    try {
      const stop = await getLinkBridge().onServices((event) => apply(event));
      if (!following) {
        stop();
        return;
      }
      unsubscribe = stop;
      failed.value = false;
    } catch (error) {
      if (!following) return;
      failed.value = true;
      logUiError(error, "services:subscribe");
      timer = setTimeout(() => {
        timer = undefined;
        if (following) pending = subscribe();
      }, SERVICES_RETRY_MS);
    } finally {
      settled.value = true;
    }
  }

  /** Suit le moteur et les services de tous les serveurs (un seul abonnement, quel que soit le nombre d'appels). */
  function start(): Promise<void> {
    if (following) return pending ?? Promise.resolve();
    following = true;
    pending = subscribe();
    return pending;
  }

  /** Reprend l'abonnement tout de suite (« Vérifier à nouveau » sous l'erreur de lecture). */
  function retry(): Promise<void> {
    clearTimeout(timer);
    timer = undefined;
    unsubscribe?.();
    unsubscribe = null;
    following = true;
    settled.value = false;
    pending = subscribe();
    return pending;
  }

  /**
   * « Vérifier à nouveau » : la détection du moteur est relancée tout de suite. L'état publié arrive par le flux ;
   * l'échec d'un appel (lien tombé, agent muet) est dit sous la carte, jamais une erreur du moteur inventée.
   */
  async function recheck(serverId: string): Promise<void> {
    if (recheckOf(serverId) === "busy") return;
    recheckStates.set(serverId, "busy");
    try {
      await getLinkBridge().recheckEngine(serverId);
      recheckStates.set(serverId, "idle");
    } catch (error) {
      recheckStates.set(serverId, "failed");
      logUiError(error, "services:recheck");
    }
  }

  /** Cesse de suivre un serveur retiré du carnet et libère ce qu'on en tenait. */
  function forget(serverId: string): void {
    entries.delete(serverId);
    recheckStates.delete(serverId);
  }

  // Un serveur retiré du carnet est oublié.
  const servers = useServersStore();
  watch(
    () => servers.servers.map((server) => server.id),
    (ids) => {
      if (!servers.loaded) return;
      for (const serverId of [...entries.keys()]) if (!ids.includes(serverId)) forget(serverId);
    },
  );

  /** Arrête tout (tests). */
  function reset(): void {
    following = false;
    clearTimeout(timer);
    timer = undefined;
    unsubscribe?.();
    unsubscribe = null;
    pending = null;
    entries.clear();
    recheckStates.clear();
    failed.value = false;
    settled.value = false;
  }

  return {
    entries,
    failed,
    settled,
    of,
    recheckOf,
    apply,
    start,
    retry,
    recheck,
    forget,
    reset,
  };
});
