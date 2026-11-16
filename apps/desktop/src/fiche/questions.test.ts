import { describe, expect, it } from "vitest";
import type { FicheQuestion } from "@/link";
import { rich, terraria } from "@/link/simulated-fiches";
import { advancedLabels, splitQuestions } from "./questions";

// La répartition des questions à l'étape « Réglages » (BR-FICHE-065).
function question(id: string, extra: Partial<FicheQuestion> = {}): FicheQuestion {
  return {
    id,
    label: id.toUpperCase(),
    help: "",
    secret: false,
    proposed: "x",
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
    ...extra,
  };
}

describe("splitQuestions", () => {
  it("garde dans l'essentiel la question du monde, les secrets et celles sans valeur proposée", () => {
    const questions = [
      question("world_name"),
      question("password", { secret: true, proposed: null }),
      question("motd", { proposed: null }),
      question("difficulty"),
      question("players"),
    ];
    const split = splitQuestions(questions, {
      worldImport: { namesSetting: "world_name", carriedByWorld: [] },
    });
    expect(split.essential.map((q) => q.id)).toEqual(["world_name", "password", "motd"]);
    expect(split.advanced.map((q) => q.id)).toEqual(["difficulty", "players"]);
  });

  it("sans fiche qui nomme le monde, seules les questions à répondre sont essentielles", () => {
    const split = splitQuestions([question("a"), question("b", { proposed: null })], null);
    expect(split.essential.map((q) => q.id)).toEqual(["b"]);
    expect(split.advanced.map((q) => q.id)).toEqual(["a"]);
  });

  it("conserve l'ordre de la fiche et ne perd aucune question", () => {
    const detail = rich();
    const split = splitQuestions(detail.questions, detail);
    expect(split.essential.length + split.advanced.length).toBe(detail.questions.length);
    const order = detail.questions.map((q) => q.id);
    for (const list of [split.essential, split.advanced]) {
      const indexes = list.map((q) => order.indexOf(q.id));
      expect(indexes).toEqual([...indexes].sort((a, b) => a - b));
    }
    expect(splitQuestions(terraria().questions, terraria()).essential.length).toBeGreaterThan(0);
  });
});

describe("advancedLabels", () => {
  it("cite quatre libellés au plus puis le nombre des autres", () => {
    const six = ["a", "b", "c", "d", "e", "f"].map((id) => question(id));
    expect(advancedLabels(six)).toEqual({ shown: ["A", "B", "C", "D"], more: 2 });
    expect(advancedLabels(six.slice(0, 2))).toEqual({ shown: ["A", "B"], more: 0 });
  });
});
