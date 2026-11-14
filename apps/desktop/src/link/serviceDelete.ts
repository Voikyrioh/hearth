import type {
  DeleteOutcome as BoundDeleteOutcome,
  DeletePreviewOutcome as BoundPreviewOutcome,
  DeletePreviewDto,
} from "@/bindings";
import type { AdminActKind } from "./reauth";
import type { OperationKind } from "./serviceOps";
import type { EngineState, ServiceState } from "./services";

/**
 * Supprimer un service (HRT-86, `BR-DEL-*`) : types de l'interface et conversions depuis les types générés
 * (`bindings.ts`). Aucune règle ici : la liaison construit l'acte, l'agent arbitre. Une réponse `202` ne dit JAMAIS
 * l'issue : elle arrive par le suivi des opérations (HRT-84). Les chemins des dossiers sont ceux du serveur : l'agent
 * ne les envoie qu'aux comptes qui ont le droit de gérer les services, l'interface ne les écrit nulle part.
 */

/** L'acte d'administration de la suppression : jamais couvert par l'élévation (BR-DEL-012). */
export const DELETE_ACT_KIND: AdminActKind = "service_delete";

/** Ce que la suppression fera d'un dossier géré par Hearth (BR-DEL-006 à 008). */
export type FolderFate =
  | "erasable"
  | "kept_shared"
  | "kept_unverifiable"
  | "kept_outside_root"
  | "unknown";

/** Un dossier géré par Hearth, tel que la fenêtre le montre. */
export interface FolderPreview {
  path: string;
  bytes: number;
  items: number;
  /** La mesure a été coupée : « plus de {taille} » (BR-DEL-009). */
  partial: boolean;
  /** Le dossier n'a pas pu être mesuré du tout. */
  unmeasured: boolean;
  fate: FolderFate;
  /** Les services qui utilisent aussi ce dossier, par nom (BR-DEL-008). */
  sharedWith: string[];
  /** Un conteneur créé hors de Hearth l'utilise aussi : dit sans nom (BR-ADOPT-002). */
  sharedWithUnmanaged: boolean;
}

/** De quoi bâtir la fenêtre « Supprimer {nom} ? ». */
export interface DeletePreview {
  name: string;
  /** `dépôt:version` : l'image reste sur le serveur (BR-DEL-004). */
  image: string;
  /** Le service tourne : il sera arrêté d'abord (BR-DEL-013). */
  running: boolean;
  /** Le délai d'arrêt propre du service, en secondes. */
  stopGraceS: number;
  /** Le conteneur n'existe plus mais Hearth connaît le service (BR-DEL-018). */
  containerMissing: boolean;
  folders: FolderPreview[];
  /** Les dossiers du serveur montés dans le service : jamais effacés (BR-DEL-006). */
  serverFolders: string[];
  /** Les montages qui ne sont pas des dossiers (volumes du moteur) : conservés. */
  volumes: number;
  /** La mesure de tous les dossiers a tenu dans les 10 secondes. */
  measuredInTime: boolean;
  /** Les sauvegardes de ce service que Hearth connaît (toujours 0 tant que HRT-97 n'est pas livré). */
  backups: number;
}

/** Pourquoi l'agent n'a pas donné la prévisualisation. */
export type DeletePreviewRefusal =
  | { kind: "service_not_found" }
  | { kind: "service_adopted" }
  | { kind: "engine_unavailable"; engine: EngineState | null }
  | { kind: "other" };

export type DeletePreviewOutcome =
  | { kind: "ready"; preview: DeletePreview }
  | { kind: "refused"; refusal: DeletePreviewRefusal };

/** La demande de suppression : les deux cases (décochées par défaut) et le nom retapé (BR-DEL-005, 011). */
export interface DeleteRequest {
  eraseData: boolean;
  eraseBackups: boolean;
  confirmName: string | null;
}

/** Pourquoi l'agent a refusé la suppression (liste fermée de codes ; l'interface les met en phrases). */
export type DeleteRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "engine_unavailable"; engine: EngineState | null }
  | { kind: "service_not_found" }
  | { kind: "service_adopted" }
  | { kind: "service_busy"; by: string | null; action: OperationKind | null }
  | { kind: "not_drivable"; state: ServiceState | null }
  | { kind: "name_mismatch" }
  | { kind: "backups_unavailable" }
  | { kind: "other" };

/**
 * L'issue de l'envoi : accepté (l'opération court à l'arrière-plan), refusé, ou résultat inconnu (le lien est
 * tombé avant la réponse : jamais rejoué, BR-DEL-023).
 */
export type DeleteOutcome =
  | { kind: "accepted"; operationId: string }
  | { kind: "refused"; refusal: DeleteRefusal }
  | { kind: "unknown"; opId: string };

function toPreview(dto: DeletePreviewDto): DeletePreview {
  return {
    name: dto.name,
    image: dto.image,
    running: dto.running,
    stopGraceS: dto.stopGraceS,
    containerMissing: dto.containerMissing,
    folders: dto.folders.map((folder) => ({
      path: folder.path,
      // Un nombre que le fil ne sait pas porter (`null`) n'est jamais lu comme zéro mesuré : « non mesuré ».
      bytes: folder.bytes ?? 0,
      items: folder.items ?? 0,
      partial: folder.partial,
      unmeasured: folder.unmeasured || folder.bytes === null || folder.items === null,
      fate: folder.fate,
      sharedWith: folder.sharedWith,
      sharedWithUnmanaged: folder.sharedWithUnmanaged,
    })),
    serverFolders: dto.serverFolders,
    volumes: dto.volumes,
    measuredInTime: dto.measuredInTime,
    backups: dto.backups,
  };
}

export function toDeletePreviewOutcome(dto: BoundPreviewOutcome): DeletePreviewOutcome {
  if (dto.kind === "ready") return { kind: "ready", preview: toPreview(dto.preview) };
  return { kind: "refused", refusal: dto.refusal as DeletePreviewRefusal };
}

export function toDeleteOutcome(dto: BoundDeleteOutcome): DeleteOutcome {
  switch (dto.kind) {
    case "accepted":
      return { kind: "accepted", operationId: dto.operationId };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal as DeleteRefusal };
  }
}
