import type {
  ServicesEvent as BoundServicesEvent,
  EngineDto,
  ServiceDto,
  ServiceStateDto,
} from "@/bindings";

/**
 * Le moteur de conteneurs et les services d'un serveur (HRT-62, ADR-0038) : types de l'interface et
 * conversions depuis les types générés (`bindings.ts`). Aucune règle ici : la capacité de l'agent, la
 * fusion des lots et le tri par nom sont faits par la liaison (`hearth-link`), l'interface reçoit toujours
 * un état COMPLET. Ni valeur ni nom de variable d'environnement, ni commande de lancement : le protocole de
 * cette tranche n'en porte pas (lecture seule).
 */

export type EngineState = EngineDto["state"];
export type ServiceState = ServiceStateDto;

/** Le moteur de conteneurs d'un serveur. Les dates sont des millisecondes depuis l'époque. */
export interface EngineInfo {
  state: EngineState;
  /** Nom et version annoncés par le moteur, seulement s'il a répondu. */
  name: string | null;
  version: string | null;
  minVersion: string;
  /** Le compte sous lequel l'agent tourne (aide de l'état « Accès refusé à l'agent »). */
  agentAccount: string;
  /** Ce compte est l'administrateur du serveur. */
  agentIsAdmin: boolean;
  host: {
    /** `ID` de `/etc/os-release`, vide si inconnu. */
    osId: string;
    osLike: string[];
    init: "systemd" | "other";
    /** Serveur configuré de façon déclarative (NixOS) : l'aide est un texte de configuration. */
    declarative: boolean;
  };
  lastReadyAt: number | null;
  checkedAt: number;
}

export interface ServicePort {
  hostPort: number;
  containerPort: number;
  protocol: "tcp" | "udp" | "sctp" | "unknown";
  /** `all` : tout le réseau du serveur ; `local` : ce serveur seulement. */
  scope: "all" | "local" | "unknown";
}

/** Un service de la liste. */
export interface ServiceInfo {
  /** Identité stable, opaque : jamais le nom ni l'identifiant du conteneur. */
  key: string;
  name: string;
  /** `unknown` : une origine que ce client ne connaît pas (aucun repère n'est alors montré). */
  origin: "hearth" | "external" | "unknown";
  driver: "engine" | "supervisor" | "unknown" | null;
  /** Nom que le système du serveur donne au service (repère « Géré par le système »). */
  unit: string | null;
  removedOnStop: boolean;
  composeProject: string | null;
  state: ServiceState;
  /** Depuis quand le service est dans cet état (ms). */
  since: number | null;
  createdAt: number | null;
  neverStarted: boolean;
  exitCode: number | null;
  /** Démarrages non demandés des 10 dernières minutes. */
  restarts10m: number;
  image: { ref: string | null; tag: string | null; shortId: string };
  ports: ServicePort[];
  hostNetwork: boolean;
}

/**
 * Ce que reçoit un abonné : l'état complet d'un serveur, ou « cet agent ne gère pas les services » (pas
 * de capacité `services` dans `/hello` : ni sujet ni route envoyés, `metrics` continue). `seq` croît
 * strictement par serveur : le récepteur écarte tout événement dont `seq` n'est pas supérieur au dernier connu.
 */
export type ServicesEvent =
  | { kind: "state"; serverId: string; seq: number; engine: EngineInfo; services: ServiceInfo[] }
  | { kind: "unsupported"; serverId: string; seq: number; agentVersion: string };

/** Une date RFC 3339 en millisecondes ; `null` si absente ou illisible (jamais une date inventée). */
export function parseDate(text: string | null): number | null {
  if (text === null) return null;
  const value = Date.parse(text);
  return Number.isFinite(value) ? value : null;
}

export function toEngine(dto: EngineDto): EngineInfo {
  return {
    state: dto.state,
    name: dto.name,
    version: dto.version,
    minVersion: dto.minVersion,
    agentAccount: dto.agentAccount,
    agentIsAdmin: dto.agentIsAdmin,
    host: dto.host,
    lastReadyAt: parseDate(dto.lastReadyAt),
    checkedAt: parseDate(dto.checkedAt) ?? 0,
  };
}

export function toService(dto: ServiceDto): ServiceInfo {
  return {
    key: dto.key,
    name: dto.name,
    origin: dto.origin,
    driver: dto.driver,
    unit: dto.unit,
    removedOnStop: dto.removedOnStop,
    composeProject: dto.composeProject,
    state: dto.state,
    since: parseDate(dto.since),
    createdAt: parseDate(dto.createdAt),
    neverStarted: dto.neverStarted,
    exitCode: dto.exitCode,
    restarts10m: dto.restarts10m,
    image: { ref: dto.image.ref, tag: dto.image.tag, shortId: dto.image.shortId },
    ports: dto.ports.map((port) => ({ ...port })),
    hostNetwork: dto.hostNetwork,
  };
}

export function toServicesEvent(dto: BoundServicesEvent): ServicesEvent {
  if (dto.view.kind === "unsupported") {
    return {
      kind: "unsupported",
      serverId: dto.serverId,
      seq: dto.seq,
      agentVersion: dto.view.agentVersion,
    };
  }
  return {
    kind: "state",
    serverId: dto.serverId,
    seq: dto.seq,
    engine: toEngine(dto.view.engine),
    services: dto.view.services.map(toService),
  };
}
