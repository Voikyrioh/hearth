import { flushPromises, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { useServicesStore } from "@/stores/services";
import { dialogButton, typeInto } from "@/test/confirm";
import {
  answer,
  bootCatalogue,
  confirmTop,
  dialogText,
  goNext,
  installPath,
  next,
  questionInput,
  reasonText,
  setName,
  settle,
  toasts,
  topButton,
  useFicheTestClock,
} from "@/test/fiche";

// Installer un serveur de jeu depuis une fiche (HRT-102, BR-FICHE-012 à 036) : les trois étapes, de la fiche à l'issue, sur
// le pont simulé dont l'agent juge COMME LE VRAI. Terraria a la forme réelle d'une fiche ; la fiche d'essai porte tout ce
// qu'une fiche peut offrir (variantes, contrat, jeton, ressources mesurées). Aucune règle de jeu n'est écrite dans le client.
useFicheTestClock();

const GIB = 1024 ** 3;
const SECRET = "SENTINEL-motdepasse";

const rich = (bridge: SimulatedLinkBridge) => bridge.catalog.addRich();

/** La fenêtre ouverte, en texte. */
const dialog = () => document.querySelector("dialog[open]");

/** Remplit ce qui manque à la fiche d'essai (le jeton est obligatoire), puis passe à l'étape « Ressources ». */
async function fillRich(wrapper: VueWrapper) {
  await answer(wrapper, "server_token", "jeton-du-serveur");
}

async function openSummary(wrapper: VueWrapper) {
  await goNext(wrapper);
  await goNext(wrapper);
}

const running = (bridge: SimulatedLinkBridge) => {
  const found = bridge.deploys.running("forge")[0];
  if (!found) throw new Error("aucune installation en cours");
  return found;
};

describe("la page (BR-FICHE-014, 015)", () => {
  it("s'ouvre sur « Réponses » avec le nom proposé par la fiche et les valeurs que la fiche propose", async () => {
    const { wrapper, router } = await bootCatalogue(undefined, installPath("terraria"));
    expect(router.currentRoute.value.name).toBe("fiche-install");
    expect(wrapper.get("[data-install-title]").text()).toBe("Installer Terraria");
    expect(wrapper.get(".nav__item.router-link-active").text()).toBe("Catalogue");
    const steps = wrapper.findAll(".trail__step").map((item) => item.text());
    expect(steps[0]).toContain("Réponses");
    expect(steps[1]).toContain("Ressources");
    expect(steps[2]).toContain("Vérifiez avant de lancer");
    expect(wrapper.get<HTMLInputElement>('[data-field="name"] input').element.value).toBe(
      "terraria",
    );
    expect(questionInput(wrapper, "world_name").element.value).toBe("Monde");
    expect(questionInput(wrapper, "max_players").element.value).toBe("8");
    expect(questionInput(wrapper, "password").element.value).toBe("");
    expect(wrapper.get("[data-install-version]").text()).toBe(
      "Version : vanilla-1.4.5.8 (la seule proposée par cette fiche).",
    );
    // Aucun champ libre : ni image, ni variable, ni commande, ni dossier.
    expect(wrapper.find('[data-field="image"]').exists()).toBe(false);
    expect(wrapper.find("[data-env-add]").exists()).toBe(false);
    expect(wrapper.find('[data-field="command"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it("propose terraria-2 quand terraria est pris", async () => {
    const { wrapper } = await bootCatalogue(
      (bridge) =>
        bridge.services.seed("forge", [
          { key: "h_t", name: "terraria", origin: "hearth", state: "running" },
        ]),
      installPath("terraria"),
    );
    expect(wrapper.get<HTMLInputElement>('[data-field="name"] input').element.value).toBe(
      "terraria-2",
    );
    wrapper.unmount();
  });

  it("rend chaque question dans le champ de son genre, avec ses bornes, ses choix et sa mention de figé", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    // Le mot de passe est masqué, le nombre est un champ numérique sans molette, les choix sont des boutons exclusifs.
    expect(questionInput(wrapper, "password").attributes("type")).toBe("password");
    expect(questionInput(wrapper, "max_players").attributes("inputmode")).toBe("numeric");
    const size = wrapper.get('[data-question="world_size"]');
    expect(size.findAll('[role="radio"]').map((item) => item.text())).toEqual([
      "Petit",
      "Moyen",
      "Grand",
    ]);
    expect(size.get('[aria-checked="true"]').text()).toBe("Grand");
    const frozen = wrapper.findAll("[data-question-frozen]").map((item) => item.text());
    expect(frozen).toHaveLength(3);
    expect(frozen[0]).toContain(
      "Ce réglage ne pourra plus être changé après la création du monde.",
    );
    expect(wrapper.get('[data-question="world_name"]').text()).toContain(
      "Lettres, chiffres, tirets et soulignés seulement.",
    );
    expect(wrapper.get('[data-question="max_players"]').text()).toContain(
      "Le nombre est compris entre 1 et 255.",
    );
    wrapper.unmount();
  });

  it("avertit d'un mot de passe laissé vide sans bloquer", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    expect(wrapper.get("[data-question-open-game]").text()).toBe(
      "Aucun mot de passe : toute personne qui peut joindre ce port peut rejoindre la partie.",
    );
    expect(next(wrapper).attributes("aria-disabled")).not.toBe("true");
    await answer(wrapper, "password", SECRET);
    expect(wrapper.find("[data-question-open-game]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("renvoie vers la fiche un compte qui ne peut pas gérer les services", async () => {
    const { wrapper, router } = await bootCatalogue(
      (bridge) => bridge.setServiceRightOnServer("forge", "marie", false),
      installPath("terraria"),
    );
    expect(router.currentRoute.value.name).toBe("fiche");
    expect(wrapper.find("[data-install-page]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("les défauts de forme, jugés sur la fiche", () => {
  it("ne montre un défaut qu'une fois le champ quitté, cite UN caractère, et garde « Continuer » inactif", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "world_name", "Mon monde");
    expect(wrapper.get('[data-question="world_name"]').text()).not.toContain("ne sait pas lire");
    await questionInput(wrapper, "world_name").trigger("blur");
    expect(wrapper.get('[data-question="world_name"]').text()).toContain(
      "Ce texte contient un caractère que ce jeu ne sait pas lire :  ",
    );
    expect(next(wrapper).attributes("aria-disabled")).toBe("true");
    expect(reasonText(wrapper)).toBe("Corrigez les champs signalés pour continuer.");
    await answer(wrapper, "world_name", "Monde_2");
    expect(next(wrapper).attributes("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });

  it("juge un nombre contre les bornes de la fiche et un nom de service contre les règles du déploiement", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "max_players", "300");
    await questionInput(wrapper, "max_players").trigger("blur");
    expect(wrapper.get('[data-question="max_players"]').text()).toContain(
      "Le nombre doit être compris entre 1 et 255.",
    );
    await answer(wrapper, "max_players", "16");
    await setName(wrapper, "mon terraria");
    await wrapper.get('[data-field="name"] input').trigger("blur");
    expect(wrapper.get('[data-field="name"]').text()).toContain(
      "lettres, chiffres, tirets et points",
    );
    expect(next(wrapper).attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("dit en direct qu'un nom est pris, sans jamais dire par qui", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    useServicesStore();
    await setName(wrapper, "base");
    expect(wrapper.get('[data-field="name"]').text()).toContain("Ce nom est déjà pris");
    expect(next(wrapper).attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("vérifie le port auprès de l'agent et propose le port libre le plus proche", async () => {
    const { wrapper } = await bootCatalogue(
      (bridge) => bridge.deploys.listen("forge", 7777, "sshd"),
      installPath("terraria"),
    );
    await settle();
    expect(wrapper.get("[data-fiche-port]").text()).toContain("occupé sur le serveur par « sshd »");
    expect(wrapper.get("[data-port-suggested]").text()).toContain(
      "Le port libre le plus proche est le 7778.",
    );
    await wrapper.get("[data-port-use]").trigger("click");
    await settle();
    expect(wrapper.get<HTMLInputElement>("[data-fiche-port] input").element.value).toBe("7778");
    expect(wrapper.get("[data-port-changed]").text()).toBe(
      "Ce port est celui que vos joueurs devront utiliser.",
    );
    wrapper.unmount();
  });

  it("refuse un port sous 1024 sans interroger l'agent", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    const before = bridge.deploys.checks.length;
    typeInto(wrapper.get<HTMLInputElement>("[data-fiche-port] input").element, "80");
    await settle();
    await wrapper.get("[data-fiche-port] input").trigger("blur");
    expect(wrapper.get("[data-fiche-port]").text()).toContain("compris entre 1024 et 65535");
    expect(
      bridge.deploys.checks.slice(before).every((check) => check.input.ports.length === 0),
    ).toBe(true);
    wrapper.unmount();
  });
});

describe("l'étape « Ressources » (BR-FICHE-020 à 023, BR-RES-037)", () => {
  it("place la mémoire et le processeur sans chiffre de fiche, et le dit", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await goNext(wrapper);
    expect(wrapper.find("[data-install-resources]").exists()).toBe(true);
    expect(wrapper.get("[data-memory-note]").text()).toContain(
      "Hearth n'a pas de chiffre vérifié pour ce jeu",
    );
    expect(wrapper.get("[data-cpu-note]").text()).toBe(
      "Hearth ne sait pas combien de processeur ce jeu demande.",
    );
    expect(wrapper.get("[data-resources-notice]").text()).toContain(
      "Hearth ne sait pas combien ce jeu en demande.",
    );
    // Tous les processeurs du serveur, et aucune position « sans limite ».
    expect(wrapper.text()).not.toMatch(/sans limite/i);
    expect(wrapper.find("[data-install-back]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("place les curseurs sur ce que la fiche recommande, avec sa preuve et son minimum", async () => {
    const { wrapper } = await bootCatalogue(rich, installPath("essai"));
    await fillRich(wrapper);
    await goNext(wrapper);
    const resources = wrapper.get("[data-install-resources]").text();
    expect(resources).toContain("Recommandé : 4 Go (mesuré au repos).");
    expect(resources).toContain("Minimum de la fiche : 2 Go (mesuré au repos).");
    expect(resources).toContain("Processeur recommandé : 2 cœurs (mesuré).");
    expect(wrapper.find("[data-resources-notice]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("ramène une recommandation au-dessus de la machine à sa capacité et le dit", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      rich(bridge);
      bridge.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 3 * GIB });
    }, installPath("essai"));
    await fillRich(wrapper);
    await goNext(wrapper);
    expect(wrapper.get("[data-resources-clamped]").text()).toBe(
      "Recommandé : 4 Go, ramené à la capacité de votre serveur.",
    );
    wrapper.unmount();
  });

  it("dit en chiffres ce qui reste de mémoire quand la machine est assez grande mais déjà engagée", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      rich(bridge);
      bridge.deploys.setCapacity("forge", { cpus: 8, memoryBytes: 16 * GIB }, 12 * GIB);
    }, installPath("essai"));
    await fillRich(wrapper);
    await goNext(wrapper);
    expect(wrapper.get('[data-resources-notice="remaining"]').text()).toBe(
      "Il reste 3 Go de mémoire disponible sur votre serveur (4 Go recommandés pour ce jeu).",
    );
    expect(wrapper.get('[data-resources-notice="over_committed"]').text()).toContain(
      "il en reste 3 Go",
    );
    wrapper.unmount();
  });

  it("refuse sans case un besoin d'espace mesuré qui dépasse la place, et avertit sur le chiffre de l'éditeur", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => {
      rich(b);
      b.deploys.setDisk("forge", 2 * GIB);
    }, installPath("essai"));
    await fillRich(wrapper);
    await goNext(wrapper);
    expect(wrapper.get("[data-resources-disk]").text()).toContain(
      "L'éditeur annonce environ 8 Go d'espace pour installer ce jeu et il en reste 2 Go.",
    );
    expect(next(wrapper).attributes("aria-disabled")).not.toBe("true");
    const measured = bridge.catalog.get("essai");
    if (measured) measured.resources.disk.evidence = { kind: "measured" };
    await wrapper.get("[data-install-back]").trigger("click");
    await bridge.readFiche("forge", "essai");
    wrapper.unmount();
  });

  it("revient à l'étape « Réponses » sans rien perdre", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "motd", "Bonjour");
    await goNext(wrapper);
    await wrapper.get("[data-install-back]").trigger("click");
    await settle();
    expect(questionInput(wrapper, "motd").element.value).toBe("Bonjour");
    wrapper.unmount();
  });
});

describe("la machine trop petite (BR-FICHE-021)", () => {
  it("le dit avant toute saisie et désactive « Continuer » sans case pour passer outre", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      rich(bridge);
      bridge.deploys.setCapacity("forge", { cpus: 4, memoryBytes: 1 * GIB });
    }, installPath("essai"));
    expect(wrapper.get("[data-install-too-small]").text()).toContain(
      "Votre serveur est trop petit pour ce jeu : il faut au moins 2 Go de mémoire et il en a 1 Go.",
    );
    await fillRich(wrapper);
    expect(next(wrapper).attributes("aria-disabled")).toBe("true");
    expect(reasonText(wrapper)).toBe("Votre serveur est trop petit pour ce jeu.");
    wrapper.unmount();
  });
});

describe("« Vérifiez avant de lancer » (BR-FICHE-025, 026, 036)", () => {
  it("montre tout ce qui part, avec les secrets masqués, et la phrase de confiance", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "password", SECRET);
    await answer(wrapper, "max_players", "16");
    await openSummary(wrapper);
    const text = dialogText();
    expect(text).toContain("Vérifiez avant de lancer");
    expect(document.querySelector("[data-sum-fiche]")?.textContent).toContain("Terraria");
    expect(document.querySelector("[data-sum-fiche]")?.textContent).toMatch(/Livrée avec Hearth/);
    expect(document.querySelector("[data-sum-name]")?.textContent).toBe("terraria");
    expect(document.querySelector("[data-sum-image]")?.textContent).toContain(
      "ryshe/terraria:vanilla-1.4.5.8",
    );
    expect(document.querySelector("[data-sum-ports]")?.textContent).toContain(
      "7777 vers 7777 (TCP)",
    );
    expect(document.querySelector("[data-sum-exposed]")?.textContent).toBe(
      "Ce port sera accessible à tout appareil qui peut joindre votre serveur.",
    );
    expect(document.querySelector("[data-sum-folders]")?.textContent).toContain(
      "/root/.local/share/Terraria/Worlds",
    );
    // Les réponses : le mot de passe masqué, jamais en clair ; ce qui diffère est compté.
    const answers = document.querySelector("[data-sum-answers]")?.textContent ?? "";
    expect(answers).toContain("Mot de passe du serveur");
    expect(answers).toContain("••••••••");
    expect(answers).toContain("16");
    expect(answers).toContain("2 réglage(s) différent(s) des valeurs proposées.");
    expect(text).not.toContain(SECRET);
    expect(document.querySelector("[data-sum-resources]")?.textContent).toMatch(
      /de mémoire, .* de processeur/,
    );
    expect(document.querySelector("[data-sum-restart]")?.textContent).toBeTruthy();
    expect(document.querySelector("[data-sum-stop]")?.textContent).toContain(
      "Le délai d'arrêt est de 10 secondes.",
    );
    expect(document.querySelector("[data-sum-imposed]")?.textContent).toContain(
      "3 réglage(s) posé(s)",
    );
    expect(document.querySelector("[data-sum-trust]")?.textContent).toContain(
      "Elle fait tourner une image publique sur votre serveur, avec les droits d'un service ordinaire.",
    );
    expect(text).not.toMatch(/(?<![\p{L}])(isolé|isolée|sûr|sûre|protégé|sécurisé)(?![\p{L}])/iu);
    wrapper.unmount();
  });

  it("ouvre ce que la fiche impose depuis la fenêtre", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await openSummary(wrapper);
    topButton("Voir ce que la fiche impose")?.click();
    await settle();
    expect(document.querySelector("[data-imposed-dialog]")?.textContent).toContain(
      "Le port du jeu dans le service",
    );
    wrapper.unmount();
  });

  it("retourne à la page d'où l'on vient sans rien perdre", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "motd", "Bonjour");
    await openSummary(wrapper);
    expect(dialog()).not.toBeNull();
    dialogButton("Retour")?.click();
    await settle();
    expect(dialog()).toBeNull();
    expect(wrapper.find("[data-install-resources]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("demande une case par point d'attention et garde « Installer » inactif tant qu'un point n'est pas validé", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      bridge.services.seed("forge", [
        {
          key: "h_old",
          name: "vieux",
          origin: "hearth",
          state: "stopped",
          ports: [{ hostPort: 7777, containerPort: 7777, protocol: "tcp", scope: "all" }],
        },
      ]);
    }, installPath("terraria"));
    await openSummary(wrapper);
    const attention = document.querySelector("[data-sum-attention]");
    expect(attention?.querySelector('[data-attention="reserved_ports"]')).not.toBeNull();
    expect(document.querySelector("[data-sum-need]")?.textContent).toBe(
      "Validez les points d'attention pour continuer.",
    );
    const submit = topButton("Installer le serveur de jeu");
    expect(submit?.disabled || submit?.getAttribute("aria-disabled") === "true").toBe(true);
    wrapper.unmount();
  });
});

describe("lancer, suivre, terminer (BR-FICHE-013, 027, 034)", () => {
  it("installe avec le mot de passe de confirmation à chaque fois, suit l'opération et efface la saisie à l'issue", async () => {
    const { wrapper, bridge, router } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "password", SECRET);
    await setName(wrapper, "mon-terraria");
    await openSummary(wrapper);
    await confirmTop("Installer le serveur de jeu");
    await settle();
    // L'agent a reçu la demande : la fiche lue, ses réponses, aucun champ libre.
    const received = bridge.catalog.received[0]?.request;
    expect(received).toMatchObject({
      name: "mon-terraria",
      fiche: "terraria",
      version: 2,
      variant: null,
      agreements: [],
      ports: [{ id: "game", hostPort: 7777 }],
    });
    expect(received?.answers).toContainEqual({ id: "password", value: SECRET });
    expect(received?.answers).toContainEqual({ id: "max_players", value: "8" });
    expect(Object.keys(received ?? {})).not.toEqual(
      expect.arrayContaining(["image", "env", "command", "mounts"]),
    );
    // La fenêtre de progression de l'installation.
    expect(dialogText()).toContain("Installation de mon-terraria");
    const op = running(bridge);
    bridge.deploys.progress("forge", op.operationId, 40, 400, 1000);
    await settle();
    expect(dialogText()).toContain("Récupération");
    bridge.deploys.toStep("forge", op.operationId, "start");
    await settle();
    bridge.deploys.succeed("forge", op.operationId);
    await settle();
    expect(toasts()).toContain("Le service mon-terraria est en marche.");
    expect(router.currentRoute.value.name).toBe("services");
    expect(router.currentRoute.value.query.service).toBe(op.key);
    // Rien de la saisie ne reste en mémoire : ni la page ni le magasin.
    expect(JSON.stringify(document.body.innerHTML)).not.toContain(SECRET);
    wrapper.unmount();
  });

  it("annule l'installation pendant la récupération de l'image : rien n'est créé", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await openSummary(wrapper);
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const op = running(bridge);
    bridge.deploys.progress("forge", op.operationId, 10);
    await settle();
    document.querySelector<HTMLElement>("[data-deploy-cancel]")?.click();
    await settle();
    await confirmTop("Annuler le déploiement");
    await vi.advanceTimersByTimeAsync(10);
    await settle();
    expect(bridge.deploys.cancels).toEqual([{ serverId: "forge", opId: op.operationId }]);
    expect(toasts()).toContain("Le déploiement a été annulé et rien n'a été créé.");
    // Rien n'a été créé, et la page garde la saisie.
    expect(bridge.services.find("forge", op.key)).toBeUndefined();
    expect(wrapper.find("[data-install-page]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("rouvre la page remplie quand l'installation échoue avant le lancement", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "motd", "Bonjour");
    await openSummary(wrapper);
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const op = running(bridge);
    bridge.deploys.fail("forge", op.operationId, "image_not_found");
    await settle();
    expect(dialogText()).toContain("image");
    topButton("Modifier")?.click();
    await settle();
    expect(questionInput(wrapper, "motd").element.value).toBe("Bonjour");
    wrapper.unmount();
  });

  it("n'accepte jamais sans le mot de passe : un mot de passe faux ne lance rien", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await openSummary(wrapper);
    await confirmTop("Installer le serveur de jeu", "faux");
    await settle();
    expect(bridge.catalog.received).toHaveLength(0);
    expect(dialogText()).toContain("Vérifiez avant de lancer");
    wrapper.unmount();
  });
});

describe("les refus de l'agent ramènent à la page, la saisie reste (BR-FICHE-032, 035)", () => {
  it("nomme la question qu'il refuse alors que le client la jugeait bonne, et met le curseur dessus", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    // L'agent est plus strict que la page n'a su le lire : le client ne le devine pas, l'agent le dit.
    const motd = bridge.catalog.get("terraria")?.questions.find((item) => item.id === "motd");
    if (motd) motd.maxLen = 3;
    await answer(wrapper, "motd", "trop long");
    await openSummary(wrapper);
    await confirmTop("Installer le serveur de jeu");
    await settle();
    expect(dialog()).toBeNull();
    const refusal = wrapper.get("[data-install-refusal]").text();
    expect(refusal).toContain("La réponse « Message d'accueil » n'est pas valable.");
    expect(refusal).toContain("La longueur de ce texte n'est pas permise.");
    expect(document.activeElement).toBe(questionInput(wrapper, "motd").element);
    expect(questionInput(wrapper, "motd").element.value).toBe("trop long");
    wrapper.unmount();
  });

  it("dit que la fiche a changé et garde la saisie", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "motd", "Bonjour");
    await openSummary(wrapper);
    bridge.catalog.bump("terraria");
    await confirmTop("Installer le serveur de jeu");
    await settle();
    expect(wrapper.get("[data-install-refusal]").text()).toContain(
      "Cette fiche a changé depuis son affichage.",
    );
    expect(questionInput(wrapper, "motd").element.value).toBe("Bonjour");
    expect(bridge.catalog.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("dit qu'un port vient d'être pris au dernier moment et propose le port libre", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await openSummary(wrapper);
    bridge.deploys.listen("forge", 7777, "sshd");
    await confirmTop("Installer le serveur de jeu");
    await settle();
    expect(dialog()).toBeNull();
    expect(wrapper.get("[data-install-refusal]").text()).toContain("7777");
    wrapper.unmount();
  });

  it("dit le droit retiré au lancement et garde la saisie", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "motd", "Bonjour");
    await openSummary(wrapper);
    bridge.setServiceRightOnServer("forge", "marie", false);
    await confirmTop("Installer le serveur de jeu");
    await settle();
    await flushPromises();
    expect(questionInput(wrapper, "motd").element.value).toBe("Bonjour");
    expect(bridge.catalog.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("garde la demande quand le lien tombe avant la réponse : résultat inconnu, jamais rejoué", async () => {
    const { wrapper, bridge } = await bootCatalogue(undefined, installPath("terraria"));
    await openSummary(wrapper);
    bridge.actionMode = "cut";
    await confirmTop("Installer le serveur de jeu");
    await settle();
    expect(dialogText()).toContain("Le résultat de cette action n'est pas connu.");
    expect(bridge.calls.filter((call) => call === "fiche install")).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("la variante (BR-FICHE-060)", () => {
  it("propose la variante par défaut, montre ses questions et pas celles d'une autre, et dit qu'elle ne se change plus", async () => {
    const { wrapper } = await bootCatalogue(rich, installPath("essai"));
    const cards = wrapper.findAll("[data-variant-card]");
    expect(cards.map((card) => card.get(".variant__name").text())).toEqual([
      "Sans mod",
      "Avec mods",
    ]);
    expect(wrapper.get("[data-variant-frozen]").text()).toBe(
      "Ne pourra plus être changée : il faudrait installer un autre serveur.",
    );
    expect(wrapper.find('[data-question="mod_pack"]').exists()).toBe(false);
    await cards[1]?.get("input").setValue(true);
    await settle();
    expect(wrapper.find('[data-question="mod_pack"]').exists()).toBe(true);
    await cards[0]?.get("input").setValue(true);
    await settle();
    expect(wrapper.find('[data-question="mod_pack"]').exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("le jeton de serveur (BR-FICHE-059)", () => {
  it("est un champ masqué, dit où le créer sans ouvrir l'adresse de lui-même, et bloque quand il manque", async () => {
    const { wrapper, bridge } = await bootCatalogue(rich, installPath("essai"));
    expect(questionInput(wrapper, "server_token").attributes("type")).toBe("password");
    expect(wrapper.get("[data-question-token-url]").text()).toBe("https://example.org/jetons");
    expect(wrapper.get("[data-question-token-needed]").text()).toBe(
      "Ce jeu a besoin d'un jeton de serveur.",
    );
    expect(bridge.openedLinks).toEqual([]);
    await questionInput(wrapper, "server_token").trigger("blur");
    expect(next(wrapper).attributes("aria-disabled")).toBe("true");
    await wrapper.get("[data-question-token-open]").trigger("click");
    await flushPromises();
    expect(bridge.openedLinks).toEqual([
      { serverId: "forge", ficheId: "essai", kind: "token", key: "server_token" },
    ]);
    await fillRich(wrapper);
    expect(next(wrapper).attributes("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });
});

describe("le contrat de licence (BR-FICHE-026, 058)", () => {
  async function toSummary(wrapper: VueWrapper) {
    await fillRich(wrapper);
    await openSummary(wrapper);
  }

  it("ouvre sa fenêtre à « Installer », avant le mot de passe, et ne s'accepte jamais d'avance", async () => {
    const { wrapper, bridge } = await bootCatalogue(rich, installPath("essai"));
    await toSummary(wrapper);
    // Aucune case de contrat dans la fenêtre : il a sa propre fenêtre.
    expect(document.querySelector("[data-agreement-dialog]")).toBeNull();
    const field = document.querySelector<HTMLInputElement>("dialog [data-reauth-field] input");
    typeInto(field, "Correct-Horse-9");
    await flushPromises();
    topButton("Installer le serveur de jeu")?.click();
    await flushPromises();
    const agreement = document.querySelector("[data-agreement-dialog]");
    expect(agreement?.textContent).toContain("Contrat de licence de l'éditeur");
    expect(agreement?.textContent).toContain(
      "L'éditeur exige que vous acceptiez son contrat avant d'installer son serveur.",
    );
    expect(agreement?.textContent).toContain("Lisez le contrat avant de répondre.");
    // Le focus est sur « Lire le contrat » ; « J'accepte » et « Je refuse » sont deux boutons secondaires de même poids.
    expect(agreement?.querySelector("[data-agreement-read]")?.hasAttribute("autofocus")).toBe(true);
    for (const id of ["refuse", "accept"]) {
      const button = agreement?.querySelector(`[data-agreement-${id}]`);
      expect(button?.className).toContain("btn--secondary");
      expect(button?.hasAttribute("autofocus")).toBe(false);
    }
    expect(bridge.catalog.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("« Je refuse » n'envoie rien et laisse la fenêtre ouverte avec la saisie", async () => {
    const { wrapper, bridge } = await bootCatalogue(rich, installPath("essai"));
    await toSummary(wrapper);
    typeInto(
      document.querySelector<HTMLInputElement>("dialog [data-reauth-field] input"),
      "Correct-Horse-9",
    );
    await flushPromises();
    topButton("Installer le serveur de jeu")?.click();
    await flushPromises();
    (document.querySelector("[data-agreement-refuse]") as HTMLElement).click();
    await settle();
    expect(document.querySelector("[data-agreement-dialog]")).toBeNull();
    expect(bridge.catalog.received).toHaveLength(0);
    expect(dialogText()).toContain("Vérifiez avant de lancer");
    wrapper.unmount();
  });

  it("« Lire le contrat » ouvre le navigateur par la coquille ; « J'accepte » envoie l'identifiant du contrat", async () => {
    const { wrapper, bridge } = await bootCatalogue(rich, installPath("essai"));
    await toSummary(wrapper);
    typeInto(
      document.querySelector<HTMLInputElement>("dialog [data-reauth-field] input"),
      "Correct-Horse-9",
    );
    await flushPromises();
    topButton("Installer le serveur de jeu")?.click();
    await flushPromises();
    (
      document.querySelector("[data-agreement-dialog] [data-agreement-read]") as HTMLElement
    ).click();
    await flushPromises();
    expect(bridge.openedLinks).toEqual([
      { serverId: "forge", ficheId: "essai", kind: "agreement", key: "licence" },
    ]);
    (document.querySelector("[data-agreement-accept]") as HTMLElement).click();
    await settle();
    expect(bridge.catalog.received[0]?.request.agreements).toEqual(["licence"]);
    expect(bridge.catalog.received[0]?.request.variant).toBe("vanilla");
    wrapper.unmount();
  });

  it("n'accepte rien à la place de l'utilisateur après « Installer avec les valeurs proposées »", async () => {
    const { wrapper, bridge } = await bootCatalogue(
      (b) => {
        rich(b);
      },
      installPath("essai", "?proposees=1"),
    );
    // Le jeton manque : la page reste sur « Réponses » et le dit, aucune fenêtre ne s'ouvre.
    await settle();
    expect(dialog()).toBeNull();
    expect(bridge.catalog.received).toHaveLength(0);
    await fillRich(wrapper);
    wrapper.unmount();
  });
});

describe("« Installer avec les valeurs proposées » (BR-FICHE-039)", () => {
  it("ouvre aussitôt la fenêtre « Vérifiez avant de lancer » quand tout est valable", async () => {
    const { wrapper } = await bootCatalogue(undefined, installPath("terraria", "?proposees=1"));
    await settle();
    expect(dialogText()).toContain("Vérifiez avant de lancer");
    expect(document.querySelector("[data-sum-name]")?.textContent).toBe("terraria");
    wrapper.unmount();
  });
});

describe("fermer et abandonner (BR-FICHE-034, BR-DEPLOY-029)", () => {
  it("ferme sans question quand rien n'est saisi", async () => {
    const { wrapper, router } = await bootCatalogue(undefined, installPath("terraria"));
    await wrapper.get("[data-install-close]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("fiche");
    wrapper.unmount();
  });

  it("demande « Abandonnez-vous votre saisie ? » quand une réponse est saisie, puis l'efface", async () => {
    const { wrapper, router } = await bootCatalogue(undefined, installPath("terraria"));
    await answer(wrapper, "password", SECRET);
    await wrapper.get("[data-install-close]").trigger("click");
    await settle();
    expect(dialogText()).toContain("Abandonner votre saisie ?");
    // On reste et la saisie est intacte.
    dialogButton("Continuer la saisie")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("fiche-install");
    expect(questionInput(wrapper, "password").element.value).toBe(SECRET);
    await wrapper.get("[data-install-close]").trigger("click");
    await settle();
    dialogButton("Abandonner")?.click();
    await settle();
    expect(router.currentRoute.value.name).toBe("fiche");
    await router.push(installPath("terraria"));
    await settle();
    expect(questionInput(wrapper, "password").element.value).toBe("");
    wrapper.unmount();
  });
});

describe("le récapitulatif suit la variante et le mapping des ports (BR-FICHE-019, 060)", () => {
  it("ne liste ni le port ni le dossier d'une autre variante", async () => {
    const { wrapper } = await bootCatalogue(rich, installPath("essai"));
    await fillRich(wrapper);
    await openSummary(wrapper);
    expect(document.querySelector("[data-sum-ports]")?.textContent).toContain(
      "25565 vers 25565 (TCP)",
    );
    expect(document.querySelector("[data-sum-ports]")?.textContent).not.toContain("25575");
    expect(document.querySelector("[data-sum-folders]")?.textContent).not.toContain("/data/mods");
    wrapper.unmount();
  });

  it("montre « 7778 vers 7778 » pour un port que le jeu annonce, et le dossier de la variante avec mods", async () => {
    const { wrapper } = await bootCatalogue(rich, installPath("essai"));
    await wrapper.findAll("[data-variant-card] input")[1]?.setValue(true);
    await settle();
    await fillRich(wrapper);
    await answer(wrapper, "mod_pack", "base");
    typeInto(wrapper.get<HTMLInputElement>('[data-port="mods_port"] input').element, "7778");
    await settle();
    await openSummary(wrapper);
    const ports = document.querySelector("[data-sum-ports]")?.textContent ?? "";
    expect(ports).toContain("7778 vers 7778 (TCP)");
    expect(ports).toContain("25565 vers 25565 (TCP)");
    expect(document.querySelector("[data-sum-folders]")?.textContent).toContain("/data/mods");
    wrapper.unmount();
  });

  it("dit dans la fenêtre du contrat que le navigateur ne s'est pas ouvert, sans notification sous la modale", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => {
      rich(b);
      b.failLinks = true;
    }, installPath("essai"));
    await fillRich(wrapper);
    await openSummary(wrapper);
    typeInto(
      document.querySelector<HTMLInputElement>("dialog [data-reauth-field] input"),
      "Correct-Horse-9",
    );
    await flushPromises();
    topButton("Installer le serveur de jeu")?.click();
    await flushPromises();
    (
      document.querySelector("[data-agreement-dialog] [data-agreement-read]") as HTMLElement
    ).click();
    await settle();
    expect(document.querySelector("[data-agreement-failed]")?.textContent).toContain(
      "Hearth n'a pas pu ouvrir le navigateur.",
    );
    expect(toasts()).toEqual([]);
    expect(bridge.catalog.received).toHaveLength(0);
    wrapper.unmount();
  });
});
