import { flushPromises } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { bootCatalogue, dialogText, settle, useFicheTestClock } from "@/test/fiche";

// La fiche d'un jeu (HRT-102, BR-FICHE-004, 008, 010, 012, 021, 033, 036) : tout ce que la fiche offre, lu dans la
// fiche, avec la preuve de chaque chiffre. Sur le pont simulé : Terraria (aucune preuve) et la fiche d'essai (tout).
useFicheTestClock();

const GIB = 1024 ** 3;
const fiche = (id: string) => `/servers/forge/catalogue/${id}`;

describe("les cartes de la fiche (BR-FICHE-004)", () => {
  it("montre les huit cartes de Terraria avec les mots de la spec, et aucune ligne « compte exigé »", async () => {
    const { wrapper } = await bootCatalogue(undefined, fiche("terraria"));
    expect(wrapper.get("[data-page-title]").text()).toBe("Terraria");
    const titles = wrapper
      .findAll("[data-fiche-sheet] h2")
      .map((item) => item.text().toLowerCase());
    expect(titles).toEqual([
      "image",
      "ports",
      "dossiers",
      "ressources",
      "téléchargements",
      "arrêt du jeu",
      "questions",
      "ce que cette fiche impose",
    ]);
    expect(wrapper.text()).not.toMatch(/compte exig|nom d'utilisateur/i);
    expect(wrapper.get(".nav__item.router-link-active").text()).toBe("Catalogue");
    wrapper.unmount();
  });

  it("dit l'image en entier, son registre et sa version, et l'empreinte de la fiche entière", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, fiche("terraria"));
    const digest = bridge.catalog.get("terraria")?.digest ?? "";
    expect(wrapper.get("[data-image-ref]").text()).toBe("ryshe/terraria:vanilla-1.4.5.8");
    expect(wrapper.get("[data-fiche-image]").text()).toContain("le registre public par défaut");
    expect(wrapper.get("[data-fiche-image]").text()).toContain(
      "Version : vanilla-1.4.5.8 (la seule proposée par cette fiche).",
    );
    expect(wrapper.get("[data-fiche-image]").text()).toContain("non relevée");
    expect(wrapper.get("[data-fiche-digest]").text()).toContain(digest);
    wrapper.unmount();
  });

  it("liste les ports, les dossiers, les questions (au plus huit) et ce que la fiche impose", async () => {
    const { wrapper } = await bootCatalogue(undefined, fiche("terraria"));
    const port = wrapper.get("[data-fiche-port-row]").text();
    expect(port).toContain("7777");
    expect(port).toContain("TCP");
    expect(port).toContain("Port du jeu");
    expect(wrapper.findAll("[data-fiche-folder]").map((item) => item.text())).toEqual([
      expect.stringContaining("Le monde"),
      expect.stringContaining("La configuration"),
    ]);
    const questions = wrapper.findAll("[data-fiche-question]");
    expect(questions).toHaveLength(6);
    expect(questions.length).toBeLessThanOrEqual(8);
    const world = questions[0]?.text() ?? "";
    expect(world).toContain("Nom du monde");
    expect(world).toContain("La valeur proposée est Monde.");
    // Un choix se dit par son libellé, jamais par le numéro que la fiche garde (revue UX, constat 4).
    const choice = questions[4]?.text() ?? "";
    expect(choice).toMatch(/La valeur proposée est (Petit|Moyen|Grand)\./);
    expect(choice).not.toMatch(/La valeur proposée est \d/);
    expect(world).toContain("Ce réglage ne pourra plus être changé après la création du monde.");
    // Un secret n'a aucune valeur proposée, un nombre dit ses bornes, un choix dit sa liste.
    expect(questions[1]?.text()).toContain("La fiche ne propose aucune valeur.");
    expect(questions[3]?.text()).toContain("Le nombre est compris entre 1 et 255.");
    expect(questions[4]?.text()).toContain("Les choix sont : Petit, Moyen, Grand.");
    expect(wrapper.get("[data-imposed-count]").text()).toBe(
      "3 réglage(s) posé(s) par la fiche à votre place.",
    );
    wrapper.unmount();
  });

  it("dit la façon d'arrêter le jeu sans promettre une suite non essayée, et le premier démarrage long", async () => {
    const { wrapper } = await bootCatalogue(undefined, fiche("terraria"));
    const stop = wrapper.get("[data-fiche-stop]").text();
    expect(stop).toContain("Hearth ne peut pas encore lui demander de le faire");
    expect(stop).toContain("Le délai d'arrêt est de 10 secondes.");
    const downloads = wrapper.get("[data-fiche-downloads]").text();
    expect(downloads).toContain(
      "Cette fiche ne déclare aucun téléchargement au premier démarrage.",
    );
    expect(downloads).toContain(
      "Le premier démarrage peut prendre longtemps : Hearth attend jusqu'à 30 minutes.",
    );
    expect(downloads).toContain("le jeu génère aussi son monde");
    wrapper.unmount();
  });

  it("ouvre la liste de ce que la fiche impose, secrets exclus, avec la raison de chaque réglage", async () => {
    const { wrapper } = await bootCatalogue(undefined, fiche("terraria"));
    await wrapper.get("[data-imposed-open]").trigger("click");
    await settle();
    const dialog = document.querySelector("[data-imposed-dialog]");
    expect(dialog?.textContent).toContain("Ce que cette fiche impose");
    const rows = [...(dialog?.querySelectorAll("[data-imposed-row]") ?? [])].map(
      (row) => row.textContent ?? "",
    );
    expect(rows).toHaveLength(3);
    expect(rows[0]).toContain("Calculée par Hearth");
    expect(rows[1]).toContain("7777");
    expect(rows[1]).toContain("Le port du jeu dans le service");
    wrapper.unmount();
  });
});

describe("les ressources et leurs preuves (BR-FICHE-008, 021)", () => {
  it("dit une seule fois que Hearth ne connaît pas les besoins d'un jeu, sans tableau d'absences", async () => {
    const { wrapper } = await bootCatalogue(undefined, fiche("terraria"));
    expect(wrapper.get("[data-res-none]").text()).toBe(
      "Hearth ne connaît pas les besoins de ce jeu en processeur, en mémoire ni en espace disque.",
    );
    expect(wrapper.get("[data-res-server-has]").text()).toBe(
      "Votre serveur offre 4 cœurs et 16 Go de mémoire.",
    );
    // Ni tableau, ni « Non vérifié » répété, ni carte Espace disque qui le redit.
    expect(wrapper.find("[data-res-row]").exists()).toBe(false);
    expect(wrapper.get("[data-fiche-resources]").text()).not.toContain("Non vérifié");
    expect(wrapper.find("[data-fiche-disk]").exists()).toBe(false);
    // Le bouton reste actif : rien n'est bloqué par une preuve manquante.
    expect(wrapper.get("[data-fiche-install]").attributes("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });

  it("ne met aucun nombre sur une valeur non vérifiée quand une autre est connue, et le dit en mots", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      const detail = bridge.catalog.get("terraria");
      if (!detail) throw new Error("fiche absente");
      detail.resources.disk = {
        installBytes: 2 * GIB,
        evidence: { kind: "publisher", url: "https://example.org/disque" },
      };
      bridge.catalog.put(detail);
    }, fiche("terraria"));
    const memory = wrapper.get('[data-res-row="memory"]');
    expect(memory.get("[data-res-min]").text()).toBe("Non vérifié");
    expect(memory.get("[data-res-recommended]").text()).toBe("Non vérifié");
    expect(memory.get("[data-res-server]").text()).toBe("16 Go");
    expect(memory.get("[data-proof]").text()).toContain("Non vérifié");
    const verdicts = wrapper.get("[data-res-verdicts]").text();
    expect(verdicts).toContain(
      "Mémoire nécessaire : non vérifiée. Hearth ne peut pas dire si votre serveur est assez grand.",
    );
    expect(verdicts).toContain("Processeur nécessaire : non vérifié.");
    // Aucun verdict de couverture sur un chiffre qu'on n'a pas.
    expect(wrapper.find("[data-res-cover]").exists()).toBe(false);
    expect(wrapper.get("[data-fiche-disk]").text()).toContain("2 Go nécessaires");
    // Le bouton reste actif : rien n'est bloqué par une preuve manquante.
    expect(wrapper.get("[data-fiche-install]").attributes("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });

  it("montre chaque preuve de la fiche d'essai et dit si la machine couvre le recommandé", async () => {
    const { wrapper } = await bootCatalogue((bridge) => bridge.catalog.addRich(), fiche("essai"));
    expect(wrapper.get('[data-res-row="cpu"] [data-proof]').attributes("data-proof")).toBe(
      "measured",
    );
    expect(wrapper.get('[data-res-row="memory"] [data-proof]').attributes("data-proof")).toBe(
      "measured_idle",
    );
    expect(wrapper.get('[data-res-row="disk"] [data-proof]').attributes("data-proof")).toBe(
      "publisher",
    );
    expect(wrapper.get('[data-res-row="memory"] [data-res-min]').text()).toBe("2 Go");
    expect(wrapper.get('[data-res-row="memory"] [data-res-recommended]').text()).toBe("4 Go");
    expect(wrapper.find('[data-res-cover="memory-covers"]').exists()).toBe(true);
    expect(wrapper.get("[data-fiche-disk]").text()).toContain("8 Go nécessaires");
    // Le téléchargement du premier démarrage dit sa taille, sa source et sa preuve.
    const downloads = wrapper.get("[data-fiche-downloads]").text();
    expect(downloads).toContain("environ 6 Go de fichiers de jeu depuis les serveurs de l'éditeur");
    expect(downloads).toContain("Chiffre de l'éditeur");
    expect(downloads).toContain("Étapes que Hearth reconnaît");
    wrapper.unmount();
  });

  it("ouvre la source d'un chiffre de l'éditeur dans le navigateur, par la coquille et non par l'adresse", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => b.catalog.addRich(), fiche("essai"));
    await wrapper.get('[data-res-row="disk"] [data-proof-source]').trigger("click");
    await flushPromises();
    expect(bridge.openedLinks).toEqual([
      { serverId: "forge", ficheId: "essai", kind: "evidence", key: "disk" },
    ]);
    wrapper.unmount();
  });

  it("dit « trop petit » sur un minimum MESURÉ, avec sa preuve, et désactive « Installer » sans case", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      bridge.catalog.addRich();
      bridge.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 1 * GIB });
    }, fiche("essai"));
    const error = wrapper.get("[data-fiche-too-small]").text();
    expect(error).toContain(
      "Votre serveur est trop petit pour ce jeu : il faut au moins 2 Go de mémoire et il en a 1 Go.",
    );
    expect(error).toContain("la mémoire que le jeu prend au repos");
    for (const button of ["[data-fiche-install]", "[data-fiche-install-proposed]"]) {
      expect(wrapper.get(button).attributes("aria-disabled")).toBe("true");
    }
    expect(wrapper.get("[data-fiche-reason]").text()).toBe(
      "Votre serveur est trop petit pour ce jeu.",
    );
    wrapper.unmount();
  });

  it("avertit sur le chiffre de l'éditeur sans rien bloquer", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      const rich = bridge.catalog.addRich();
      rich.resources.memory = {
        minBytes: 32 * GIB,
        recommendedBytes: null,
        evidence: { kind: "publisher", url: "https://example.org/x" },
      };
      bridge.catalog.put(rich);
    }, fiche("essai"));
    expect(wrapper.get('[data-res-verdict="publisher_above"]').text()).toContain(
      "L'éditeur du jeu annonce au moins 32 Go de mémoire et votre serveur en a 16 Go. Hearth n'a pas mesuré ce chiffre.",
    );
    expect(wrapper.find("[data-fiche-too-small]").exists()).toBe(false);
    expect(wrapper.get("[data-fiche-install]").attributes("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });
});

describe("« Installer » (BR-FICHE-012, 033, 039)", () => {
  it("ouvre l'installation, ou l'installation avec les valeurs proposées", async () => {
    const { wrapper, router } = await bootCatalogue(undefined, fiche("terraria"));
    await wrapper.get("[data-fiche-install]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("fiche-install");
    expect(router.currentRoute.value.query).toEqual({});
    await router.push(fiche("terraria"));
    await settle();
    await wrapper.get("[data-fiche-install-proposed]").trigger("click");
    await settle();
    expect(router.currentRoute.value.query.proposees).toBe("1");
    wrapper.unmount();
  });

  it("reste visible et désactivé, avec la raison, pour un compte qui ne peut pas gérer les services", async () => {
    const { wrapper } = await bootCatalogue(
      (bridge) => bridge.setServiceRightOnServer("forge", "marie", false),
      fiche("terraria"),
    );
    expect(wrapper.get("[data-fiche-install]").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-fiche-rights]").text()).toContain("ne peut pas gérer les services");
    wrapper.unmount();
  });

  it("explique le moteur arrêté sous le bouton, et ne propose rien quand le moteur est absent", async () => {
    const stopped = await bootCatalogue(
      (bridge) => bridge.services.setEngine("forge", { state: "stopped" }),
      fiche("terraria"),
    );
    expect(stopped.wrapper.get("[data-fiche-install]").attributes("aria-disabled")).toBe("true");
    expect(stopped.wrapper.get("[data-fiche-reason]").text()).toContain("arrêté");
    stopped.wrapper.unmount();
    const absent = await bootCatalogue(
      (bridge) => bridge.services.setEngine("forge", { state: "absent" }),
      fiche("terraria"),
    );
    expect(absent.wrapper.find("[data-fiche-install]").exists()).toBe(false);
    absent.wrapper.unmount();
  });

  it("lit une fiche inconnue comme telle et ne propose pas d'installer une fiche que le client ne sait pas lire", async () => {
    const unknown = await bootCatalogue(undefined, fiche("nope"));
    expect(unknown.wrapper.get("[data-fiche-unknown]").text()).toContain(
      "Cette fiche n'est pas proposée par ce serveur.",
    );
    unknown.wrapper.unmount();
    const partial = await bootCatalogue((bridge) => {
      const rich = bridge.catalog.addRich();
      rich.unreadable = 1;
      bridge.catalog.put(rich);
    }, fiche("essai"));
    expect(partial.wrapper.find("[data-fiche-unreadable]").exists()).toBe(true);
    expect(partial.wrapper.get("[data-fiche-install]").attributes("aria-disabled")).toBe("true");
    partial.wrapper.unmount();
  });

  it("dit la phrase de confiance, jamais « isolé » ni « sûr »", async () => {
    const { wrapper } = await bootCatalogue(undefined, fiche("terraria"));
    const trust = wrapper.get("[data-fiche-trust]").text();
    expect(trust).toBe(
      "Cette fiche a été écrite par la personne qui publie Hearth. Elle fait tourner une image publique sur votre serveur, avec les droits d'un service ordinaire.",
    );
    expect(wrapper.get("[data-fiche-page]").text()).not.toMatch(
      /(?<![\p{L}])(isolé|isolée|sûr|sûre|protégé|sécurisé)(?![\p{L}])/iu,
    );
    expect(dialogText()).toBe("");
    wrapper.unmount();
  });
});

describe("le contrat de licence (BR-FICHE-058)", () => {
  it("se lit dans le navigateur de l'utilisateur, jamais dans Hearth", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => b.catalog.addRich(), fiche("essai"));
    const card = wrapper.get("[data-fiche-agreements]");
    expect(card.text()).toContain("Contrat de licence de l'éditeur");
    await card.get("[data-agreement-read]").trigger("click");
    await flushPromises();
    expect(bridge.openedLinks).toEqual([
      { serverId: "forge", ficheId: "essai", kind: "agreement", key: "licence" },
    ]);
    // Le texte du contrat n'est jamais affiché dans l'application.
    expect(wrapper.text()).not.toContain("example.org/licence");
    wrapper.unmount();
  });
});
