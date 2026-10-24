import { describe, expect, it } from "vitest";
import type { FicheQuestion } from "@/link";
import { characterName, correctedText } from "./character";
import { answerIssueText, extraHint, valueLabel } from "./messages";

// Revue UX 2026-10-24 : constat 4 (un choix par son libellé), constat 5 (le caractère refusé en mots et la correction),
// constat 18 (une aide dite une seule fois).

const question = (patch: Partial<FicheQuestion>): FicheQuestion => ({
  id: "q",
  label: "Question",
  help: "",
  secret: false,
  proposed: null,
  kind: "text",
  choices: [],
  min: null,
  max: null,
  minLen: null,
  maxLen: null,
  charset: null,
  frozenAfterCreation: false,
  variants: [],
  secretKind: null,
  obtainUrl: null,
  ...patch,
});

describe("valueLabel", () => {
  it("dit un choix par son libellé et un oui-non par Oui ou Non, jamais par la valeur de la fiche", () => {
    const difficulty = question({
      kind: "choice",
      choices: [
        { value: "0", label: "Normal" },
        { value: "1", label: "Expert" },
        { value: "2", label: "Maître" },
      ],
    });
    expect(valueLabel(difficulty, "2")).toBe("Maître");
    expect(valueLabel(difficulty, "9")).toBe("9");
    const pvp = question({
      kind: "bool",
      choices: [
        { value: "true", label: "true" },
        { value: "false", label: "false" },
      ],
    });
    expect(valueLabel(pvp, "true")).toBe("Oui");
    expect(valueLabel(pvp, "false")).toBe("Non");
    expect(valueLabel(question({}), "Monde")).toBe("Monde");
  });
});

describe("characterName", () => {
  it("nomme en mots ce qu'on ne voit pas ou ce qui se confond", () => {
    expect(characterName(" ")).toBe("l'espace");
    expect(characterName("!")).toBe("le point d'exclamation");
    expect(characterName(".")).toBe("le point");
    expect(characterName("é")).toBe("la lettre accentuée « é »");
    expect(characterName("€")).toBe("le caractère « € »");
  });
});

describe("correctedText", () => {
  it("propose le nom corrigé : espaces en soulignés, accents retirés, le reste enlevé", () => {
    expect(correctedText("file_stem", "Mon monde")).toBe("Mon_monde");
    expect(correctedText("file_stem", "Mon monde!")).toBe("Mon_monde");
    expect(correctedText("file_stem", "Été à la mer")).toBe("Ete_a_la_mer");
    expect(correctedText("arg_safe", "-x")).toBe("x");
  });

  it("ne propose rien quand rien ne change, quand il ne reste rien ou quand le jeu de caractères est inconnu", () => {
    expect(correctedText("file_stem", "Monde_1")).toBeNull();
    expect(correctedText("file_stem", "!!!")).toBeNull();
    expect(correctedText("line", "a b")).toBeNull();
    expect(correctedText(null, "a b")).toBeNull();
  });
});

describe("le message d'un caractère refusé", () => {
  it("nomme le caractère et propose la correction", () => {
    expect(answerIssueText({ code: "charset", character: " ", suggestion: "Mon_monde" })).toBe(
      "Ce texte contient un caractère que ce jeu ne sait pas lire : l'espace. Essayez « Mon_monde ».",
    );
    expect(answerIssueText({ code: "charset", character: "!", suggestion: null })).toBe(
      "Ce texte contient un caractère que ce jeu ne sait pas lire : le point d'exclamation.",
    );
  });
});

describe("extraHint", () => {
  const players = question({ kind: "int", min: 1, max: 255 });

  it("ajoute les bornes seulement quand l'aide de la fiche ne les dit pas", () => {
    expect(extraHint(players, "Combien de joueurs.")).toBe("Le nombre est compris entre 1 et 255.");
    expect(extraHint(players, "Combien de joueurs, de 1 à 255.")).toBeNull();
    // 25 n'est pas 255 : un nombre qui en contient un autre ne compte pas.
    expect(extraHint(players, "Combien de joueurs, de 1 à 25.")).toBe(
      "Le nombre est compris entre 1 et 255.",
    );
  });

  it("ajoute le jeu de caractères seulement quand l'aide ne parle pas de lettres ni de chiffres", () => {
    const name = question({ charset: "file_stem" });
    expect(extraHint(name, "Le nom du monde.")).toBe(
      "Lettres, chiffres, tirets et soulignés seulement.",
    );
    expect(extraHint(name, "Lettres, chiffres, tiret et souligné seulement.")).toBeNull();
  });
});
