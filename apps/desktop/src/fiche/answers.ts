import type { FicheDetail, FicheQuestion } from "@/link";

/**
 * Les questions d'installation d'une fiche (HRT-102, BR-FICHE-016, BR-FICHE-017, BR-FICHE-038), des fonctions pures sans
 * E/S. Chaque question dit elle-même son genre, ses bornes, ses choix et son jeu de caractères : AUCUNE règle de jeu
 * n'est écrite ici (écart 14.26). C'est le MIROIR de ce que l'agent juge (`validate_value`) : le formulaire dit d'avance
 * ce que le serveur refusera, le serveur reste l'arbitre. Chaque fonction rend un CODE, jamais un texte, jamais la
 * valeur reçue (sauf UN caractère refusé, comme l'agent).
 */

export type AnswerIssue =
  | { code: "required" }
  | { code: "too_short"; min: number }
  | { code: "too_long"; max: number }
  | { code: "multiline" }
  /** `character` : le caractère refusé, `null` pour un secret (jamais un caractère d'un mot de passe). */
  | { code: "charset"; character: string | null }
  | { code: "not_integer" }
  | { code: "out_of_range"; min: number | null; max: number | null }
  | { code: "bad_choice" };

/** Un caractère qui n'a rien à faire dans un texte affiché ou écrit dans un fichier (comme `is_risky_char`). */
const RISKY = /[\p{Cc}\u2028\u2029\uFEFF\u200B-\u200F\u202A-\u202E\u2060-\u2064\u2066-\u2069]/u;

const FILE_STEM = /^[A-Za-z0-9_-]$/;
const ARG_SAFE = /^[A-Za-z0-9._:-]$/;

function lengthIssue(question: FicheQuestion, value: string): AnswerIssue | null {
  const length = [...value].length;
  const min = question.minLen ?? 0;
  if (value === "" && min > 0) return { code: "required" };
  if (length < min) return { code: "too_short", min };
  if (question.maxLen !== null && length > question.maxLen) {
    return { code: "too_long", max: question.maxLen };
  }
  return null;
}

/**
 * Le défaut d'une réponse, ou `null`. Une réponse vide n'est refusée que si la question exige une longueur
 * (un secret facultatif vide veut dire « aucun », un texte facultatif vide « non écrit »).
 */
export function answerIssue(question: FicheQuestion, value: string): AnswerIssue | null {
  switch (question.kind) {
    case "int": {
      if (value === "") return { code: "required" };
      const number = Number(value);
      if (!/^-?\d+$/.test(value) || !Number.isSafeInteger(number) || String(number) !== value) {
        return { code: "not_integer" };
      }
      const below = question.min !== null && number < question.min;
      const above = question.max !== null && number > question.max;
      return below || above ? { code: "out_of_range", min: question.min, max: question.max } : null;
    }
    case "choice":
    case "bool":
      return question.choices.some((choice) => choice.value === value)
        ? null
        : { code: "bad_choice" };
    case "secret": {
      const length = lengthIssue(question, value);
      if (length) return length;
      if (/[\r\n]/.test(value)) return { code: "multiline" };
      return RISKY.test(value) ? { code: "charset", character: null } : null;
    }
    default: {
      // « text » et tout genre que ce client ne connaît pas : l'agent juge.
      const length = lengthIssue(question, value);
      if (length) return length;
      if (/[\r\n]/.test(value)) return { code: "multiline" };
      for (const char of value) {
        const bad =
          question.charset === "file_stem"
            ? !FILE_STEM.test(char)
            : question.charset === "arg_safe"
              ? !ARG_SAFE.test(char)
              : RISKY.test(char);
        if (bad) return { code: "charset", character: char };
      }
      if (question.charset === "arg_safe" && /^[-+]/.test(value)) {
        return { code: "charset", character: value[0] ?? null };
      }
      return null;
    }
  }
}

/** Les questions qui s'appliquent à la variante choisie (BR-FICHE-060) : les autres disparaissent, sans message. */
export function visibleQuestions(detail: FicheDetail, variant: string | null): FicheQuestion[] {
  return detail.questions.filter(
    (question) =>
      question.variants.length === 0 || (variant !== null && question.variants.includes(variant)),
  );
}

/** La valeur de départ d'une question : celle que la fiche propose, rien pour un secret. */
export function startingAnswer(question: FicheQuestion): string {
  return question.secret ? "" : (question.proposed ?? "");
}

/** La réponse en cours d'une question : la saisie, sinon la valeur de départ. */
export function currentAnswer(
  question: FicheQuestion,
  answers: Readonly<Record<string, string>>,
): string {
  return answers[question.id] ?? startingAnswer(question);
}

/** Le défaut de chaque question visible, par identifiant ; une question sans défaut n'y figure pas. */
export function answerIssues(
  questions: readonly FicheQuestion[],
  answers: Readonly<Record<string, string>>,
): Map<string, AnswerIssue> {
  const found = new Map<string, AnswerIssue>();
  for (const question of questions) {
    const issue = answerIssue(question, currentAnswer(question, answers));
    if (issue) found.set(question.id, issue);
  }
  return found;
}

/** Les réponses à envoyer : celles des questions visibles, non vides (une réponse vide laisse la fiche décider). */
export function answersToSend(
  questions: readonly FicheQuestion[],
  answers: Readonly<Record<string, string>>,
): { id: string; value: string }[] {
  const out: { id: string; value: string }[] = [];
  for (const question of questions) {
    const value = currentAnswer(question, answers);
    if (value !== "") out.push({ id: question.id, value });
  }
  return out;
}

/** Les questions dont la réponse diffère de ce que la fiche propose (BR-FICHE-040) : un secret saisi compte. */
export function changedAnswers(
  questions: readonly FicheQuestion[],
  answers: Readonly<Record<string, string>>,
): FicheQuestion[] {
  return questions.filter(
    (question) => currentAnswer(question, answers) !== startingAnswer(question),
  );
}
