import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import ProofTag from "@/components/molecules/ProofTag.vue";
import QuestionField from "@/components/molecules/QuestionField.vue";
import VariantChoice from "@/components/molecules/VariantChoice.vue";
import FirstStartPanel from "@/components/organisms/FirstStartPanel.vue";
import type { FicheQuestion } from "@/link";
import { rich } from "@/link/simulated-fiches";

// Les composants de la fiche (HRT-102), seuls : le genre d'une question décide son champ, la preuve est dite en mots,
// le premier démarrage ne dit que des faits.
const question = (patch: Partial<FicheQuestion>): FicheQuestion => ({
  id: "q",
  label: "Question",
  help: "Aide de la fiche.",
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

describe("ProofTag", () => {
  it.each([
    ["measured", "Mesuré par Hearth"],
    ["measured_idle", "Mesuré au repos"],
    ["publisher", "Chiffre de l'éditeur"],
    ["unmeasured", "Non vérifié"],
  ] as const)("dit %s en mots, avec un pictogramme", (proof, label) => {
    const wrapper = mount(ProofTag, { props: { proof } });
    expect(wrapper.text()).toContain(label);
    expect(wrapper.find("svg").exists()).toBe(true);
  });

  it("ne propose « Voir la source » que pour un chiffre de l'éditeur, et l'émet", async () => {
    const without = mount(ProofTag, { props: { proof: "measured" } });
    expect(without.find("[data-proof-source]").exists()).toBe(false);
    const wrapper = mount(ProofTag, {
      props: { proof: "publisher", source: true, subject: "Mémoire" },
    });
    expect(wrapper.get("[data-proof-source]").attributes("aria-label")).toContain("Mémoire");
    await wrapper.get("[data-proof-source]").trigger("click");
    expect(wrapper.emitted("source")).toHaveLength(1);
  });
});

describe("QuestionField", () => {
  it("rend un choix de quatre valeurs au plus en boutons exclusifs, plus en liste", () => {
    const four = mount(QuestionField, {
      props: {
        question: question({
          kind: "choice",
          choices: ["a", "b", "c", "d"].map((value) => ({ value, label: value.toUpperCase() })),
        }),
        modelValue: "b",
      },
    });
    expect(four.findAll('[role="radio"]').map((item) => item.text())).toEqual(["A", "B", "C", "D"]);
    const five = mount(QuestionField, {
      props: {
        question: question({
          kind: "choice",
          choices: ["a", "b", "c", "d", "e"].map((value) => ({
            value,
            label: value.toUpperCase(),
          })),
        }),
        modelValue: "b",
      },
    });
    expect(five.find("select").exists()).toBe(true);
    expect(five.findAll("option")).toHaveLength(5);
  });

  it("rend « oui ou non » avec les valeurs du jeu et émet la valeur du jeu, jamais « true » par défaut", async () => {
    const wrapper = mount(QuestionField, {
      props: {
        question: question({
          kind: "bool",
          choices: [
            { value: "1", label: "1" },
            { value: "0", label: "0" },
          ],
        }),
        modelValue: "0",
      },
    });
    const buttons = wrapper.findAll('[role="radio"]');
    expect(buttons.map((item) => item.text())).toEqual(["Oui", "Non"]);
    await buttons[0]?.trigger("click");
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["1"]);
  });

  it("rend un genre qu'il ne connaît pas en champ de texte : l'agent juge", () => {
    const wrapper = mount(QuestionField, {
      props: { question: question({ kind: "unknown" }), modelValue: "" },
    });
    expect(wrapper.find('input[type="text"]').exists()).toBe(true);
  });

  it("masque un secret, et dit sa mention de figé et l'aide du jeu de caractères", () => {
    const secret = mount(QuestionField, {
      props: {
        question: question({ kind: "secret", secret: true, secretKind: "game_password" }),
        modelValue: "abc",
      },
    });
    expect(secret.get("input").attributes("type")).toBe("password");
    const text = mount(QuestionField, {
      props: {
        question: question({ charset: "file_stem", frozenAfterCreation: true }),
        modelValue: "Monde",
      },
    });
    expect(text.text()).toContain("Lettres, chiffres, tirets et soulignés seulement.");
    expect(text.get("[data-question-frozen]").text()).toContain("ne pourra plus être changé");
  });
});

describe("VariantChoice", () => {
  it("coche la variante proposée et émet celle qu'on choisit", async () => {
    const wrapper = mount(VariantChoice, {
      props: { variants: rich().variants, modelValue: "vanilla" },
    });
    const inputs = wrapper.findAll<HTMLInputElement>("input[type=radio]");
    expect(inputs.map((input) => input.element.checked)).toEqual([true, false]);
    await inputs[1]?.setValue(true);
    expect(wrapper.emitted("update:modelValue")?.[0]).toEqual(["modded"]);
  });

  it("passe en liste au-delà de quatre variantes et montre l'aide de la variante choisie", () => {
    const many = Array.from({ length: 5 }, (_, index) => ({
      id: `v${index}`,
      label: `Variante ${index}`,
      help: `Aide ${index}`,
      default: index === 0,
    }));
    const wrapper = mount(VariantChoice, { props: { variants: many, modelValue: "v2" } });
    expect(wrapper.find("select").exists()).toBe(true);
    expect(wrapper.text()).toContain("Aide 2");
  });
});

describe("FirstStartPanel (BR-FICHE-056)", () => {
  const first = rich().firstStart as NonNullable<ReturnType<typeof rich>["firstStart"]>;

  it("dit des faits, annoncé en statut, sans barre ni pourcentage", () => {
    const wrapper = mount(FirstStartPanel, {
      props: {
        firstStart: first,
        facts: {
          stage: "Téléchargement des fichiers de jeu",
          writtenBytes: 3 * 1024 ** 3,
          elapsedSeconds: 60,
        },
      },
    });
    const panel = wrapper.get("[data-first-start]");
    expect(panel.attributes("role")).toBe("status");
    expect(panel.text()).toContain("Ce n'est pas une panne.");
    expect(panel.text()).toContain("Étape en cours : Téléchargement des fichiers de jeu.");
    expect(panel.text()).toContain("3 Go de fichiers de jeu déjà écrits, sur environ 6 Go.");
    expect(wrapper.find("progress, [role=progressbar], .meter").exists()).toBe(false);
    expect(panel.text()).not.toMatch(/%|restant/);
  });

  it("se remplace par « Hearth ne peut pas savoir si le jeu est prêt. » quand la durée annoncée est dépassée", () => {
    const wrapper = mount(FirstStartPanel, {
      props: {
        firstStart: first,
        facts: { stage: null, writtenBytes: null, elapsedSeconds: first.readyTimeoutSeconds + 1 },
      },
    });
    expect(wrapper.find("[data-first-start]").exists()).toBe(false);
    expect(wrapper.get("[data-first-start-unknown]").text()).toBe(
      "Hearth ne peut pas savoir si le jeu est prêt.",
    );
  });
});
