import type { EngineInfo, EngineState, ServiceInfo } from "@/link";

/**
 * Ce que l'interface sait d'UN serveur côté services, et ce qu'elle en montre. Aucune règle d'état de service
 * ni de moteur ici : l'agent les décide ; ce module ne choisit que ce qui s'affiche (BR-ENGINE-001, 015, 019,
 * BR-SVC-017, 018).
 */
export interface ServerServices {
  serverId: string;
  /** Le dernier `seq` appliqué : tout événement dont `seq` n'est pas plus grand est écarté. */
  seq: number;
  support: "supported" | "unsupported";
  /** La version de l'agent, seulement s'il ne gère pas les services. */
  agentVersion: string | null;
  engine: EngineInfo | null;
  services: ServiceInfo[];
  /** L'état du moteur avant l'actuel et la dernière fois qu'il a été vu (message « Ne répond pas »). */
  previous: { state: EngineState; at: number } | null;
  /** La dernière liste vue quand le moteur était prêt (BR-SVC-018) ; `at` : sa réception par la liaison. */
  lastReady: { engineName: string | null; services: ServiceInfo[]; at: number | null } | null;
  /** Quand (horloge de ce poste) le dernier événement est arrivé à l'interface (seulement pour « Dernier état connu »). */
  receivedAt: number;
  /**
   * Quand (horloge de ce poste) la LIAISON a reçu la vue du moteur actuelle. Ce repère vient de l'étage qui
   * reçoit le message : un lot qui ne republie pas la vue ne le remet pas à zéro, un état rejoué ou relu du
   * disque garde sa vraie date. `null` : inconnue. L'interface ne le pose jamais elle-même à l'arrivée d'un
   * événement (ce serait l'heure du rejeu, pas celle de la réception).
   */
  engineReceivedAt: number | null;
  /** Services que l'agent a envoyés sans que cette version sache les lire (écartés, comptés ; 0 hors support). */
  unreadable: number;
}

/**
 * Moteur prêt mais liste peut-être plus à jour (`list_degraded` de l'agent) : « Impossible de lire les
 * services » et la liste datée par `currentAt`, le dernier moment où elle était à jour, EXPRIMÉ À L'HORLOGE
 * DE CE POSTE (réception de la vue moins l'âge que l'agent a mesuré avec sa seule horloge) : l'âge affiché
 * est `maintenant - currentAt`, sans jamais comparer deux horloges. `null` quand il n'y a rien à dater
 * (aucune liste lue, ou agent sans date) : on ne l'invente pas, la liste est seulement annoncée non relisible.
 */
export interface Degraded {
  currentAt: number | null;
}

/**
 * Le dernier moment où la liste était à jour, à l'horloge de ce poste (HRT-72). Âge à la réception =
 * `max(0, at - listCurrentAt)`, deux dates de l'agent ; le temps écoulé depuis la réception s'y ajoute par
 * l'horloge de ce poste. Agent sans `at` (plus ancien) : comportement d'avant, sa dernière lecture complète.
 */
export function listCurrentAt(engine: EngineInfo, engineReceivedAt: number | null): number | null {
  if (engine.at === null) return engine.listReadAt;
  if (engine.listCurrentAt === null || engineReceivedAt === null) return null;
  return engineReceivedAt - Math.max(0, engine.at - engine.listCurrentAt);
}

export type ListShown =
  | { kind: "none" }
  /** Moteur prêt et au moins un service. */
  | { kind: "live"; services: ServiceInfo[]; degraded: Degraded | null }
  /** Moteur prêt, aucun service (BR-SVC-013). Dégradé : on ne dit pas « aucun service », la liste est inconnue. */
  | { kind: "empty"; degraded: Degraded | null }
  /** Moteur non prêt : la liste précédente, grisée et datée (BR-SVC-018). */
  | { kind: "stale"; services: ServiceInfo[]; since: number | null };

export type Shown =
  /** Première détection : aucune donnée, ou moteur en « Vérification du moteur… » (BR-ENGINE-019). */
  | { kind: "loading" }
  /** Agent sans la capacité `services` (BR-ENGINE-023). */
  | { kind: "old-agent"; agentVersion: string }
  | { kind: "engine"; engine: EngineInfo; list: ListShown };

/** Le moteur est-il le même qu'à la dernière liste vue ? Sans nom connu (absent, désactivé), non (BR-SVC-018). */
function sameEngine(known: string | null, current: string | null): boolean {
  return known !== null && current !== null && known === current;
}

/**
 * Depuis quand la liste gardée d'un moteur non prêt n'est plus à jour. Agent avec ses dates (HRT-72) : le
 * dernier moment où il l'a dite à jour (`list_current_at`, qui couvre explicitement « moteur plus joignable »),
 * ramené à l'horloge de ce poste par la réception de la vue ; JAMAIS la date du dernier événement reçu (sur
 * un serveur calme, ce serait des heures de trop). Agent sans `at` : la dernière fois que la liaison a vu
 * le moteur prêt, faute de mieux. Rien de datable : `null`, aucune date n'est inventée.
 */
function staleSince(entry: ServerServices, engine: EngineInfo): number | null {
  if (engine.at !== null) return listCurrentAt(engine, entry.engineReceivedAt);
  return entry.lastReady?.at ?? entry.engineReceivedAt;
}

export function show(entry: ServerServices | undefined): Shown {
  if (!entry) return { kind: "loading" };
  if (entry.support === "unsupported") {
    return { kind: "old-agent", agentVersion: entry.agentVersion ?? "" };
  }
  const engine = entry.engine;
  if (!engine || engine.state === "checking") return { kind: "loading" };
  if (engine.state === "ready") {
    const degraded = engine.listDegraded
      ? { currentAt: listCurrentAt(engine, entry.engineReceivedAt) }
      : null;
    return {
      kind: "engine",
      engine,
      list:
        entry.services.length > 0
          ? { kind: "live", services: entry.services, degraded }
          : { kind: "empty", degraded },
    };
  }
  // Moteur non prêt : la liste que l'agent garde (arrêté, accès refusé, muet), sinon la dernière lue prêt
  // pour le MÊME moteur ; jamais une liste d'un autre moteur, jamais présentée comme actuelle.
  if (entry.services.length > 0) {
    return {
      kind: "engine",
      engine,
      list: { kind: "stale", services: entry.services, since: staleSince(entry, engine) },
    };
  }
  const last = entry.lastReady;
  if (last && last.services.length > 0 && sameEngine(last.engineName, engine.name)) {
    return {
      kind: "engine",
      engine,
      list: { kind: "stale", services: last.services, since: last.at },
    };
  }
  return { kind: "engine", engine, list: { kind: "none" } };
}
