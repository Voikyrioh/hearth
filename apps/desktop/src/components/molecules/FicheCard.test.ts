import { mount } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { type FicheCard, SimulatedLinkBridge, setLinkBridge } from "@/link";
import { GIB } from "@/resources";
import FicheCardView from "./FicheCard.vue";

// Une carte de la grille (HRT-241, BR-FICHE-003, 033, 064) : badge, phrase, mémoire en une phrase ou raison, bouton seulement
// pour une fiche prête.
function card(extra: Partial<FicheCard> = {}): FicheCard {
  return {
    id: "valheim",
    name: "Valheim",
    summary: "Le serveur officiel de Valheim, sans mod.",
    version: 1,
    status: "published",
    installable: true,
    ...extra,
  };
}

function view(fiche: FicheCard, capacityBytes: number | null = 64 * GIB) {
  setLinkBridge(new SimulatedLinkBridge({ retryDelayMs: -1 }));
  return mount(FicheCardView, {
    props: { card: fiche, serverId: "forge", serverName: "forge", capacityBytes },
  });
}

afterEach(() => setLinkBridge(null));

describe("FicheCard : une fiche prête", () => {
  it("dit la mémoire conseillée avec la machine, porte « Voir la fiche » et ouvre la fiche", async () => {
    const wrapper = view(
      card({ memory: { recommendedBytes: 6 * GIB, evidence: { kind: "measured" } } }),
    );
    expect(wrapper.get("[data-fiche-badge]").text()).toBe("Prêt à installer");
    expect(wrapper.get("[data-fiche-memory]").text()).toBe(
      "6 Go de mémoire conseillés, forge en a 64 Go",
    );
    expect(wrapper.get("[data-fiche-open]").attributes("aria-label")).toBe(
      "Voir la fiche de Valheim",
    );
    await wrapper.trigger("click");
    expect(wrapper.emitted("open")).toHaveLength(1);
  });

  it("le bouton et la carte n'ouvrent la fiche qu'une fois chacun, sans doublon de clic", async () => {
    const wrapper = view(card());
    await wrapper.get("[data-fiche-open]").trigger("click");
    expect(wrapper.emitted("open")).toHaveLength(1);
  });

  it("dit le chiffre de l'éditeur sans citer la machine", () => {
    const wrapper = view(
      card({
        memory: {
          recommendedBytes: 16 * GIB,
          evidence: { kind: "publisher", url: "https://exemple.test" },
        },
      }),
    );
    expect(wrapper.get("[data-fiche-memory]").text()).toBe(
      "16 Go de mémoire selon l'éditeur, non mesurés par Hearth",
    );
  });

  it("une plateforme sans serveur le dit et n'a pas de bouton", () => {
    const wrapper = view(card({ installable: false }));
    expect(wrapper.find("[data-fiche-open]").exists()).toBe(false);
    expect(wrapper.get("[data-fiche-not-installable]").text()).toBe(
      "Ce jeu n'a pas de serveur pour ce système.",
    );
  });
});

describe("FicheCard : une fiche qui attend", () => {
  const waiting = () =>
    card({
      id: "factorio",
      name: "Factorio",
      status: "preview",
      installable: false,
      previewReason: "Le nom de la partie ne se règle pas encore depuis Hearth.",
    });

  it("montre « Bientôt » et sa raison, ni bouton, ni mémoire, ni ouverture au clic", async () => {
    const wrapper = view(waiting());
    expect(wrapper.get("[data-fiche-badge]").text()).toBe("Bientôt");
    expect(wrapper.get("[data-fiche-reason]").text()).toBe(
      "Le nom de la partie ne se règle pas encore depuis Hearth.",
    );
    expect(wrapper.find("[data-fiche-open]").exists()).toBe(false);
    expect(wrapper.find("[data-fiche-memory]").exists()).toBe(false);
    await wrapper.trigger("click");
    expect(wrapper.emitted("open")).toBeUndefined();
  });

  it("sans raison (agent modifié), la carte s'affiche sans phrase de raison", () => {
    const wrapper = view({ ...waiting(), previewReason: null });
    expect(wrapper.find("[data-fiche-reason]").exists()).toBe(false);
    expect(wrapper.get("[data-fiche-badge]").text()).toBe("Bientôt");
  });

  it("n'interprète jamais le texte de la raison", () => {
    const wrapper = view({ ...waiting(), previewReason: "<b>gras</b> et <script>x</script>" });
    expect(wrapper.find("b").exists()).toBe(false);
    expect(wrapper.get("[data-fiche-reason]").text()).toContain("<b>gras</b>");
  });
});
