import type {
  ServicesEvent as BoundServicesEvent,
  EngineDto,
  EnvRevealDto,
  ServiceDto,
  ServiceRightsDto,
  ServiceStateDto,
} from "@/bindings";

/**
 * Le moteur de conteneurs et les services d'un serveur (HRT-62, ADR-0038) : types de l'interface et
 * conversions depuis les types générés (`bindings.ts`). Aucune règle ici : la capacité de l'agent, la
 * fusion des lots et le tri par nom sont faits par la liaison (`hearth-link`), l'interface reçoit toujours
 * un état COMPLET. Le détail d'un service (HRT-63) porte les NOMS des variables et la commande, jamais une valeur :
 * les valeurs ne viennent que de `revealServiceEnv`, à la demande.
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
  /** Dernière lecture complète réussie de la liste ; `null` si l'agent ne la donne pas (jamais inventée). */
  listReadAt: number | null;
  /** La liste affichée n'est peut-être plus à jour (relecture en échec, conteneurs illisibles, suivi suspendu). */
  listDegraded: boolean;
  /** Heure de l'AGENT à la fabrication de la vue (HRT-72) ; `null` pour un agent plus ancien (date jamais inventée). */
  at: number | null;
  /** Dernier moment où la liste était à jour, heure de l'AGENT ; `null` : aucune liste lue (si `at` est là) ou agent ancien. */
  listCurrentAt: number | null;
}

/** Un dossier de données monté dans un service. `unknown` : un genre que ce client ne connaît pas. */
export interface ServiceMount {
  kind: "folder" | "volume" | "unknown";
  /** Dossier du serveur, ou nom du volume géré par le moteur. */
  source: string;
  target: string;
  readOnly: boolean;
}

/** Comment le service redémarre. `unknown` : une façon que ce client ne connaît pas (aucune phrase inventée). */
export type RestartMode =
  | "unless_stopped"
  | "always"
  | "on_failure"
  | "none"
  | "supervisor"
  | "unknown";

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
  /** Ports publiés coupés par l'agent au-delà de sa borne : comptés, jamais listés (BR-SVC-021). */
  portsOmitted: number;
  hostNetwork: boolean;
  /** Détail (HRT-63) : dossiers montés, coupés par l'agent au-delà de sa borne (comptés, jamais listés). */
  mounts: ServiceMount[];
  mountsOmitted: number;
  restart: RestartMode;
  /**
   * NOMS des variables, jamais une valeur (BR-SVC-008). Texte sensible : jamais dans une trace, un titre ou une
   * infobulle. Vide aussi quand `detailsReserved`.
   */
  envNames: string[];
  envNamesOmitted: number;
  /** La commande lancée ; `null` : « Commande de l'image ». Texte sensible. */
  command: string[] | null;
  /** Délai d'arrêt en secondes. */
  graceS: number | null;
  /** Qui a adopté ce service et quand (ms) ; `null` : service créé par Hearth. */
  adopted: { by: string; at: number | null } | null;
  /** Service adopté dont la commande et les noms de variables sont réservés au compte connecté (BR-ADOPT-015). */
  detailsReserved: boolean;
  /**
   * La commande et les noms de variables ont été LUS sur le serveur depuis le démarrage du client. Faux pour une
   * liste relue du disque : elle y est écrite sans eux (ils peuvent porter des secrets), donc `envNames` vide et
   * `command` à `null` y veulent dire « pas lus », JAMAIS « aucune variable » ni « commande de l'image ».
   */
  detailsKnown: boolean;
  /**
   * L'action qui tient le service (BR-SVC-031, HRT-84) : son genre (code de l'agent), qui l'a lancée et depuis quand
   * (ms). `null` : rien ne tient le service, ou ce compte ne peut pas lire ses opérations (jamais deviné).
   */
  busy: { kind: string; by: string; since: number | null } | null;
}

/** Une variable révélée par un compte qui a le droit (BR-SVC-009). Gardée en mémoire le temps de l'affichage, jamais écrite. */
export interface EnvVariable {
  name: string;
  value: string;
}

/**
 * Ce que reçoit un abonné : l'état complet d'un serveur, ou « cet agent ne gère pas les services » (pas
 * de capacité `services` dans `/hello` : ni sujet ni route envoyés, `metrics` continue). `seq` croît
 * strictement par serveur : le récepteur écarte tout événement dont `seq` n'est pas supérieur au dernier connu.
 */
export type ServicesEvent =
  | {
      kind: "state";
      serverId: string;
      seq: number;
      /**
       * Quand (ms, heure de CE poste) la LIAISON a reçu la vue du moteur de cet état : le repère de l'âge de la
       * liste. Vient de l'étage qui reçoit le message (un état rejoué ou relu du disque garde sa vraie date) ;
       * `null` : inconnue. L'interface ne la remplace jamais par l'heure à laquelle elle traite l'événement.
       */
      engineReceivedAt: number | null;
      engine: EngineInfo;
      services: ServiceInfo[];
      /** Combien de services l'agent a envoyés sans que cette version sache les lire (écartés, comptés). */
      unreadable: number;
    }
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
    listReadAt: parseDate(dto.listReadAt),
    listDegraded: dto.listDegraded,
    at: parseDate(dto.at),
    listCurrentAt: parseDate(dto.listCurrentAt),
  };
}

export function toService(dto: ServiceDto, detailsKnown: boolean): ServiceInfo {
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
    portsOmitted: dto.portsOmitted,
    hostNetwork: dto.hostNetwork,
    mounts: dto.mounts.map((mount) => ({ ...mount })),
    mountsOmitted: dto.mountsOmitted,
    restart: dto.restart,
    envNames: [...dto.envNames],
    envNamesOmitted: dto.envNamesOmitted,
    command: dto.command ? [...dto.command] : null,
    graceS: dto.graceS,
    adopted: dto.adopted ? { by: dto.adopted.by, at: parseDate(dto.adopted.at) } : null,
    detailsReserved: dto.detailsReserved,
    detailsKnown,
    busy: dto.busy
      ? { kind: dto.busy.kind, by: dto.busy.by, since: parseDate(dto.busy.since) }
      : null,
  };
}

export function toEnvVariables(dto: EnvRevealDto): EnvVariable[] {
  return dto.variables.map((variable) => ({ name: variable.name, value: variable.value }));
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
    engineReceivedAt: dto.engineReceivedAt,
    engine: toEngine(dto.view.engine),
    services: dto.view.services.map((service) => toService(service, dto.detailsKnown)),
    unreadable: dto.view.unreadable,
  };
}

/**
 * Le droit « gérer les services » du compte connecté (HRT-78, ADR-0041, BR-GRANT-011). `known` faux : l'agent
 * n'annonce pas `service-rights`, `granted` est alors toujours faux (BR-GRANT-016). `account` : l'identifiant du
 * compte connecté, texte de l'agent, à valider (`isHearthUsername`) avant tout affichage.
 */
export interface ServiceRights {
  known: boolean;
  granted: boolean;
  account: string;
}

export function toServiceRights(dto: ServiceRightsDto): ServiceRights {
  return { known: dto.known, granted: dto.granted, account: dto.account };
}
