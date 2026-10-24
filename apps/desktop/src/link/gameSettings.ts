import type {
  AnalyzeOutcomeDto,
  SaveOutcome as BoundSaveOutcome,
  SaveSettingsRequestDto,
  SettingsReadDto,
} from "@/bindings";

/**
 * Les réglages d'un serveur de jeu (HRT-109, ADR-0051, `BR-FICHE-037`, `040` à `049`, `051` à `054`) : types de l'interface, côté pont. Aucune
 * règle ici et aucune valeur inventée : les libellés, bornes et choix viennent de la fiche (`FicheSetting`), les valeurs
 * voulues et l'état du fichier de l'agent (`GameSettingsView`).
 *
 * - `readGameSettings` et `analyzeGameSettings` sont des LECTURES (l'analyse lit un texte collé et n'écrit rien) ;
 * - `saveGameSettings` est un acte d'administration confirmé, couvert par l'élévation de cinq minutes, jamais rejoué.
 *   L'agent ne redémarre rien : redémarrer est un autre geste (`pilotService`).
 *
 * Un secret n'est JAMAIS rendu par l'agent (ni sa valeur ni sa longueur) : seulement « défini » ou non. Une valeur secrète
 * saisie ou collée ne traverse que ces appels, n'est ni gardée ni tracée.
 */

export type { SaveSettingsRequestDto as SaveSettingsRequest };

/** Ce qu'un changement demande au service (déduit du canal par l'agent) ; `unknown` : un effet plus récent que ce client. */
export type SettingEffect = "none" | "restart" | "recreate" | "unknown";

/** Les trois états d'un réglage (BR-FICHE-037) ; `unknown` : un état plus récent que ce client. */
export type ValueState = "written" | "unwritten" | "secret_set" | "unknown";

export type FileState = "ok" | "missing" | "unreadable" | "unknown";

/** La valeur voulue d'un réglage de la fiche, telle que l'agent la rend. */
export interface SettingValue {
  id: string;
  state: ValueState;
  /** Absente pour un secret et pour « non écrit ». */
  value: string | null;
  frozen: boolean;
  imposed: boolean;
  /** N'existe plus dans la version du jeu : gardé et grisé (BR-FICHE-049). Faux tant que la version n'est pas lue. */
  gone: boolean;
  effect: SettingEffect;
}

/** Un réglage dont le fichier ne dit plus ce que Hearth veut (BR-FICHE-048). Jamais un secret. */
export interface Drift {
  id: string;
  hearth: string | null;
  file: string | null;
  /** Faux : la valeur du fichier n'est pas acceptée par le réglage, « Garder la valeur du fichier » sera refusé. */
  fileValid: boolean;
}

/** Un réglage que la fiche ne connaît pas (BR-FICHE-047). */
export interface OtherSetting {
  key: string;
  /** Absente quand le nom ressemble à un secret. */
  value: string | null;
  masked: boolean;
}

export interface GameSettingsView {
  /** La révision à renvoyer avec un enregistrement. */
  rev: number;
  fiche: { id: string; version: number; digest: string };
  fileState: FileState;
  /** L'empreinte du fichier tel que lu : à renvoyer avec l'enregistrement, obligatoire. */
  fileStamp: string | null;
  fileChanged: boolean;
  /** Le fichier brut peut être édité (faux : le jeu le réécrit à chaque démarrage). */
  rawEdit: boolean;
  settings: SettingValue[];
  drifts: Drift[];
  others: OtherSetting[];
  /** Les réglages imposés que le fichier a défaits : signalés, jamais réécrits. */
  imposedBroken: string[];
}

/** La lecture : trouvés, ou « ce service n'a pas de réglages de jeu » (jamais une panne). */
export type GameSettingsRead = { kind: "found"; view: GameSettingsView } | { kind: "absent" };

export type ImportState = "take" | "same" | "rejected" | "imposed" | "frozen" | "unknown";

export interface ImportLine {
  id: string;
  state: ImportState;
  /** Le code court d'un refus (`bounds`, `choice`, `charset`, `length`, `version`...). */
  reason: string | null;
  /** La valeur collée ; absente pour un secret. */
  pasted: string | null;
  secret: boolean;
}

export interface ImportAnalysis {
  lines: ImportLine[];
  /** Lignes que la fiche ne connaît pas : gardées dans le fichier. */
  unknown: number;
  duplicates: number;
  /** Les réglages imposés que le texte retire ou change. */
  imposedUndone: string[];
}

/** La lecture d'un texte collé : lue, ou illisible par le format du jeu (rien n'est écrit dans les deux cas). */
export type AnalyzeOutcome =
  | { kind: "read"; analysis: ImportAnalysis }
  | { kind: "unreadable"; reason: string | null; line: number | null };

/** Pourquoi l'agent a refusé l'enregistrement : un CODE, jamais un texte de l'agent. */
export type SaveRefusal =
  | { kind: "wrong_password" }
  | { kind: "password_required" }
  | { kind: "too_many_attempts"; retry_after_s: number }
  | { kind: "busy" }
  | { kind: "conflict"; by: string | null; effectMismatch: boolean }
  | { kind: "file_changed"; ids: string[] }
  | { kind: "file_unreadable"; reason: string | null; line: number | null }
  | { kind: "setting_refused"; id: string | null; reason: string; character: string | null }
  | { kind: "recreate_not_available"; ids: string[] }
  | { kind: "operation_in_progress" }
  | { kind: "service_not_found" }
  | { kind: "invalid" }
  | { kind: "other" };

export type SaveOutcome =
  | {
      kind: "saved";
      rev: number;
      /** Ce qu'il faut faire pour appliquer : la sorte la plus lourde des changements. */
      effect: SettingEffect;
      fileWritten: boolean;
      changed: number;
      /** Les réglages dont on a repris la valeur du fichier. */
      adopted: string[];
    }
  | { kind: "refused"; refusal: SaveRefusal }
  /** Le lien est tombé avant la réponse : jamais rejoué, l'interface relit les réglages. */
  | { kind: "unknown"; opId: string };

// ── Conversions depuis les types générés (`bindings.ts`) ──────────────────────────────────────────

export function toGameSettingsRead(dto: SettingsReadDto): GameSettingsRead {
  if (dto.kind === "absent") return { kind: "absent" };
  const view = dto.view;
  return {
    kind: "found",
    view: {
      rev: view.rev,
      fiche: { ...view.fiche },
      fileState: view.fileState,
      fileStamp: view.fileStamp,
      fileChanged: view.fileChanged,
      rawEdit: view.rawEdit,
      // Un secret n'a jamais de valeur côté client, même si un agent en envoyait une.
      settings: view.settings.map((setting) => ({
        ...setting,
        value: setting.state === "secret_set" ? null : setting.value,
      })),
      drifts: view.drifts.map((drift) => ({ ...drift })),
      others: view.others.map((other) => ({
        key: other.key,
        value: other.masked ? null : other.value,
        masked: other.masked,
      })),
      imposedBroken: [...view.imposedBroken],
    },
  };
}

export function toAnalyzeOutcome(dto: AnalyzeOutcomeDto): AnalyzeOutcome {
  if (dto.kind === "unreadable") {
    return { kind: "unreadable", reason: dto.reason, line: dto.line };
  }
  const view = dto.view;
  return {
    kind: "read",
    analysis: {
      lines: view.lines.map((line) => ({
        ...line,
        pasted: line.secret ? null : line.pasted,
      })),
      unknown: view.unknown,
      duplicates: view.duplicates,
      imposedUndone: [...view.imposedUndone],
    },
  };
}

export function toSaveOutcome(dto: BoundSaveOutcome): SaveOutcome {
  switch (dto.kind) {
    case "saved":
      return {
        kind: "saved",
        rev: dto.rev,
        effect: dto.effect,
        fileWritten: dto.fileWritten,
        changed: dto.changed,
        adopted: [...dto.adopted],
      };
    case "unknown":
      return { kind: "unknown", opId: dto.opId };
    case "refused":
      return { kind: "refused", refusal: dto.refusal };
  }
}
