import type {
  FicheSetting,
  GameSettingsView,
  SettingEffect,
  SettingValue,
  ValueState,
} from "@/link";

/**
 * La ligne d'un réglage (HRT-109, BR-FICHE-037, BR-FICHE-041), des fonctions pures sans E/S : le réglage du catalogue de
 * la fiche (libellé, bornes, choix, valeur proposée : tout ce que la fiche dit) et sa valeur voulue (ce que l'agent dit).
 * AUCUNE règle de jeu ici : un réglage ne se range, ne se filtre et ne se règle que d'après ce que la fiche et l'agent
 * donnent (écart 14.26).
 */

/** Pourquoi un réglage ne se règle pas ici ; `null` : il se règle. */
export type SettingLock =
  /** Figé après la création du monde (BR-FICHE-042). */
  | "frozen"
  /** Imposé par la fiche (BR-JEU-039). */
  | "imposed"
  /** N'existe plus dans la version du jeu (BR-FICHE-049). */
  | "gone"
  /** Reçu par variable ou argument : il se change en recréant le service, pas encore livré (HRT-87). */
  | "recreate"
  | null;

export interface SettingModel {
  setting: FicheSetting;
  id: string;
  /** Les trois états de BR-FICHE-037 : une valeur, non écrit, ou un secret défini. */
  state: ValueState;
  /** La valeur voulue ; `null` pour « non écrit » et pour un secret (jamais rendu). */
  value: string | null;
  effect: SettingEffect;
  lock: SettingLock;
}

/** Un secret est un réglage dont le contrôle est masqué : sa valeur n'est jamais lue, seulement posée. */
export function isSecret(setting: FicheSetting): boolean {
  return setting.question.secret || setting.question.kind === "secret";
}

function lockOf(setting: FicheSetting, value: SettingValue): SettingLock {
  if (value.imposed || setting.imposedWhy !== null) return "imposed";
  if (value.gone) return "gone";
  if (value.frozen) return "frozen";
  if (value.effect === "recreate") return "recreate";
  return null;
}

/**
 * Les réglages d'un service installé : un par réglage du catalogue, dans l'ordre de la fiche. Un réglage que l'agent ne
 * rend pas (agent plus ancien que la fiche, ou fiche mise à jour) est « non écrit » : on ne l'invente pas. Une valeur
 * d'un réglage que le catalogue ne connaît pas n'est pas montrée ici (c'est une ligne « Autres réglages »).
 */
export function modelsOf(
  catalogue: readonly FicheSetting[],
  view: GameSettingsView,
): SettingModel[] {
  const byId = new Map(view.settings.map((value) => [value.id, value]));
  return catalogue.map((setting) => {
    const found = byId.get(setting.question.id);
    if (!found) {
      return {
        setting,
        id: setting.question.id,
        state: "unwritten",
        value: null,
        effect: "unknown",
        lock: setting.imposedWhy !== null ? "imposed" : null,
      };
    }
    return {
      setting,
      id: setting.question.id,
      state: found.state,
      value: isSecret(setting) ? null : found.value,
      effect: found.effect,
      lock: lockOf(setting, found),
    };
  });
}

/**
 * Les réglages d'une installation (mode « Voir tous les réglages », BR-FICHE-040) : tous sont sur leur valeur proposée,
 * aucun fichier n'existe encore. Un réglage imposé est montré, jamais modifiable ; un réglage figé à la création du
 * monde se règle encore (le monde n'existe pas) avec sa mention. Les réglages d'une autre variante n'existent pas.
 */
export function installModelsOf(
  catalogue: readonly FicheSetting[],
  variant: string | null,
): SettingModel[] {
  return catalogue
    .filter((setting) => appliesTo(setting, variant))
    .map((setting) => ({
      setting,
      id: setting.question.id,
      state: setting.question.proposed !== null ? "written" : "unwritten",
      value: setting.question.proposed,
      effect: "none",
      lock: setting.imposedWhy !== null ? "imposed" : null,
    }));
}

/** Le réglage existe pour la variante choisie (vide : toutes, BR-FICHE-060). */
function appliesTo(setting: FicheSetting, variant: string | null): boolean {
  const variants = setting.question.variants;
  return variants.length === 0 || (variant !== null && variants.includes(variant));
}

/** La valeur proposée par la fiche ; `null` : la fiche n'en propose pas (la remise retire la ligne, BR-FICHE-044). */
export function proposedOf(model: SettingModel): string | null {
  return isSecret(model.setting) ? null : model.setting.question.proposed;
}

/** La fiche ne dit pas la valeur par défaut du jeu : « non vérifiée » (BR-FICHE-037, BR-FICHE-057). */
export function defaultUnverified(model: SettingModel): boolean {
  return model.setting.gameDefault === null && !isSecret(model.setting);
}
