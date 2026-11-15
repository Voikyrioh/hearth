import { nameIssue } from "@/deploy/rules";
import type { FicheDetail, FicheInstallRequest, FichePort } from "@/link";
import type { Resources } from "@/resources";
import { answersToSend, extraAnswersToSend, extraSettings, visibleQuestions } from "./answers";
import { type PackDraft, packIssue } from "./pack";
import { publishedPorts, readableHostPorts, startingHostPort } from "./ports";
import {
  hiddenByWorld,
  newWorldDraft,
  type WorldDraft,
  wipeWorldDraft,
  worldMissing,
  worldPickOf,
} from "./world";

/**
 * La saisie de l'installation d'un serveur de jeu (HRT-102, BR-FICHE-034) et la demande qu'on en tire, des fonctions pures
 * sans E/S. Le brouillon vit en mémoire SEULEMENT (`stores/ficheInstall.ts`) : jamais dans l'adresse, sur le disque,
 * dans un instantané ou une trace. Une réponse peut être un secret (mot de passe du serveur, jeton) : elle reste dans un
 * conteneur mémoire effaçable jusqu'à ce que l'issue de l'installation soit connue, puis `wipeFicheDraft` l'efface.
 */

export interface FicheDraft {
  /** La fiche telle que l'utilisateur l'a LUE : l'installation reprend sa version et son empreinte. */
  ficheId: string;
  version: number;
  digest: string;
  name: string;
  /** `null` : la fiche n'a pas de variante. */
  variant: string | null;
  /** Les réponses saisies, par identifiant de question ; une question non touchée n'y figure pas. */
  answers: Record<string, string>;
  /** Les ports du serveur saisis, par identifiant de port de la fiche ; un port non touché n'y figure pas. */
  hostPorts: Record<string, string>;
  /** Les identifiants des ports facultatifs que l'utilisateur publie. */
  optionalPorts: string[];
  /** La carte « Monde » (HRT-106) : un monde neuf, ou la reprise d'un monde existant. */
  world: WorldDraft;
  /** « Installer avec un pack de mods » est coché (HRT-125, BR-MOD-006). */
  packWanted: boolean;
  /** Le pack déposé et ce que l'agent en a lu ; `null` : aucun fichier choisi. Jamais gardé hors de la mémoire. */
  pack: PackDraft | null;
  /** `null` tant que les bornes de la machine ne sont pas lues : les curseurs prennent alors leur valeur de départ. */
  resources: Resources | null;
  /** L'utilisateur a touché au moins un champ : fermer demande « Abandonnez-vous votre saisie ? ». */
  edited: boolean;
}

/** La variante proposée : celle que la fiche marque par défaut, sinon la première ; `null` sans variante. */
export function defaultVariant(detail: FicheDetail): string | null {
  const proposed = detail.variants.find((variant) => variant.default) ?? detail.variants[0];
  return proposed?.id ?? null;
}

export function newFicheDraft(detail: FicheDetail, name: string): FicheDraft {
  return {
    ficheId: detail.card.id,
    version: detail.card.version,
    digest: detail.digest,
    name,
    variant: defaultVariant(detail),
    answers: {},
    hostPorts: {},
    optionalPorts: [],
    world: newWorldDraft(),
    packWanted: false,
    pack: null,
    resources: null,
    edited: false,
  };
}

/**
 * Efface la saisie de la mémoire, autant que JavaScript le permet : chaque valeur est écrasée avant que les
 * références ne soient lâchées (une chaîne est immuable, on ne peut qu'oublier la référence). Après l'appel le
 * brouillon est vide.
 */
export function wipeFicheDraft(draft: FicheDraft): void {
  for (const id of Object.keys(draft.answers)) draft.answers[id] = "";
  for (const id of Object.keys(draft.answers)) delete draft.answers[id];
  for (const id of Object.keys(draft.hostPorts)) delete draft.hostPorts[id];
  draft.optionalPorts.splice(0);
  wipeWorldDraft(draft.world);
  draft.name = "";
  draft.variant = null;
  draft.packWanted = false;
  draft.pack = null;
  draft.resources = null;
  draft.edited = false;
}

/** Les cases du récapitulatif qui partent avec la demande, plus les contrats que l'utilisateur a acceptés. */
export interface InstallChoices {
  accept: { reservedPorts: boolean; lowDisk: boolean };
  agreements: readonly string[];
  /** Le geste « Je comprends » du récapitulatif, quand une reprise de monde le demande (BR-MONDE-016). */
  worldVersionUnderstood?: boolean;
}

/**
 * La demande d'installation, ou `null` si le brouillon n'est pas prêt (nom refusé, port illisible, ressources non
 * réglées : BR-RES-037, aucune valeur « sans limite »). Les réponses envoyées sont celles des questions visibles pour
 * la variante choisie, non vides.
 */
export function toInstallRequest(
  detail: FicheDetail,
  draft: FicheDraft,
  choices: InstallChoices,
): FicheInstallRequest | null {
  if (nameIssue(draft.name) !== null) return null;
  // Reprendre un monde sans l'avoir choisi : la demande ne part pas (BR-MONDE-002).
  if (worldMissing(draft.world)) return null;
  const hidden = hiddenByWorld(detail, draft.world);
  if (packIssue(detail, draft.packWanted, draft.pack, draft.variant) !== null) return null;
  const resources = draft.resources;
  if (resources === null || resources.memoryBytes === null || resources.cpuMillis === null) {
    return null;
  }
  const published = publishedPorts(detail.ports, draft.optionalPorts, draft.variant);
  const readable = readableHostPorts(published, draft.hostPorts);
  if (readable.length !== published.length) return null;
  return {
    name: draft.name,
    fiche: draft.ficheId,
    version: draft.version,
    digest: draft.digest,
    variant: draft.variant,
    // Le monde repris porte son nom et ses réglages figés : aucune réponse du client pour eux (BR-MONDE-014).
    answers: [
      ...answersToSend(visibleQuestions(detail, draft.variant), draft.answers),
      ...extraAnswersToSend(extraSettings(detail, draft.variant), draft.answers),
    ].filter((answer) => !hidden.has(answer.id)),
    agreements: [...choices.agreements],
    ports: readable.map(({ port, deploy }) => ({ id: port.id, hostPort: deploy.hostPort })),
    optionalPorts: published.filter((port) => port.publish === "optional").map((port) => port.id),
    world: worldPickOf(draft.world, choices.worldVersionUnderstood === true),
    resources: { memoryBytes: resources.memoryBytes, cpuMillis: resources.cpuMillis },
    accept: { ...choices.accept },
    pack: draft.packWanted ? (draft.pack?.deposit ?? null) : null,
  };
}

/** Le port du serveur à montrer pour un port de la fiche : la saisie, sinon la valeur de la fiche. */
export function shownHostPort(draft: Pick<FicheDraft, "hostPorts">, port: FichePort): string {
  return draft.hostPorts[port.id] ?? startingHostPort(port);
}
