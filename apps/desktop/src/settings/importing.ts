import type { ImportAnalysis, ImportLine, SaveSettingsRequest, SettingEffect } from "@/link";
import { heaviestEffect } from "./edits";
import type { SettingModel } from "./model";

/**
 * L'import d'un texte collé (HRT-109, BR-FICHE-052), des fonctions pures sans E/S. C'est l'AGENT qui lit le texte (il
 * connaît le format du jeu) et dit, ligne par ligne, ce qu'il en ferait ; le client range cette réponse et construit
 * l'enregistrement, qui est le MÊME acte que celui du formulaire (origine `import`). Le texte (un mot de passe possible)
 * n'est jamais gardé : il vit dans la fenêtre d'import et s'efface à sa fermeture.
 */

/** Une ligne du tableau d'import. */
export interface ImportRow {
  line: ImportLine;
  /** Le réglage de la fiche ; `null` : l'agent a rendu un identifiant que le catalogue ne connaît pas (ignoré). */
  model: SettingModel | null;
  /** Les valeurs de la liste du réglage quand la valeur collée n'en fait pas partie (choix, interrupteur). */
  alternatives: { value: string; label: string }[];
}

export function importRows(analysis: ImportAnalysis, models: readonly SettingModel[]): ImportRow[] {
  const byId = new Map(models.map((model) => [model.id, model]));
  return analysis.lines.map((line) => {
    const model = byId.get(line.id) ?? null;
    const kind = model?.setting.question.kind;
    const pick = line.state === "rejected" && (kind === "choice" || kind === "bool");
    return {
      line,
      model,
      alternatives:
        pick && model ? model.setting.question.choices.map((choice) => ({ ...choice })) : [],
    };
  });
}

/** Les lignes que l'import appliquerait : reconnues, ou refusées dont la personne a choisi une valeur de la liste. */
export function importChanges(
  rows: readonly ImportRow[],
  picks: Readonly<Record<string, string>>,
): { id: string; kind: "paste" | "pick" }[] {
  const out: { id: string; kind: "paste" | "pick" }[] = [];
  for (const row of rows) {
    if (row.model === null || row.model.lock !== null) continue;
    if (row.line.state === "take") out.push({ id: row.line.id, kind: "paste" });
    else if (
      row.line.state === "rejected" &&
      picks[row.line.id] !== undefined &&
      picks[row.line.id] !== ""
    ) {
      out.push({ id: row.line.id, kind: "pick" });
    }
  }
  return out;
}

/**
 * L'enregistrement de l'import : les lignes reconnues sont REPRISES du texte par l'agent (`from_import`, le seul moyen
 * de poser un mot de passe collé sans qu'il revienne au client), les valeurs choisies dans la liste sont posées. Les
 * réglages imposés, figés et inconnus ne partent jamais. `null` : rien à appliquer.
 */
export function toImportRequest(input: {
  rows: readonly ImportRow[];
  picks: Readonly<Record<string, string>>;
  text: string;
  baseRev: number;
  fileStamp: string;
}): { request: SaveSettingsRequest; effect: SettingEffect; count: number } | null {
  const changes = importChanges(input.rows, input.picks);
  if (changes.length === 0) return null;
  const models = changes
    .map((change) => input.rows.find((row) => row.line.id === change.id)?.model)
    .filter((model): model is SettingModel => model !== null && model !== undefined);
  const effect = heaviestEffect(models);
  return {
    count: changes.length,
    effect,
    request: {
      baseRev: input.baseRev,
      fileStamp: input.fileStamp,
      origin: "import",
      effect,
      changes: changes.map((change) =>
        change.kind === "paste"
          ? { id: change.id, action: "from_import", value: null }
          : { id: change.id, action: "set", value: input.picks[change.id] ?? "" },
      ),
      others: [],
      driftChoices: [],
      importText: input.text,
    },
  };
}

/** Les réglages imposés que le texte retire ou change, par leur libellé (jamais un nom venu d'ailleurs). */
export function undoneLabels(analysis: ImportAnalysis, models: readonly SettingModel[]): string[] {
  const byId = new Map(models.map((model) => [model.id, model]));
  return analysis.imposedUndone.flatMap((id) => {
    const model = byId.get(id);
    return model ? [model.setting.question.label] : [];
  });
}
