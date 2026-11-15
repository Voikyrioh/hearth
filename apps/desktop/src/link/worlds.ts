import type {
  WorldInspectionDto,
  WorldInspectReadDto,
  WorldPickDto,
  WorldRefusalDto,
  WorldsReadDto,
  WorldsViewDto,
} from "@/bindings";

/**
 * Reprendre un monde existant (HRT-106, ADR-0059, `BR-MONDE-*`) : types de l'interface, côté pont. Deux LECTURES (la
 * liste des mondes que la fiche reconnaît dans les dossiers ouverts aux services, l'inspection d'un monde) : ni acte,
 * ni journal, rien n'est écrit. Le monde est désigné par un dossier ouvert et un chemin relatif, jamais un chemin
 * absolu ; un nom de monde ou un dossier est un texte du serveur, rendu inoffensif avant d'être affiché et jamais écrit
 * dans une trace. Installer avec un monde passe par `installFiche` (membre `world` de la demande).
 */

/** Un monde trouvé. `files` et `bytes` : connus d'un monde « fichier » sans rien ouvrir ; absents d'un monde « dossier ». */
export interface WorldEntry {
  path: string;
  name: string;
  files: number | null;
  bytes: number | null;
  modifiedAt: string | null;
  /** `false` : le nom de ce monde est refusé par le réglage de la fiche ; listé, mais il ne s'installe pas (BR-MONDE-014). */
  usable: boolean;
}

/** Un dossier ouvert aux services et ce que la recherche y a trouvé. */
export interface WorldRoot {
  folder: string;
  /** `false` : le dossier n'a pas pu être ouvert (absent, droits). */
  readable: boolean;
  /** La recherche s'est arrêtée à sa borne : des mondes peuvent manquer (BR-MONDE-003). */
  truncated: boolean;
  worlds: WorldEntry[];
}

export interface WorldsView {
  /** Vide : le serveur n'a ouvert aucun dossier aux services (BR-MONDE-002). */
  roots: WorldRoot[];
  maxDepth: number;
  maxEntries: number;
}

/** Ce que la lecture a donné : la liste, ou « cette fiche ne dit pas ce qu'est un monde » (BR-MONDE-039). */
export type WorldsRead = { kind: "ready"; view: WorldsView } | { kind: "not_supported" };

/** Le monde désigné. */
export interface WorldRef {
  folder: string;
  path: string;
}

/** Le monde choisi pour l'installation et les deux gestes qui l'accompagnent. */
export interface WorldPick extends WorldRef {
  /** « Je comprends » : Hearth ne sait pas avec quelle version du jeu le monde a été enregistré (BR-MONDE-016). */
  versionUnderstood: boolean;
  /** « Garder cette copie » d'avance si des fichiers changent pendant la copie (BR-MONDE-009). */
  keepIfChanged: boolean;
}

/** Ce que la copie écrirait et ce qu'elle laisserait où elle est (BR-MONDE-006, 009, 012). */
export interface WorldInspection {
  files: number;
  bytes: number;
  notCopied: number;
  notCopiedNames: string[];
  skippedLinks: number;
  skippedLinkNames: string[];
  /** La mesure s'est arrêtée à son budget : les chiffres sont « au moins ». */
  partial: boolean;
  freeBytes: number | null;
}

export type WorldRefusal = WorldRefusalDto;

/** Ce que l'inspection a donné : les chiffres, ou le refus du monde (pas une panne). */
export type WorldInspectRead =
  | { kind: "ready"; inspection: WorldInspection }
  | { kind: "refused"; reason: WorldRefusal };

// ── Conversions depuis les types générés (`bindings.ts`) ──────────────────────────────────────────

function toView(dto: WorldsViewDto): WorldsView {
  return {
    roots: dto.roots.map((root) => ({
      folder: root.folder,
      readable: root.readable,
      truncated: root.truncated,
      worlds: root.worlds.map((world) => ({
        path: world.path,
        name: world.name,
        files: world.files,
        bytes: world.bytes,
        modifiedAt: world.modifiedAt,
        usable: world.usable,
      })),
    })),
    maxDepth: dto.maxDepth,
    maxEntries: dto.maxEntries,
  };
}

export function toWorldsRead(dto: WorldsReadDto): WorldsRead {
  return dto.kind === "ready"
    ? { kind: "ready", view: toView(dto.view) }
    : { kind: "not_supported" };
}

function toInspection(dto: WorldInspectionDto): WorldInspection {
  return {
    // Un nombre illisible sur le fil (`null`) ne devient jamais un faux chiffre : zéro, et `partial` le dit déjà.
    files: dto.files ?? 0,
    bytes: dto.bytes ?? 0,
    notCopied: dto.notCopied ?? 0,
    notCopiedNames: [...dto.notCopiedNames],
    skippedLinks: dto.skippedLinks ?? 0,
    skippedLinkNames: [...dto.skippedLinkNames],
    partial: dto.partial,
    freeBytes: dto.freeBytes,
  };
}

export function toWorldInspectRead(dto: WorldInspectReadDto): WorldInspectRead {
  return dto.kind === "ready"
    ? { kind: "ready", inspection: toInspection(dto.inspection) }
    : { kind: "refused", reason: dto.reason };
}

/** Le monde choisi tel que la coquille le reçoit. */
export function toWorldPickDto(pick: WorldPick): WorldPickDto {
  return {
    folder: pick.folder,
    path: pick.path,
    versionUnderstood: pick.versionUnderstood,
    keepIfChanged: pick.keepIfChanged,
  };
}
