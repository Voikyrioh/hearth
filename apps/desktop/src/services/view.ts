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
  /** La dernière liste vue quand le moteur était prêt (BR-SVC-018). */
  lastReady: { engineName: string | null; services: ServiceInfo[]; at: number } | null;
  /** Quand (horloge de ce poste) le dernier événement est arrivé. */
  receivedAt: number;
}

/**
 * Moteur prêt mais liste peut-être plus à jour (`list_degraded` de l'agent) : « Impossible de lire les
 * services » et la liste datée par `readAt`. `readAt` est `null` quand l'agent ne donne pas la date : on ne
 * l'invente pas, la liste est alors seulement annoncée comme non relisible.
 */
export interface Degraded {
  readAt: number | null;
}

export type ListShown =
  | { kind: "none" }
  /** Moteur prêt et au moins un service. */
  | { kind: "live"; services: ServiceInfo[]; degraded: Degraded | null }
  /** Moteur prêt, aucun service (BR-SVC-013). Dégradé : on ne dit pas « aucun service », la liste est inconnue. */
  | { kind: "empty"; degraded: Degraded | null }
  /** Moteur non prêt : la liste précédente, grisée et datée (BR-SVC-018). */
  | { kind: "stale"; services: ServiceInfo[]; since: number };

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

export function show(entry: ServerServices | undefined): Shown {
  if (!entry) return { kind: "loading" };
  if (entry.support === "unsupported") {
    return { kind: "old-agent", agentVersion: entry.agentVersion ?? "" };
  }
  const engine = entry.engine;
  if (!engine || engine.state === "checking") return { kind: "loading" };
  if (engine.state === "ready") {
    const degraded = engine.listDegraded ? { readAt: engine.listReadAt } : null;
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
      list: {
        kind: "stale",
        services: entry.services,
        since: entry.lastReady?.at ?? engine.lastReadyAt ?? entry.receivedAt,
      },
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
