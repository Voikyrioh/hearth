import type { DeployProtocol } from "@/link";
import type { Resources } from "@/resources";
import { imageIssue, looksSecret, splitImage } from "./rules";

/**
 * La saisie du formulaire de déploiement (HRT-69, BR-DEPLOY-029) et ce qu'on en tire, des fonctions pures sans E/S.
 * Le brouillon vit en mémoire SEULEMENT (`stores/deployForm.ts`) : jamais dans l'adresse, sur le disque, dans un
 * instantané ou une trace. Les valeurs de variables et la commande sont des secrets.
 */

export interface EnvRow {
  /** Identité stable de la rangée (clé de rendu, focus) : jamais un indice. */
  id: number;
  name: string;
  value: string;
  /** `null` : le nom décide (masquée si le nom évoque un secret) ; `true`/`false` : le choix de l'utilisateur (BR-DEPLOY-006). */
  reveal: boolean | null;
}

export interface PortRow {
  id: number;
  /** Texte saisi : un nombre n'est jugé qu'à la lecture (`parsePort`). */
  host: string;
  container: string;
  protocol: DeployProtocol;
}

export interface Draft {
  name: string;
  image: string;
  tag: string;
  env: EnvRow[];
  ports: PortRow[];
  command: string;
  /** `null` tant que les bornes de la machine ne sont pas lues : les curseurs prennent alors leur valeur de départ. */
  resources: Resources | null;
}

export function emptyDraft(): Draft {
  return { name: "", image: "", tag: "", env: [], ports: [], command: "", resources: null };
}

/** La valeur d'une variable est-elle montrée en clair ? Le choix de l'utilisateur d'abord, sinon le nom (BR-DEPLOY-006). */
export function valueShown(row: Pick<EnvRow, "name" | "reveal">): boolean {
  return row.reveal ?? !looksSecret(row.name);
}

/** Au moins un champ est rempli : fermer demande « Abandonner votre saisie ? » (BR-DEPLOY-029). */
export function isFilled(draft: Draft): boolean {
  return (
    draft.name !== "" ||
    draft.image !== "" ||
    draft.tag !== "" ||
    draft.env.length > 0 ||
    draft.ports.length > 0 ||
    draft.command !== ""
  );
}

/**
 * Efface la saisie de la mémoire, autant que JavaScript le permet : chaque valeur est écrasée avant que les
 * références ne soient lâchées (une chaîne est immuable, on ne peut qu'oublier la référence). Après l'appel le
 * brouillon est vide.
 */
export function wipeDraft(draft: Draft): void {
  for (const row of draft.env) {
    row.name = "";
    row.value = "";
  }
  draft.env.splice(0);
  draft.ports.splice(0);
  draft.name = "";
  draft.image = "";
  draft.tag = "";
  draft.command = "";
  draft.resources = null;
}

/** La version écrite dans le champ de l'image passe dans le champ Version (BR-DEPLOY-005). `moved` : un déplacement a eu lieu. */
export function movedVersion(
  image: string,
  tag: string,
): { image: string; tag: string; moved: boolean } {
  const { repository, embedded } = splitImage(image);
  if (embedded === null || imageIssue(image) !== null) return { image, tag, moved: false };
  // Deux versions différentes : on ne choisit pas pour l'utilisateur, l'erreur de conflit le dit.
  if (tag.trim() !== "" && tag.trim() !== embedded) return { image, tag, moved: false };
  return { image: repository, tag: embedded, moved: true };
}
