import type {
  CatalogDto,
  FicheDetailDto,
  FicheInstallRequestDto,
  FicheLinkKind,
  FicheQuestionDto,
  FicheReadDto,
} from "@/bindings";

export type { FicheLinkKind };

/**
 * Le Catalogue et l'installation depuis une fiche (HRT-102, ADR-0050, `BR-FICHE-*`) : types de l'interface, côté
 * pont. Aucune règle ici et aucune valeur inventée : tout ce que l'écran montre d'une fiche (libellés, bornes, choix,
 * ressources, preuves) est lu dans la fiche que l'agent propose (écart 14.26).
 *
 * - `readCatalog` et `readFiche` sont des LECTURES, ouvertes à tout compte, jamais journalisées ;
 * - `installFiche` est le déploiement ordinaire avec un membre `source` : un acte d'administration confirmé, JAMAIS
 *   couvert par l'élévation, jamais rejoué. La valeur d'une réponse (mot de passe du serveur, jeton) ne traverse que
 *   cet appel : elle n'est ni gardée ni tracée par le pont.
 */

export type FicheStatus = "published" | "preview" | "unknown";

/** Une fiche dans la liste. */
export interface FicheCard {
  id: string;
  name: string;
  summary: string;
  version: number;
  status: FicheStatus;
  /** `false` : la fiche est listée mais ne s'installe pas sur ce serveur. */
  installable: boolean;
}

/** La liste des fiches que CET agent propose. */
export interface Catalog {
  fiches: FicheCard[];
  /** Fiches que l'agent a écartées au chargement : leur dossier et l'invariant violé, jamais un contenu. */
  refused: { dir: string; invariant: string }[];
  /** Fiches d'une version plus récente que ce client, illisibles ici. */
  unreadable: number;
}

/** D'où vient un chiffre de ressource (BR-FICHE-008, BR-FICHE-057). */
export type Evidence =
  | { kind: "measured" }
  | { kind: "measured_idle" }
  | { kind: "publisher"; url: string }
  | { kind: "unmeasured" }
  | { kind: "unknown" };

/** Le jeu de caractères d'un texte ; `unknown` : un jeu plus récent que ce client, l'agent juge. */
export type Charset = "line" | "file_stem" | "arg_safe" | "unknown";

/** Le genre d'un secret. */
export type SecretKind = "game_password" | "admin_password" | "server_token" | "unknown";

/** Comment le port du serveur se relie à celui du service : `same_number`, le jeu annonce son port, les deux valent le même nombre. */
export type PortMapping = "nat" | "same_number" | "unknown";

export type QuestionKind =
  | "text"
  | "secret"
  | "int"
  | "bool"
  | "choice"
  | "id_list"
  | "opaque"
  | "unknown";

/** Une question d'installation : tout ce qu'il faut pour la poser, lu dans la fiche. */
export interface FicheQuestion {
  id: string;
  label: string;
  help: string;
  secret: boolean;
  /** La valeur proposée ; jamais pour un secret. */
  proposed: string | null;
  kind: QuestionKind;
  choices: { value: string; label: string }[];
  min: number | null;
  max: number | null;
  minLen: number | null;
  maxLen: number | null;
  charset: Charset | null;
  /** « Ne pourra plus être changé après la création du monde » (BR-FICHE-016). */
  frozenAfterCreation: boolean;
  /** Les variantes pour lesquelles la question existe ; vide : toutes. */
  variants: string[];
  /** Le genre d'un secret ; `null` pour une question qui n'en est pas un. */
  secretKind: SecretKind | null;
  /** Où créer un jeton de serveur. */
  obtainUrl: string | null;
}

export interface FichePort {
  id: string;
  container: number;
  protocol: "tcp" | "udp";
  /** `game`, `query`, `voice`, `web_admin`, `remote_console` ou `other`. */
  role: string;
  /** `always`, `optional` ou `never`. */
  publish: string;
  defaultHost: number | null;
  mapping: PortMapping;
  /** Les variantes pour lesquelles le port existe ; vide : toutes. */
  onlyVariants: string[];
}

/** Ce que le programme du jeu télécharge lui-même au premier démarrage, avec la preuve de la taille (BR-FICHE-010). */
export interface FicheDownload {
  what: string;
  source: string;
  /** `null` : la fiche ne connaît pas la taille. */
  bytes: number | null;
  evidence: Evidence;
}

/** Le premier démarrage d'une fiche (BR-FICHE-056). */
export interface FicheFirstStart {
  /** Au-delà, Hearth ne peut plus dire si le jeu est prêt. */
  readyTimeoutSeconds: number;
  worldGeneration: boolean;
  downloads: FicheDownload[];
  /** Les libellés des étapes que Hearth reconnaît dans les journaux du jeu. */
  stages: string[];
}

export type WorldSaved = "by_image_on_signal" | "by_sequence" | "not_saved" | "unknown";

/** La façon d'arrêter le jeu (BR-JEU-002). */
export interface FicheStop {
  graceSeconds: number;
  worldSaved: WorldSaved;
  /** La suite d'étapes a été essayée sur la vraie image ; sinon Hearth ne l'envoie pas. */
  sequenceTested: boolean;
}

/** Un réglage posé par la fiche sans que l'utilisateur l'ait choisi (BR-JEU-039). */
export interface ImposedSetting {
  label: string;
  /** `null` : tirée, dérivée ou secrète, jamais montrée. */
  value: string | null;
  why: string;
}

/** Le détail d'une fiche, avec l'empreinte que l'installation doit reprendre. */
export interface FicheDetail {
  card: FicheCard;
  digest: string;
  image: { repository: string; tag: string; digest: string | null };
  ownerUid: number;
  ownerGid: number;
  variants: { id: string; label: string; help: string; default: boolean }[];
  ports: FichePort[];
  folders: { id: string; role: string; target: string; backup: boolean; onlyVariants: string[] }[];
  agreements: { id: string; title: string; url: string; sentence: string }[];
  questions: FicheQuestion[];
  resources: {
    cpu: { minMillis: number | null; recommendedMillis: number | null; evidence: Evidence };
    memory: { minBytes: number | null; recommendedBytes: number | null; evidence: Evidence };
    disk: { installBytes: number | null; evidence: Evidence };
  };
  /** `null` : un agent plus ancien ne le dit pas. */
  firstStart: FicheFirstStart | null;
  stop: FicheStop | null;
  imposed: ImposedSetting[];
  /** Des éléments d'une version plus récente que ce client, illisibles : la fiche ne s'installe pas à l'aveugle. */
  unreadable: number;
}

/** La lecture d'une fiche : trouvée, ou inconnue de cet agent (une fiche d'essai sur un agent publié se lit ainsi). */
export type FicheRead = { kind: "found"; fiche: FicheDetail } | { kind: "unknown" };

/** La demande d'installation (membre `source` du déploiement, BR-FICHE-013). */
export interface FicheInstallRequest {
  name: string;
  fiche: string;
  /** La version et l'empreinte de la fiche que l'utilisateur a LUES : sinon `FICHE_CHANGED`. */
  version: number;
  digest: string;
  variant: string | null;
  /** Les réponses non vides ; une valeur peut être un secret. */
  answers: { id: string; value: string }[];
  /** Les contrats que l'utilisateur a acceptés par un geste (BR-FICHE-058). */
  agreements: string[];
  ports: { id: string; hostPort: number }[];
  optionalPorts: string[];
  /** Mémoire en octets et processeur en millicœurs : obligatoires (BR-RES-013). */
  resources: { memoryBytes: number; cpuMillis: number };
  /** Les cases de l'agent : le port réservé, le manque de place annoncé par l'éditeur. */
  accept: { reservedPorts: boolean; lowDisk: boolean };
}

// ── Conversions depuis les types générés (`bindings.ts`) ──────────────────────────────────────────

function toQuestion(dto: FicheQuestionDto): FicheQuestion {
  return {
    id: dto.id,
    label: dto.label,
    help: dto.help,
    secret: dto.secret,
    proposed: dto.secret ? null : dto.proposed,
    kind: dto.kind,
    choices: dto.choices.map((choice) => ({ value: choice.value, label: choice.label })),
    min: dto.min,
    max: dto.max,
    minLen: dto.minLen,
    maxLen: dto.maxLen,
    charset: dto.charset,
    frozenAfterCreation: dto.frozenAfterCreation,
    variants: dto.variants,
    secretKind: dto.secretKind,
    obtainUrl: dto.obtainUrl,
  };
}

export function toCatalog(dto: CatalogDto): Catalog {
  return {
    fiches: dto.fiches.map((card) => ({ ...card })),
    refused: dto.refused.map((refused) => ({ dir: refused.dir, invariant: refused.invariant })),
    unreadable: dto.unreadable,
  };
}

function toFicheDetail(dto: FicheDetailDto): FicheDetail {
  return {
    card: { ...dto.card },
    digest: dto.digest,
    image: { ...dto.image },
    ownerUid: dto.ownerUid,
    ownerGid: dto.ownerGid,
    variants: dto.variants.map((variant) => ({ ...variant })),
    ports: dto.ports.map((port) => ({
      id: port.id,
      container: port.container,
      protocol: port.protocol === "udp" ? "udp" : "tcp",
      role: port.role,
      publish: port.publish,
      defaultHost: port.defaultHost,
      mapping: port.mapping,
      onlyVariants: port.onlyVariants,
    })),
    folders: dto.folders.map((folder) => ({ ...folder })),
    agreements: dto.agreements.map((agreement) => ({ ...agreement })),
    questions: dto.questions.map(toQuestion),
    resources: {
      cpu: { ...dto.resources.cpu },
      memory: { ...dto.resources.memory },
      disk: { ...dto.resources.disk },
    },
    firstStart: dto.firstStart
      ? {
          readyTimeoutSeconds: dto.firstStart.readyTimeoutSeconds,
          worldGeneration: dto.firstStart.worldGeneration,
          downloads: dto.firstStart.downloads.map((download) => ({ ...download })),
          stages: [...dto.firstStart.stages],
        }
      : null,
    stop: dto.stop ? { ...dto.stop } : null,
    imposed: dto.imposed.map((setting) => ({ ...setting })),
    unreadable: dto.unreadable,
  };
}

export function toFicheRead(dto: FicheReadDto): FicheRead {
  return dto.kind === "found"
    ? { kind: "found", fiche: toFicheDetail(dto.fiche) }
    : { kind: "unknown" };
}

/** La demande telle que la coquille la reçoit. Les réponses (secrets possibles) ne traversent que ici, jamais gardées. */
export function toFicheInstallRequestDto(request: FicheInstallRequest): FicheInstallRequestDto {
  return {
    name: request.name,
    fiche: request.fiche,
    version: request.version,
    digest: request.digest,
    variant: request.variant,
    answers: request.answers.map((answer) => ({ id: answer.id, value: answer.value })),
    agreements: [...request.agreements],
    ports: request.ports.map((port) => ({ id: port.id, hostPort: port.hostPort })),
    optionalPorts: [...request.optionalPorts],
    resources: {
      memoryBytes: request.resources.memoryBytes,
      cpuMillis: request.resources.cpuMillis,
    },
    accept: { reservedPorts: request.accept.reservedPorts, lowDisk: request.accept.lowDisk },
  };
}
