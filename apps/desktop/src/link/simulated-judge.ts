import type { FicheList, FicheQuestion } from "./catalog";

/**
 * Le JUGE d'une valeur de réglage du pont SIMULÉ : la même règle que l'agent (`validate_value`,
 * `crates/hearth-agent/src/domain/catalog/loading/model.rs`), écrite ICI à la main et à part de celles du client
 * (`fiche/answers.ts`, `settings/edits.ts`). Si le client se trompe, ce juge le dit : il ne les importe jamais. Sert à
 * l'installation d'une fiche (`SimulatedCatalog`) et à l'enregistrement des réglages (`SimulatedGameSettings`). Jamais
 * livré (derrière `import.meta.env.DEV`).
 */

export interface Refusal {
  reason: string;
  /** Au plus UN caractère refusé, jamais pour un secret. */
  character: string | null;
}

const risky = (char: string) =>
  /[\p{Cc}\u2028\u2029\u200B-\u200F\u202A-\u202E\u2066-\u2069]/u.test(char);

/** Une valeur jugée comme le fait l'agent : la raison et, au plus, UN caractère refusé. `null` : permise. */
export function judgeValue(
  question: FicheQuestion,
  value: string,
  list: FicheList | null = null,
): Refusal | null {
  const refuse = (reason: string, character: string | null = null) => ({ reason, character });
  const length = [...value].length;
  const lengthBad =
    (question.minLen !== null && length < question.minLen) ||
    (question.maxLen !== null && length > question.maxLen);
  switch (question.kind) {
    case "text": {
      if (lengthBad) return refuse("length");
      for (const char of value) {
        const bad =
          question.charset === "file_stem"
            ? !/^[A-Za-z0-9_-]$/.test(char)
            : question.charset === "arg_safe"
              ? !/^[A-Za-z0-9._:-]$/.test(char)
              : risky(char);
        if (bad) return refuse("charset", char);
      }
      if (question.charset === "arg_safe" && /^[-+]/.test(value)) {
        return refuse("charset", value[0] ?? null);
      }
      return null;
    }
    case "secret": {
      if (lengthBad) return refuse("length");
      return [...value].some(risky) ? refuse("charset") : null;
    }
    case "int": {
      const number = Number(value);
      if (!/^-?\d+$/.test(value) || !Number.isSafeInteger(number) || String(number) !== value) {
        return refuse("bounds");
      }
      const below = question.min !== null && number < question.min;
      const above = question.max !== null && number > question.max;
      return below || above ? refuse("bounds") : null;
    }
    case "bool":
    case "choice":
      return question.choices.some((choice) => choice.value === value) ? null : refuse("choice");
    case "id_list": {
      if (value === "") return null;
      const items = value.split(list?.separator ?? ",");
      if (list && items.length > list.maxItems) return refuse("length");
      for (const item of items) {
        if (item === "" || [...item].length > 64) return refuse("length");
        const bad =
          list?.itemCharset === "digits"
            ? [...item].find((char) => !/^[0-9]$/.test(char))
            : list?.itemCharset === "slug"
              ? [...item].find((char) => !/^[a-z0-9_-]$/.test(char))
              : [...item].find(risky);
        if (bad !== undefined) return refuse("charset", bad);
      }
      return null;
    }
    default:
      // « opaque » et tout genre plus récent : longueur et caractères de contrôle.
      return lengthBad ? refuse("length") : [...value].some(risky) ? refuse("charset") : null;
  }
}
