import { flushPromises } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { bootCatalogue, settle, useFicheTestClock } from "@/test/fiche";

// Le Catalogue (HRT-102, HRT-241, BR-FICHE-001 à 006, 033, 063, 064) : une grille de fiches livrées avec l'agent, en deux
// sections, avec recherche et filtres, lisible de tous les comptes, sans bouton « Installer », avec le pied des fiches
// écartées. Sur le pont simulé dont l'agent a la forme réelle de Terraria.
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
  it("montre la fiche avec son badge, sa mémoire et « Voir la fiche », jamais un bouton « Installer »", async () => {
    const { wrapper } = await bootCatalogue();
    const card = wrapper.get("[data-fiche-card]");
    expect(card.get("h3").text()).toBe("Terraria");
    expect(card.text()).toContain("Le serveur officiel de Terraria, sans mod.");
    expect(card.get("[data-fiche-badge]").text()).toBe("Prêt à installer");
    expect(card.get("[data-fiche-memory]").text()).toBe(
      "Mémoire nécessaire non vérifiée par Hearth",
    );
    expect(card.text()).toContain("Voir la fiche");
    expect(wrapper.get("[data-catalogue-footer]").text()).toMatch(/^Fiches livrées avec Hearth/);
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
    const names = wrapper.findAll("[data-fiche-card] h3").map((item) => item.text());
    // Rangées par nom dans la section « Prêts à installer » (BR-FICHE-063).
    expect(names).toEqual(["Jeu d'essai", "Terraria"]);
    wrapper.unmount();
  });

  it("atténue un jeu sans serveur pour ce système et le dit", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      const rich = bridge.catalog.addRich();
      rich.card.status = "published";
      rich.card.installable = false;
      bridge.catalog.put(rich);
    });
    expect(wrapper.get('[data-fiche="essai"]').text()).toContain(
      "Ce jeu n'a pas de serveur pour ce système.",
    );
    expect(wrapper.get('[data-fiche="essai"]').find("[data-fiche-open]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("les sections, la recherche et les filtres (BR-FICHE-063, BR-FICHE-033)", () => {
  const withWaiting = (bridge: { catalog: { addRich(): unknown; addWaiting(): unknown } }) => {
    bridge.catalog.addRich();
    bridge.catalog.addWaiting();
  };

  it("range « Prêts à installer » puis « Bientôt dans Hearth », chacune par nom", async () => {
    const { wrapper } = await bootCatalogue(withWaiting);
    const titles = wrapper.findAll(".cat__title").map((item) => item.text());
    expect(titles).toEqual(["Prêts à installer", "Bientôt dans Hearth"]);
    const soon = wrapper
      .findAll("[data-catalogue-soon] [data-fiche-card] h3")
      .map((item) => item.text());
    expect(soon).toEqual([
      "7 Days to Die",
      "Core Keeper",
      "Factorio",
      "Palworld",
      "Project Zomboid",
      "Rust",
      "Satisfactory",
      "V Rising",
    ]);
    wrapper.unmount();
  });

  it("une carte « Bientôt » dit sa raison, sans bouton, sans clic, sans mémoire", async () => {
    const { wrapper, router } = await bootCatalogue(withWaiting);
    const card = wrapper.get('[data-fiche="factorio"]');
    expect(card.get("[data-fiche-badge]").text()).toBe("Bientôt");
    expect(card.get("[data-fiche-reason]").text()).toBe(
      "Le nom de la partie, son mot de passe et le nombre de joueurs ne se règlent pas encore depuis Hearth.",
    );
    expect(card.find("[data-fiche-open]").exists()).toBe(false);
    expect(card.find("[data-fiche-memory]").exists()).toBe(false);
    const before = router.currentRoute.value.fullPath;
    await card.trigger("click");
    await settle();
    expect(router.currentRoute.value.fullPath).toBe(before);
    wrapper.unmount();
  });

  it("les filtres portent les comptes ; « Bientôt » garde la seule section qui attend", async () => {
    const { wrapper } = await bootCatalogue(withWaiting);
    const radios = () => wrapper.findAll("[data-catalogue-filters] [role=radio]");
    expect(radios().map((b) => b.text())).toEqual([
      "Tous les jeux 10",
      "Prêts à installer 2",
      "Bientôt 8",
    ]);
    await radios()[2]?.trigger("click");
    expect(wrapper.find("[data-catalogue-ready]").exists()).toBe(false);
    expect(wrapper.findAll("[data-catalogue-soon] [data-fiche-card]")).toHaveLength(8);
    wrapper.unmount();
  });

  it("la recherche ignore majuscules et accents, dit quand rien ne correspond, Échap la vide", async () => {
    const { wrapper } = await bootCatalogue(withWaiting);
    const search = wrapper.get("input[type=search]");
    await search.setValue("PALWORLD");
    expect(
      wrapper.findAll("[data-fiche-card]").map((card) => card.attributes("data-fiche")),
    ).toEqual(["palworld"]);
    await search.setValue("zzzz");
    expect(wrapper.get("[data-catalogue-no-match]").text()).toBe(
      "Aucun jeu ne correspond à « zzzz ». Essayez un autre nom.",
    );
    await wrapper.get("[data-catalogue-toolbar]").trigger("keydown", { key: "Escape" });
    expect(wrapper.findAll("[data-fiche-card]")).toHaveLength(10);
    wrapper.unmount();
  });

  it("une recherche cachée par le filtre ne dit pas que le jeu n'existe pas : elle propose de tout afficher (revue UX v2, N5)", async () => {
    const { wrapper } = await bootCatalogue(withWaiting);
    await wrapper.findAll("[data-catalogue-filters] [role=radio]")[1]?.trigger("click");
    await wrapper.get("input[type=search]").setValue("palworld");
    expect(wrapper.get("[data-catalogue-no-match]").text()).toContain(
      "« palworld » existe, mais le filtre choisi le cache.",
    );
    await wrapper.get("[data-catalogue-show-all]").trigger("click");
    expect(
      wrapper.findAll("[data-fiche-card]").map((card) => card.attributes("data-fiche")),
    ).toEqual(["palworld"]);
    wrapper.unmount();
  });

  it("sans fiche qui attend : ni filtre « Bientôt », ni section", async () => {
    const { wrapper } = await bootCatalogue();
    const labels = wrapper
      .findAll("[data-catalogue-filters] [role=radio]")
      .map((button) => button.text());
    expect(labels).toEqual(["Tous les jeux 1", "Prêts à installer 1"]);
    expect(wrapper.find("[data-catalogue-soon]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("la carte entière (revue UX 2026-10-24, constat 21)", () => {
  it("ouvre la fiche d'un clic n'importe où sur la carte", async () => {
    const { wrapper, router } = await bootCatalogue();
    await wrapper.get(".fcard__name").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("fiche");
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
      "2 fiches écartées : leur contenu n'est pas accepté. Mettez Hearth à jour ou réinstallez-le.",
    );
    expect(wrapper.get("[data-catalogue-unreadable]").text()).toContain("ne peut pas lire : 1.");
    // Ni le dossier ni l'invariant écarté ne sont montrés.
    expect(wrapper.text()).not.toMatch(/I2|I3/);
    wrapper.unmount();
  });

  it("« J'ai déjà un serveur de jeu » ouvre l'aide existante", async () => {
    const { wrapper, router } = await bootCatalogue();
    await wrapper.get("[data-catalogue-have]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("existing-server-help");
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
