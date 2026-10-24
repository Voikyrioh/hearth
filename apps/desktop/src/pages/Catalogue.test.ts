import { flushPromises } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { bootCatalogue, settle, useFicheTestClock } from "@/test/fiche";

// Le Catalogue (HRT-102, BR-FICHE-001 à 006) : une pile de fiches livrées avec l'agent, lisible de tous les comptes, sans
// bouton « Installer », avec le pied des fiches écartées. Sur le pont simulé dont l'agent a la forme réelle de Terraria.
useFicheTestClock();

describe("l'entrée du menu", () => {
  it("est une entrée de premier niveau, après « Services », pour les deux rôles", async () => {
    for (const [server, path] of [
      ["forge", "/servers/forge/catalogue"],
      ["salon", "/servers/salon/catalogue"],
    ] as const) {
      const { wrapper } = await bootCatalogue(undefined, path, server);
      const labels = wrapper.findAll(".nav__item").map((item) => item.text());
      const services = labels.indexOf("Services");
      expect(services).toBeGreaterThanOrEqual(0);
      expect(labels[services + 1]).toBe("Catalogue");
      expect(wrapper.get(".nav__item.router-link-active").text()).toBe("Catalogue");
      expect(wrapper.get("h1").text()).toBe("Catalogue");
      wrapper.unmount();
    }
  });
});

describe("la liste (BR-FICHE-003)", () => {
  it("montre la fiche avec « Livrée avec Hearth » et « Voir la fiche », jamais un bouton « Installer »", async () => {
    const { wrapper } = await bootCatalogue();
    const card = wrapper.get("[data-fiche-card]");
    expect(card.get("h2").text()).toBe("Terraria");
    expect(card.text()).toContain("Le serveur officiel de Terraria, sans mod.");
    expect(card.get("[data-fiche-delivered]").text()).toMatch(/^Livrée avec Hearth/);
    expect(card.text()).toContain("Voir la fiche");
    expect(wrapper.find("[data-catalogue-list]").text()).not.toMatch(/Installer/);
    // La liste ne dit rien d'un compte exigé ni d'une origine autre que la livraison avec l'agent.
    expect(wrapper.find("[data-catalogue]").text()).not.toMatch(/compte exig|signature|isolé|sûr/i);
    wrapper.unmount();
  });

  it("ouvre la fiche au clic sur « Voir la fiche »", async () => {
    const { wrapper, router } = await bootCatalogue();
    await wrapper.get("[data-fiche-open]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("fiche");
    expect(router.currentRoute.value.params.fiche).toBe("terraria");
    wrapper.unmount();
  });

  it("range plusieurs fiches dans l'ordre de l'agent, une carte chacune", async () => {
    const { wrapper } = await bootCatalogue((bridge) => bridge.catalog.addRich());
    const names = wrapper.findAll("[data-fiche-card] h2").map((item) => item.text());
    expect(names).toEqual(["Terraria", "Jeu d'essai"]);
    wrapper.unmount();
  });

  it("atténue un jeu sans serveur pour ce système et le dit", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      const rich = bridge.catalog.addRich();
      rich.card.installable = false;
      bridge.catalog.put(rich);
    });
    expect(wrapper.get('[data-fiche="essai"]').text()).toContain(
      "Ce jeu n'a pas de serveur pour ce système.",
    );
    expect(wrapper.get('[data-fiche="essai"]').classes()).toContain("fcard--dim");
    wrapper.unmount();
  });
});

describe("le pied de la liste (BR-FICHE-006)", () => {
  it("dit les fiches écartées et celles que ce client ne sait pas lire", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      bridge.catalog.refused = [
        { dir: "a", invariant: "I2" },
        { dir: "b", invariant: "I3" },
      ];
      bridge.catalog.unreadable = 1;
    });
    expect(wrapper.get("[data-catalogue-refused]").text()).toBe(
      "2 fiche(s) écartée(s) : leur contenu n'est pas accepté. Mettez Hearth à jour ou réinstallez-le.",
    );
    expect(wrapper.get("[data-catalogue-unreadable]").text()).toContain("1 fiche(s)");
    // Ni le dossier ni l'invariant écarté ne sont montrés.
    expect(wrapper.text()).not.toMatch(/I2|I3/);
    wrapper.unmount();
  });

  it("n'ajoute aucune ligne vers une aide « Installer un autre serveur »", async () => {
    const { wrapper } = await bootCatalogue();
    expect(wrapper.find("[data-catalogue]").text()).not.toMatch(
      /autre serveur|J'ai déjà un serveur/,
    );
    wrapper.unmount();
  });
});

describe("les états", () => {
  it("dit qu'aucune fiche n'est disponible", async () => {
    const { wrapper } = await bootCatalogue((bridge) => bridge.catalog.remove("terraria"));
    expect(wrapper.get("[data-catalogue-empty]").text()).toContain(
      "Aucune fiche n'est disponible sur ce serveur.",
    );
    wrapper.unmount();
  });

  it("dit qu'un agent sans les services doit être mis à jour", async () => {
    const { wrapper } = await bootCatalogue((bridge) =>
      bridge.services.setUnsupported("forge", "0.1.0"),
    );
    expect(wrapper.get("[data-catalogue-old-agent]").text()).toContain(
      "Ce serveur doit être mis à jour pour proposer des fiches.",
    );
    expect(wrapper.find("[data-catalogue-list]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("reste lisible quand le moteur n'est pas prêt, avec son encart en tête", async () => {
    const { wrapper } = await bootCatalogue((bridge) =>
      bridge.services.setEngine("forge", { state: "stopped" }),
    );
    expect(wrapper.find("[data-catalogue-list]").exists()).toBe(true);
    expect(wrapper.find("[data-engine-state]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("dit l'échec de la lecture et la refait au clic", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => {
      b.catalog.failRead = true;
    });
    expect(wrapper.get("[data-catalogue-failed]").text()).toContain(
      "Impossible de lire le catalogue",
    );
    bridge.catalog.failRead = false;
    await wrapper.get("[data-catalogue-retry]").trigger("click");
    await settle();
    await flushPromises();
    expect(wrapper.find("[data-catalogue-list]").exists()).toBe(true);
    wrapper.unmount();
  });
});
