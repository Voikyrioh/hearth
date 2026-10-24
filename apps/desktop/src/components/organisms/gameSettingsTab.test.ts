import { flushPromises, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { typeInto } from "@/test/confirm";
import {
  boot,
  confirmTop,
  dialogText,
  settle,
  toasts,
  topButton,
  useDeployTestClock,
} from "@/test/deploy";

// L'onglet « Réglages » d'un service de jeu (HRT-109, BR-FICHE-037, 040 à 049, 051 à 054) sur le pont simulé, dont l'agent d'essai juge
// comme le vrai. Terraria a la forme réelle d'une fiche : 32 réglages, 7 thèmes. Les valeurs voulues, l'état du fichier et
// les refus viennent de l'agent ; les libellés, bornes et choix de la fiche.
useDeployTestClock();

const KEY = "h_game";
const SENTINEL = "SENTINELLE-mot-de-passe-9f3a";
const PATH = `/servers/forge/services?service=${KEY}&onglet=reglages`;

function setup(answers: { id: string; value: string }[] = [], rewrite?: string) {
  return (bridge: SimulatedLinkBridge) => {
    bridge.services.seed("forge", [
      { key: "h_db", name: "base", origin: "hearth", state: "running" },
      { key: KEY, name: "terraria", origin: "hearth", state: "running" },
    ]);
    const detail = bridge.catalog.get("terraria");
    if (!detail) throw new Error("Terraria est la fiche de départ");
    bridge.gameSettings.register("forge", KEY, detail, answers);
    if (rewrite !== undefined) bridge.gameSettings.rewriteFile("forge", KEY, rewrite);
  };
}

async function open(prepare = setup([{ id: "password", value: SENTINEL }])) {
  const ctx = await boot(prepare, PATH);
  await settle();
  return ctx;
}

const row = (wrapper: VueWrapper, id: string) => wrapper.get(`[data-setting="${id}"]`);
const input = (wrapper: VueWrapper, id: string) =>
  row(wrapper, id).get<HTMLInputElement>("input, textarea");

async function type(wrapper: VueWrapper, id: string, value: string) {
  typeInto(input(wrapper, id).element, value);
  await settle();
}

async function showAll(wrapper: VueWrapper) {
  const all = wrapper
    .get("[data-settings-scope]")
    .findAll("button, input, label")
    .find((item) => item.text() === "Tout afficher");
  await all?.trigger("click");
  // Un `radio` natif change par `change`.
  const radio = wrapper
    .get("[data-settings-scope]")
    .findAll<HTMLInputElement>("input")
    .find((item) => item.element.value === "all");
  if (radio) {
    radio.element.checked = true;
    await radio.trigger("change");
  }
  await settle();
}

describe("l'onglet", () => {
  it("n'existe que pour un service né d'une fiche, deuxième après « Détail »", async () => {
    const { wrapper } = await open();
    const tabs = wrapper.findAll("[role=tab]").map((tab) => tab.text());
    expect(tabs).toEqual(["Détail", "Réglages", "Journaux"]);
    wrapper.unmount();
  });

  it("est proposé dès que l'agent annonce les réglages de jeu, et dit « pas de réglages » d'un service ordinaire", async () => {
    const { wrapper } = await boot(setup(), "/servers/forge/services?service=h_db&onglet=reglages");
    await settle();
    // La liste ne dit pas qu'un service vient d'une fiche : la vue n'est lue qu'à l'ouverture de l'onglet.
    expect(wrapper.findAll("[role=tab]").map((tab) => tab.text())).toEqual([
      "Détail",
      "Réglages",
      "Journaux",
    ]);
    expect(wrapper.get("[data-settings-absent]").text()).toContain(
      "Ce service n'a pas de réglages de jeu.",
    );
    wrapper.unmount();
  });

  it("n'existe pas pour un agent qui n'annonce pas les réglages de jeu, ni pour un service adopté", async () => {
    const { wrapper } = await boot((bridge) => {
      setup()(bridge);
      bridge.accounts.setRightsKnown("forge", false);
    });
    await settle();
    expect(wrapper.findAll("[role=tab]").map((tab) => tab.text())).not.toContain("Réglages");
    wrapper.unmount();
    const adopted = await boot((bridge) => {
      bridge.services.seed("forge", [
        {
          key: "c_adopte",
          name: "ancien",
          origin: "hearth",
          adopted: { by: "marie", at: "2026-10-13T08:00:00Z" },
        },
      ]);
    }, "/servers/forge/services?service=c_adopte");
    await settle();
    expect(adopted.wrapper.findAll("[role=tab]").map((tab) => tab.text())).not.toContain(
      "Réglages",
    );
    adopted.wrapper.unmount();
  });
});

describe("les thèmes en mots simples (BR-FICHE-041)", () => {
  it("range les réglages courants par thème de la fiche, repliés ou ouverts selon leur contenu", async () => {
    const { wrapper } = await open();
    const titles = wrapper.findAll("[data-setting-theme] h2").map((title) => title.text());
    expect(titles).toEqual(["Ma partie", "Joueurs", "Monde", "Triche et sécurité"]);
    // Un thème qui contient un réglage essentiel est ouvert d'office.
    expect(wrapper.get('[data-theme="game"]').attributes("open")).toBeDefined();
    wrapper.unmount();
  });

  it("montre les 32 réglages de Terraria avec « Tout afficher », mode Voyage compris", async () => {
    const { wrapper } = await open();
    await showAll(wrapper);
    expect(wrapper.findAll("[data-setting-theme]")).toHaveLength(7);
    expect(wrapper.findAll("[data-setting]")).toHaveLength(32);
    expect(wrapper.get('[data-theme="journey"]').text()).toContain("15 réglage(s), 0 modifié(s)");
    wrapper.unmount();
  });

  it("dit pour chaque réglage sa phrase, son nom technique, son défaut et sa valeur proposée", async () => {
    const { wrapper } = await open();
    const players = row(wrapper, "max_players");
    expect(players.text()).toContain("Nombre de joueurs");
    expect(players.text()).toContain("Combien de joueurs peuvent être connectés");
    expect(players.get("[data-setting-key]").text()).toBe("Nom dans le jeu : maxplayers");
    const language = row(wrapper, "language");
    expect(language.get("[data-setting-default]").text()).toBe("Défaut du jeu : Anglais");
    // Défaut non vérifié : la ligne le dit, et Hearth n'écrit pas ce réglage.
    const seed = row(wrapper, "motd");
    expect(seed.find("[data-setting-default]").text()).toBe("Défaut du jeu : vide");
    wrapper.unmount();
  });

  it("dit l'une des quatre mentions d'effet", async () => {
    const { wrapper } = await open();
    expect(row(wrapper, "motd").get("[data-setting-effect]").text()).toBe(
      "Prend effet au prochain démarrage",
    );
    expect(row(wrapper, "world_name").get("[data-setting-effect]").text()).toContain("Figé");
    wrapper.unmount();
  });
});

describe("les réglages verrouillés ou secrets", () => {
  it("affiche un réglage figé sans le laisser changer, avec la phrase de la spec", async () => {
    const { wrapper } = await open();
    const world = row(wrapper, "world_name");
    expect(world.attributes("data-locked")).toBe("frozen");
    expect(world.get("[data-setting-effect]").text()).toBe(
      "Figé : ne peut plus changer après la création du monde. Il faudrait créer un autre monde.",
    );
    expect(input(wrapper, "world_name").attributes("disabled")).toBeDefined();
    wrapper.unmount();
  });

  it("n'offre aucun contrôle pour un réglage imposé et dit pourquoi", async () => {
    const { wrapper } = await open();
    await showAll(wrapper);
    const port = row(wrapper, "port");
    expect(port.attributes("data-locked")).toBe("imposed");
    expect(port.find("input").exists()).toBe(false);
    expect(port.get("[data-setting-lock]").text()).toContain("Imposé par la fiche :");
    expect(port.get("[data-setting-tag='imposed']").text()).toBe("Imposé par la fiche");
    wrapper.unmount();
  });

  it("dit un réglage reçu par argument en attente de la recréation, désactivé", async () => {
    const { wrapper } = await open();
    await showAll(wrapper);
    const box = row(wrapper, "announcement_box_off");
    expect(box.get("[data-setting-lock]").text()).toBe(
      "Ce réglage se change en recréant le service. Cela arrive bientôt.",
    );
    wrapper.unmount();
  });

  it("dit seulement « défini » d'un mot de passe, sans jamais l'avoir lu", async () => {
    const { wrapper } = await open();
    expect(row(wrapper, "password").get("[data-setting-secret]").text()).toContain(
      "Défini. Saisissez une nouvelle valeur pour le changer.",
    );
    expect(document.body.innerHTML).not.toContain(SENTINEL);
    // Aucun avertissement « mot de passe vide » : il est défini.
    expect(row(wrapper, "password").find("[data-question-open-game]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("chercher et filtrer (BR-FICHE-043, 045)", () => {
  it("trouve par libellé, par nom technique ou par phrase, tous niveaux confondus, et le dit", async () => {
    const { wrapper } = await open();
    const search = wrapper.get<HTMLInputElement>("[data-settings-search] input");
    typeInto(search.element, "FLUIDITE");
    await settle();
    expect(
      wrapper.findAll("[data-setting]").map((item) => item.attributes("data-setting")),
    ).toEqual(["npc_stream"]);
    expect(wrapper.get("[data-settings-found]").text()).toBe("1 réglage(s) trouvé(s).");
    typeInto(search.element, "zzz");
    await settle();
    expect(wrapper.get("[data-settings-none]").text()).toBe("Aucun réglage ne correspond.");
    wrapper.unmount();
  });
});

describe("enregistrer (BR-FICHE-042, 048, 053)", () => {
  it("écrit une valeur, dit l'effet, demande le mot de passe et marque « À appliquer »", async () => {
    const { wrapper, bridge } = await open();
    await type(wrapper, "motd", "Bienvenue chez nous");
    expect(row(wrapper, "motd").get("[data-setting-tag='modified']").text()).toBe("Modifié");
    expect(wrapper.get("[data-settings-count]").text()).toBe("1 changement(s) à enregistrer.");
    await wrapper.get("[data-settings-save]").trigger("click");
    await flushPromises();
    expect(dialogText()).toContain("Les changements s'appliqueront au prochain démarrage.");
    expect(dialogText()).toContain("Message d'accueil");
    await confirmTop("Enregistrer");
    await settle();
    expect(toasts()).toContain("Réglages enregistrés.");
    expect(bridge.gameSettings.fileText("forge", KEY)).toContain("motd=Bienvenue chez nous");
    // Le repère « À appliquer » demande le redémarrage, que seul le client lance.
    expect(wrapper.get("[data-settings-pending]").text()).toContain(
      "À appliquer au prochain démarrage",
    );
    expect(wrapper.get("[data-settings-restart]").text()).toBe("Redémarrer maintenant");
    wrapper.unmount();
  });

  it("n'envoie que ce qui change, avec la révision et l'empreinte lues, et le mot mot de passe jamais dans la vue", async () => {
    const { wrapper, bridge } = await open();
    await type(wrapper, "motd", "Salut");
    await wrapper.get("[data-settings-save]").trigger("click");
    await flushPromises();
    await confirmTop("Enregistrer");
    await settle();
    const sent = bridge.gameSettings.received.at(-1)?.request;
    expect(sent).toMatchObject({
      baseRev: 0,
      origin: "form",
      effect: "restart",
      changes: [{ id: "motd", action: "set", value: "Salut" }],
    });
    expect(sent?.fileStamp).toMatch(/^[0-9a-f]{64}$/);
    wrapper.unmount();
  });

  it("garde la saisie quand l'agent refuse une valeur et nomme le réglage par son libellé", async () => {
    const { wrapper } = await open();
    await type(wrapper, "max_players", "999");
    // Le défaut de forme est dit sous le champ, et l'enregistrement attend.
    expect(wrapper.get("[data-settings-count]").text()).toBe(
      "Corrigez les réglages signalés pour enregistrer.",
    );
    expect(wrapper.get("[data-settings-save]").attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("efface les changements à l'issue et ne garde aucune valeur secrète", async () => {
    const { wrapper } = await open();
    await type(wrapper, "password", "nouveau-secret-xyz");
    await wrapper.get("[data-settings-save]").trigger("click");
    await flushPromises();
    expect(dialogText()).toContain("Mot de passe du serveur (mot de passe modifié)");
    expect(dialogText()).not.toContain("nouveau-secret-xyz");
    await confirmTop("Enregistrer");
    await settle();
    expect(wrapper.get("[data-settings-count]").text()).toBe("Aucun changement à enregistrer.");
    expect(document.body.innerHTML).not.toContain("nouveau-secret-xyz");
    wrapper.unmount();
  });

  it("annule les changements d'un geste", async () => {
    const { wrapper } = await open();
    await type(wrapper, "motd", "Salut");
    await wrapper.get("[data-settings-discard]").trigger("click");
    await settle();
    expect(wrapper.get("[data-settings-count]").text()).toBe("Aucun changement à enregistrer.");
    expect(input(wrapper, "motd").element.value).toBe("");
    wrapper.unmount();
  });
});

describe("remettre aux valeurs proposées (BR-FICHE-044)", () => {
  it("remet un réglage, puis un thème avec sa confirmation, sans toucher figé, imposé ni secret", async () => {
    const { wrapper } = await open(
      setup([
        { id: "password", value: SENTINEL },
        { id: "max_players", value: "20" },
      ]),
    );
    expect(row(wrapper, "max_players").find("[data-setting-reset]").exists()).toBe(true);
    await row(wrapper, "max_players").get("[data-setting-reset]").trigger("click");
    await settle();
    expect(input(wrapper, "max_players").element.value).toBe("8");
    // Le thème « Joueurs » : la confirmation liste ce qui est concerné et dit ce qui n'est pas touché.
    await type(wrapper, "max_players", "30");
    await wrapper.get('[data-theme="players"] [data-theme-reset]').trigger("click");
    await settle();
    expect(dialogText()).toContain("Remettre ce thème aux valeurs proposées ?");
    expect(dialogText()).toContain("Les réglages figés, les réglages imposés et les mots de passe");
    expect(dialogText()).toContain("Nombre de joueurs : 30 devient 8");
    topButton("Remettre")?.click();
    await settle();
    expect(input(wrapper, "max_players").element.value).toBe("8");
    wrapper.unmount();
  });
});

describe("le fichier a changé (BR-FICHE-048)", () => {
  const rewritten = "# réécrit par le jeu\nmaxplayers=20\nmotd=Bonjour\nautre_ligne=1";

  it("dit l'écart, réglage par réglage, avec « Garder la valeur du fichier » par défaut", async () => {
    const { wrapper } = await open(setup([{ id: "max_players", value: "12" }], rewritten));
    const banner = wrapper.get("[data-file-changed]");
    expect(banner.text()).toContain(
      "Le fichier a changé depuis la dernière fois que Hearth l'a écrit",
    );
    const drift = wrapper.get('[data-drift="max_players"]');
    expect(drift.get("[data-drift-mine]").text()).toBe("12");
    expect(drift.get("[data-drift-file]").text()).toBe("20");
    expect(drift.text()).toContain("Garder la valeur du fichier");
    expect(drift.text()).toContain("Remettre la mienne");
    // Une ligne inconnue est montrée à part, jamais perdue.
    expect(wrapper.get("[data-others]").text()).toContain("autre_ligne");
    wrapper.unmount();
  });

  it("n'écrit rien sans réponse, puis enregistre le choix par défaut sans toucher aux octets du jeu", async () => {
    const { wrapper, bridge } = await open(setup([{ id: "max_players", value: "12" }], rewritten));
    await wrapper.get("[data-settings-save]").trigger("click");
    await flushPromises();
    await confirmTop("Enregistrer");
    await settle();
    const text = bridge.gameSettings.fileText("forge", KEY);
    expect(text).toContain("# réécrit par le jeu");
    expect(text).toContain("autre_ligne=1");
    expect(text).toContain("maxplayers=20");
    wrapper.unmount();
  });
});

describe("l'import d'un texte collé (BR-FICHE-052)", () => {
  async function openImport(wrapper: VueWrapper) {
    await wrapper.get("[data-settings-import]").trigger("click");
    await settle();
  }
  const text = () =>
    document.querySelector<HTMLTextAreaElement>("dialog [data-import-text] textarea");

  it("lit le texte, montre le tableau avec les états, puis l'applique avec le mot de passe", async () => {
    const { wrapper, bridge } = await open();
    await openImport(wrapper);
    expect(dialogText()).toContain("Le texte collé est envoyé au serveur pour être lu.");
    typeInto(text(), "motd=Importé\nmaxplayers=8\nlanguage=en/US\nport=1\nzzz=3");
    await settle();
    topButton("Lire ce texte")?.click();
    await settle();
    const table = document.querySelector("dialog [data-import-table]");
    expect(table?.textContent).toContain("Reconnu");
    expect(table?.textContent).toContain("Valeur que ce jeu ne connaît pas");
    expect(table?.textContent).toContain("Imposé par la fiche : ignoré");
    expect(document.querySelector("dialog [data-import-unknown]")?.textContent).toContain(
      "1 ligne(s) inconnue(s)",
    );
    expect(dialogText()).toContain("Ce texte retire ou change un réglage dont Hearth a besoin");
    await confirmTop("Appliquer ces réglages");
    await settle();
    expect(bridge.gameSettings.fileText("forge", KEY)).toContain("motd=Importé");
    expect(bridge.gameSettings.received.at(-1)?.request.origin).toBe("import");
    wrapper.unmount();
  });

  it("efface le texte collé à la fermeture", async () => {
    const { wrapper } = await open();
    await openImport(wrapper);
    typeInto(text(), "password=secret-colle-123");
    await settle();
    topButton("Annuler")?.click();
    await settle();
    await openImport(wrapper);
    expect(text()?.value).toBe("");
    expect(document.body.innerHTML).not.toContain("secret-colle-123");
    wrapper.unmount();
  });

  it("dit un texte illisible sans rien changer", async () => {
    const { wrapper } = await open();
    await openImport(wrapper);
    typeInto(text(), "motd=ok\nceci n'est pas une ligne");
    await settle();
    topButton("Lire ce texte")?.click();
    await settle();
    expect(document.querySelector("dialog [data-import-unreadable]")?.textContent).toContain(
      "(ligne 2)",
    );
    expect(topButton("Appliquer ces réglages")?.getAttribute("aria-disabled")).toBe("true");
    wrapper.unmount();
  });
});

describe("le fichier brut, la console et les autres réglages", () => {
  it("avertit avant le fichier brut, avec sa case « Je comprends »", async () => {
    const { wrapper } = await open();
    await wrapper.get("[data-settings-raw]").trigger("click");
    await settle();
    expect(dialogText()).toContain(
      "Vous modifiez le fichier tel que le jeu le lit. Hearth ne vérifie pas ce que vous écrivez",
    );
    expect(dialogText()).toContain("Je comprends");
    expect(dialogText()).toContain("D'ici là, importez des réglages en les collant.");
    wrapper.unmount();
  });

  it("nomme chaque fonction qui attend la console, présente, inactive, avec son texte", async () => {
    const { wrapper } = await open();
    const section = wrapper.get("[data-console-wait]");
    for (const id of ["apply", "who", "tell"]) {
      const item = section.get(`[data-console-function="${id}"]`);
      expect(item.text()).toContain(
        "Cette fonction attend la console de Hearth, qui n'est pas encore livrée.",
      );
      expect(item.get("button").attributes("aria-disabled")).toBe("true");
    }
    expect(section.text()).toContain("Enregistrez le réglage, puis redémarrez le service.");
    wrapper.unmount();
  });

  it("garde les lignes inconnues et masque celles dont le nom évoque un secret", async () => {
    const { wrapper } = await open(
      setup([], "motd=\nmod_inconnu=2\nmon_token_api=abc123\n# commentaire"),
    );
    const others = wrapper.get("[data-others]");
    expect(others.get('[data-other="mod_inconnu"]').text()).toContain(
      "Hearth ne connaît pas ce réglage.",
    );
    expect(others.get('[data-other="mon_token_api"]').get("[data-other-masked]").text()).toContain(
      "Valeur masquée",
    );
    expect(document.body.innerHTML).not.toContain("abc123");
    wrapper.unmount();
  });
});

describe("les bandeaux, un seul à la fois (design 16.2)", () => {
  it("met un réglage imposé défait avant tout, avec sa remise inactive et sa phrase", async () => {
    const { wrapper } = await open((bridge) => {
      setup([{ id: "max_players", value: "12" }], "maxplayers=20")(bridge);
      bridge.gameSettings.breakImposed("forge", KEY, ["upnp"]);
    });
    const banner = wrapper.get("[data-imposed-broken]");
    expect(banner.text()).toContain("L'arrêt propre de ce jeu n'est plus garanti");
    expect(banner.text()).toContain("Ouverture automatique des ports (UPnP)");
    expect(banner.get("[data-imposed-broken-fix]").attributes("aria-disabled")).toBe("true");
    // Le bandeau du fichier réécrit passe après : un seul à la fois.
    expect(wrapper.find("[data-file-changed]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("dit que le fichier a disparu", async () => {
    const { wrapper } = await open((bridge) => {
      setup()(bridge);
      bridge.gameSettings.deleteFile("forge", KEY);
    });
    expect(wrapper.get("[data-settings-missing]").text()).toContain("Hearth le recréera");
    wrapper.unmount();
  });
});
