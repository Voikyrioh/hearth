import type { FicheDetail, FicheQuestion } from "@/link";

/**
 * La répartition des questions de la fiche à l'étape « Réglages » (HRT-244, BR-FICHE-065), une fonction pure. L'essentiel :
 * la question qui nomme le monde (celle que `worldImport.namesSetting` désigne), les questions secrètes et les questions
 * sans valeur proposée (l'utilisateur DOIT répondre). Tout le reste va dans « Réglages avancés », replié, avec ses valeurs
 * proposées. L'ordre de la fiche est conservé dans les deux listes. Le nom du service, la variante et les ports ne sont
 * pas des questions : ils sont toujours dans l'essentiel.
 */
export interface SplitQuestions {
  essential: FicheQuestion[];
  advanced: FicheQuestion[];
}

export function splitQuestions(
  questions: readonly FicheQuestion[],
  detail: Pick<FicheDetail, "worldImport"> | null,
): SplitQuestions {
  const worldQuestion = detail?.worldImport?.namesSetting ?? null;
  const essential: FicheQuestion[] = [];
  const advanced: FicheQuestion[] = [];
  for (const question of questions) {
    const must = question.id === worldQuestion || question.secret || question.proposed === null;
    (must ? essential : advanced).push(question);
  }
  return { essential, advanced };
}

/** « Réglages avancés : Taille du monde, Difficulté, et 2 autres » : quatre libellés au plus. */
export function advancedLabels(
  advanced: readonly FicheQuestion[],
  max = 4,
): { shown: string[]; more: number } {
  const shown = advanced.slice(0, max).map((question) => question.label);
  return { shown, more: Math.max(0, advanced.length - max) };
}
