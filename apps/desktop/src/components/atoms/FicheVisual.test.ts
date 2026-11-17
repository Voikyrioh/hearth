import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { type FicheCard, SimulatedLinkBridge, setLinkBridge } from "@/link";
import FicheVisual from "./FicheVisual.vue";

// L'image d'un jeu ou son visuel neutre (HRT-239, BR-FICHE-062) : choix de la source, repli sur erreur, ratio réservé.
const HEX = `sha256:${"ab".repeat(32)}`;

function card(extra: Partial<FicheCard> = {}): FicheCard {
  return {
    id: "terraria",
    name: "Terraria",
    summary: "Un jeu.",
    version: 1,
    status: "published",
    installable: true,
    ...extra,
  };
}

function visual(fiche: FicheCard, variant: "card" | "banner" = "card") {
  setLinkBridge(new SimulatedLinkBridge({ retryDelayMs: -1 }));
  return mount(FicheVisual, { props: { serverId: "forge", card: fiche, variant } });
}

afterEach(() => setLinkBridge(null));

describe("FicheVisual : la source de l'image", () => {
  it("prend l'en-tête Steam quand la fiche porte l'identifiant du jeu", () => {
    const wrapper = visual(card({ steamAppId: 105600 }));
    const img = wrapper.get("[data-fiche-image]");
    expect(img.attributes("src")).toBe("/fixtures/illustrations/steam-105600.jpg");
    expect(img.attributes("alt")).toBe("Image du jeu Terraria");
    expect(img.attributes("loading")).toBe("lazy");
    expect(wrapper.find("[data-fiche-neutral]").exists()).toBe(false);
  });

  it("prend l'illustration de l'agent à défaut d'identifiant Steam", () => {
    const wrapper = visual(
      card({
        id: "minecraft",
        name: "Minecraft Java",
        illustration: {
          digest: HEX,
          bytes: 1000,
          width: 920,
          height: 430,
          mediaType: "image/jpeg",
        },
      }),
    );
    expect(wrapper.get("[data-fiche-image]").attributes("src")).toBe(
      "/fixtures/illustrations/agent-minecraft.jpg",
    );
  });

  it("montre le visuel neutre, décoratif, quand la fiche n'a aucune image", () => {
    const wrapper = visual(card());
    expect(wrapper.find("[data-fiche-image]").exists()).toBe(false);
    const neutral = wrapper.get("[data-fiche-neutral]");
    expect(neutral.attributes("aria-hidden")).toBe("true");
    expect(neutral.text()).toBe("");
  });

  it("un identifiant Steam qui n'est pas un entier valide n'envoie rien : visuel neutre", () => {
    expect(
      visual(card({ steamAppId: 0 }))
        .find("[data-fiche-image]")
        .exists(),
    ).toBe(false);
    expect(
      visual(card({ steamAppId: 1.5 }))
        .find("[data-fiche-image]")
        .exists(),
    ).toBe(false);
  });
});

describe("FicheVisual : repli et mise en page", () => {
  it("passe au visuel neutre dès que l'image échoue", async () => {
    const wrapper = visual(card({ steamAppId: 105600 }));
    await wrapper.get("[data-fiche-image]").trigger("error");
    expect(wrapper.find("[data-fiche-image]").exists()).toBe(false);
    expect(wrapper.find("[data-fiche-neutral]").exists()).toBe(true);
  });

  it("un jeu sans fixture (volontairement) a une source mais échoue vers le repli", async () => {
    const wrapper = visual(card({ id: "valheim", steamAppId: 892970 }));
    expect(wrapper.find("[data-fiche-image]").exists()).toBe(true);
    await wrapper.get("[data-fiche-image]").trigger("error");
    expect(wrapper.find("[data-fiche-neutral]").exists()).toBe(true);
  });

  it("le bandeau ajoute un fond flou décoratif et garde un seul texte alternatif", () => {
    const wrapper = visual(card({ steamAppId: 105600 }), "banner");
    expect(wrapper.classes()).toContain("fv--banner");
    const blurred = wrapper.get(".fv__blur");
    expect(blurred.attributes("alt")).toBe("");
    expect(blurred.attributes("aria-hidden")).toBe("true");
    expect(wrapper.findAll("img[alt='Image du jeu Terraria']")).toHaveLength(1);
  });

  it("la grille réserve le rapport de l'image d'avance (aucun saut)", () => {
    expect(visual(card()).classes()).toContain("fv--card");
  });
});
