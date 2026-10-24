import { nameIssue } from "@/deploy/rules";
import type { FicheDetail, FicheInstallRequest, FichePort } from "@/link";
import type { Resources } from "@/resources";
import { answersToSend, visibleQuestions } from "./answers";
import { publishedPorts, readableHostPorts, startingHostPort } from "./ports";

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
  draft.name = "";
  draft.variant = null;
  draft.resources = null;
  draft.edited = false;
}

/** Les cases du récapitulatif qui partent avec la demande, plus les contrats que l'utilisateur a acceptés. */
export interface InstallChoices {
  accept: { reservedPorts: boolean; lowDisk: boolean };
  agreements: readonly string[];
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
  const resources = draft.resources;
  if (resources === null || resources.memoryBytes === null || resources.cpuMillis === null) {
    return null;
  }
  const published = publishedPorts(detail.ports, draft.optionalPorts);
  const readable = readableHostPorts(published, draft.hostPorts);
  if (readable.length !== published.length) return null;
  return {
    name: draft.name,
    fiche: draft.ficheId,
    version: draft.version,
    digest: draft.digest,
    variant: draft.variant,
    answers: answersToSend(visibleQuestions(detail, draft.variant), draft.answers),
    agreements: [...choices.agreements],
    ports: readable.map(({ id, deploy }) => ({ id, hostPort: deploy.hostPort })),
    optionalPorts: [...draft.optionalPorts],
    resources: { memoryBytes: resources.memoryBytes, cpuMillis: resources.cpuMillis },
    accept: { ...choices.accept },
  };
}

/** Le port du serveur à montrer pour un port de la fiche : la saisie, sinon la valeur de la fiche. */
export function shownHostPort(draft: Pick<FicheDraft, "hostPorts">, port: FichePort): string {
  return draft.hostPorts[port.id] ?? startingHostPort(port);
}
