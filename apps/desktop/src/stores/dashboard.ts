import { defineStore } from "pinia";
import { ref, shallowReactive, watch } from "vue";
import { DEFAULT_WINDOW, SampleRing, type WindowKey } from "@/dashboard/series";
import { logUiError } from "@/errors/report";
import {
  getLinkBridge,
  type MachineEvent,
  type MachineInfo,
  type MachineSample,
  type SampleLevels,
  type Unsubscribe,
} from "@/link";
import { useServersStore } from "./servers";

/** Nouvelle tentative d'abonnement aux mesures : espacement fixe, tant que l'écran est suivi. */
export const MACHINE_RETRY_MS = 5000;

/** Un échantillon et le niveau de chacune de ses mesures. */
export interface Latest {
  sample: MachineSample;
  levels: SampleLevels;
}

/**
 * Ce que le tableau de bord sait d'UN serveur. Les champs sont des références superficielles :
 * un échantillon par seconde remplace `latest` (et ne touche pas `machine` s'il n'a pas changé),
 * les composants ne lisent que ce dont ils ont besoin et ne se redessinent donc pas tous à chaque
 * message. L'anneau d'échantillons n'est PAS réactif (jusqu'à 3 600 objets) : les courbes se
 * redessinent sur `tick`, un compteur incrémenté à chaque échantillon ajouté.
 */
export interface ServerMachine {
  readonly serverId: string;
  /** `null` tant qu'aucune identité n'est arrivée (chargement, ou serveur jamais joint). */
  machine: MachineInfo | null;
  latest: Latest | null;
  /** Échantillons gardés (instantané de 5 minutes puis flux, une heure au plus). */
  readonly ring: SampleRing;
  tick: number;
  /** L'abonnement aux mesures a échoué (nouvelle tentative en cours). */
  failed: boolean;
  /** Le premier abonnement est terminé : sans mesure, on sait que « rien n'est arrivé ». */
  settled: boolean;
}

/**
 * Tableau de bord : les mesures de chaque serveur, alimentées par le pont (`onMachine`). Un
 * serveur est suivi à la première ouverture de son tableau de bord et le reste jusqu'à son retrait
 * (les courbes d'une heure se remplissent même quand on regarde autre chose). Le pont rejoue la
 * dernière vue connue, donc le tableau s'affiche tout de suite, même hors ligne ; les niveaux
 * d'alerte sont décidés par la coquille, ce store n'en connaît aucun seuil.
 *
 * Les mesures reçues alors que le lien n'est pas « Connecté » n'existent pas : la liaison n'en émet
 * plus. C'est `StaleSurface` qui marque ce qui est affiché comme périmé (BR-DASH-009).
 */
export const useDashboardStore = defineStore("dashboard", () => {
  /** Fenêtre des courbes, commune à tous les serveurs (BR-DASH-010 : 5 min par défaut). */
  const windowKey = ref<WindowKey>(DEFAULT_WINDOW);
  const machines = shallowReactive(new Map<string, ServerMachine>());
  const subscriptions = new Map<string, Unsubscribe>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const following = new Set<string>();

  function setWindow(next: WindowKey) {
    windowKey.value = next;
  }

  function of(serverId: string): ServerMachine | undefined {
    return machines.get(serverId);
  }

  function entry(serverId: string): ServerMachine {
    let found = machines.get(serverId);
    if (!found) {
      found = shallowReactive<ServerMachine>({
        serverId,
        machine: null,
        latest: null,
        ring: new SampleRing(),
        tick: 0,
        failed: false,
        settled: false,
      });
      machines.set(serverId, found);
    }
    return found;
  }

  function apply(serverId: string, event: MachineEvent) {
    const target = entry(serverId);
    if (event.kind === "history") {
      // L'heure écoulée avant l'instantané : elle ne comble que ce que l'anneau n'a pas (plus ancien, trous),
      // ne remplace jamais un échantillon reçu en direct et ne touche ni l'identité ni le dernier échantillon
      // (BR-DASH-010).
      target.ring.fill(event.history);
      target.tick += 1;
      return;
    }
    if (event.kind === "view") {
      const { view } = event;
      // Le plus récent déjà connu, AVANT de recoller la vue (FIX:01M4CRD381DKREY9RARJ81E6WH).
      const knownAt = target.ring.last?.at;
      target.ring.merge(view.history);
      const viewLast = view.history.at(-1);
      // Une vue plus ancienne que ce qu'on sait déjà (lecture du disque tardive, ou sans aucun échantillon)
      // ne remet pas l'identité d'hier : celle d'une connexion plus récente prime.
      const stale =
        target.machine !== null &&
        knownAt !== undefined &&
        (viewLast === undefined || viewLast.at < knownAt);
      if (!stale) target.machine = view.machine;
      // Les niveaux de la vue sont ceux de SON dernier échantillon : ils ne remplacent pas ceux d'un
      // échantillon du flux plus récent.
      if (viewLast && view.levels && (!target.latest || viewLast.at >= target.latest.sample.at)) {
        target.latest = { sample: viewLast, levels: view.levels };
      }
      target.tick += 1;
      return;
    }
    // Un échantillon n'est pris que s'il est plus récent que le dernier (rejeu, doublon).
    if (!target.ring.push(event.metrics.sample)) return;
    target.latest = { sample: event.metrics.sample, levels: event.metrics.levels };
    target.tick += 1;
  }

  async function subscribe(serverId: string): Promise<void> {
    const target = entry(serverId);
    try {
      const unsubscribe = await getLinkBridge().onMachine(serverId, (event) =>
        apply(serverId, event),
      );
      if (!following.has(serverId)) {
        unsubscribe();
        return;
      }
      subscriptions.set(serverId, unsubscribe);
      target.failed = false;
    } catch (error) {
      if (!following.has(serverId)) return;
      target.failed = true;
      logUiError(error, "dashboard:subscribe");
      timers.set(
        serverId,
        setTimeout(() => {
          timers.delete(serverId);
          if (following.has(serverId)) void subscribe(serverId);
        }, MACHINE_RETRY_MS),
      );
    } finally {
      target.settled = true;
    }
  }

  /** Suit les mesures d'un serveur (une seule fois, quel que soit le nombre d'appels). */
  function follow(serverId: string): Promise<void> {
    if (following.has(serverId)) return Promise.resolve();
    following.add(serverId);
    entry(serverId);
    return subscribe(serverId);
  }

  /** Cesse de suivre un serveur (retiré du carnet) et libère ses mesures. */
  function forget(serverId: string) {
    following.delete(serverId);
    clearTimeout(timers.get(serverId));
    timers.delete(serverId);
    subscriptions.get(serverId)?.();
    subscriptions.delete(serverId);
    machines.delete(serverId);
  }

  /** Ne garde que les serveurs encore au carnet. */
  function prune(known: readonly string[]) {
    for (const serverId of [...following]) if (!known.includes(serverId)) forget(serverId);
  }

  // Un serveur retiré du carnet n'est plus suivi : ses mesures sont libérées.
  const servers = useServersStore();
  watch(
    () => servers.servers.map((server) => server.id),
    (ids) => {
      if (servers.loaded) prune(ids);
    },
  );

  /** Arrête tout (tests). */
  function reset() {
    for (const serverId of [...following]) forget(serverId);
    windowKey.value = DEFAULT_WINDOW;
  }

  return { windowKey, machines, setWindow, of, follow, forget, prune, reset };
});
