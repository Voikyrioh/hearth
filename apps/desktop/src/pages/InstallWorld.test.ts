import { flushPromises, type VueWrapper } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge, WorldRoot } from "@/link";
import { dialogButton } from "@/test/confirm";
import {
  bootCatalogue,
  confirmTop,
  dialogText,
  goNext,
  installPath,
  questionInput,
  settle,
  topButton,
  useFicheTestClock,
} from "@/test/fiche";

// Reprendre un monde existant depuis l'installation d'un serveur de jeu (HRT-106, BR-MONDE-002, 003, 006, 009, 012 à 016),
// sur le pont simulé dont l'agent juge COMME LE VRAI. Aucun nom de monde, aucun dossier ni aucun chemin n'est écrit dans le
// client : tout vient de ce que l'agent a listé.
useFicheTestClock();

const GIB = 1024 ** 3;
const FOLDER = "/srv/jeux";

const ROOT: WorldRoot = {
  folder: FOLDER,
  readable: true,
  truncated: false,
  worlds: [
    {
      path: "Worlds/Alpha.wld",
      name: "Alpha",
      files: 1,
      bytes: 300_000,
      modifiedAt: "2026-11-14T09:00:00Z",
      usable: true,
    },
    {
      path: "Worlds/Mon monde.wld",
      name: "Mon monde",
      files: 1,
      bytes: 500_000,
      modifiedAt: "2026-11-13T09:00:00Z",
      usable: false,
    },
  ],
};

function withWorlds(roots: WorldRoot[] = [ROOT]) {
  return (bridge: SimulatedLinkBridge) => bridge.worlds.seed("forge", "terraria", roots);
}

const card = (wrapper: VueWrapper) => wrapper.find("[data-world-card]");
const existing = (wrapper: VueWrapper) =>
  wrapper.get<HTMLInputElement>("[data-world-existing] input");

async function chooseExisting(wrapper: VueWrapper) {
  await existing(wrapper).setValue(true);
  await settle();
}

async function chooseAlpha(wrapper: VueWrapper) {
  await chooseExisting(wrapper);
  await wrapper.get("[data-world-choose]").trigger("click");
  await settle();
  await flushPromises();
}

async function openSummary(wrapper: VueWrapper) {
  await goNext(wrapper);
  await goNext(wrapper);
}

describe("la carte « Monde » (BR-MONDE-002, 003, 039)", () => {
  it("propose « Nouveau monde » par défaut et « Reprendre un monde existant » quand un dossier est ouvert", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    expect(card(wrapper).exists()).toBe(true);
    expect(wrapper.get<HTMLInputElement>("[data-world-new] input").element.checked).toBe(true);
    expect(existing(wrapper).element.disabled).toBe(false);
    // La liste est lue une fois, à l'ouverture, sans rien inspecter.
    expect(bridge.worlds.listed).toHaveLength(1);
    expect(bridge.worlds.inspected).toHaveLength(0);
    wrapper.unmount();
  });

  it("désactive la reprise, avec sa raison, quand aucun dossier du serveur n'est ouvert aux services", async () => {
    const { wrapper } = await bootCatalogue(withWorlds([]), installPath("terraria"));
    expect(existing(wrapper).element.disabled).toBe(true);
    expect(wrapper.get("[data-world-existing]").text()).toContain(
      "Aucun dossier du serveur n'est ouvert aux services.",
    );
    wrapper.unmount();
  });

  it("n'existe pas pour une fiche qui ne dit pas ce qu'est un monde", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => {
      b.catalog.addRich();
    }, installPath("essai"));
    expect(card(wrapper).exists()).toBe(false);
    expect(bridge.worlds.listed).toHaveLength(0);
    wrapper.unmount();
  });

  it("n'existe pas pour un agent qui ne sait pas reprendre un monde, et l'installation d'un monde neuf continue", async () => {
    const { wrapper } = await bootCatalogue((bridge) => {
      bridge.worlds.unavailable = true;
    }, installPath("terraria"));
    expect(card(wrapper).exists()).toBe(false);
    expect(wrapper.find("[data-install-answers]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("liste chaque monde avec son dossier, dit celui dont le nom est refusé et signale une recherche tronquée", async () => {
    const { wrapper } = await bootCatalogue(
      withWorlds([{ ...ROOT, truncated: true }]),
      installPath("terraria"),
    );
    await chooseExisting(wrapper);
    const rows = wrapper.findAll("[data-world-row]");
    expect(rows).toHaveLength(2);
    expect(rows[0]?.text()).toContain("Alpha");
    expect(rows[0]?.text()).toContain(FOLDER);
    expect(rows[1]?.text()).toContain("Le nom de ce monde n'est pas accepté par ce jeu.");
    expect(rows[1]?.get("[data-world-choose]").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-world-truncated]").text()).toContain(
      "La recherche s'est arrêtée à 5 000 éléments",
    );
    wrapper.unmount();
  });

  it("dit « Aucun monde trouvé » quand la recherche ne trouve rien", async () => {
    const { wrapper } = await bootCatalogue(
      withWorlds([{ ...ROOT, worlds: [] }]),
      installPath("terraria"),
    );
    await chooseExisting(wrapper);
    expect(wrapper.get("[data-world-empty]").text()).toBe(
      "Aucun monde trouvé dans les dossiers ouverts aux services.",
    );
    wrapper.unmount();
  });
});

describe("choisir un monde (BR-MONDE-006, 012, 014)", () => {
  it("inspecte le monde choisi, dit ce que la copie fera et ce qu'elle laissera, et ne demande ni son nom ni ses réglages figés", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => {
      withWorlds()(b);
      b.worlds.setInspection("forge", FOLDER, "Worlds/Alpha.wld", {
        notCopied: 1,
        notCopiedNames: ["Alpha.wld.bak"],
      });
    }, installPath("terraria"));
    // Avant le choix, la fiche pose son nom de monde et ses réglages figés.
    expect(wrapper.find('[data-question="world_name"]').exists()).toBe(true);
    await chooseAlpha(wrapper);
    expect(bridge.worlds.inspected).toEqual([
      {
        serverId: "forge",
        ficheId: "terraria",
        world: { folder: FOLDER, path: "Worlds/Alpha.wld" },
      },
    ]);
    expect(wrapper.get("[data-world-name]").text()).toBe("Alpha");
    expect(wrapper.get("[data-world-measure]").text()).toContain("Copie de");
    expect(wrapper.get("[data-world-left]").text()).toContain("Alpha.wld.bak");
    expect(wrapper.get("[data-world-never-deleted]").text()).toContain(
      "Hearth ne supprime jamais le monde d'origine.",
    );
    expect(wrapper.get("[data-world-coherence]").text()).toContain("incohérente");
    // Le monde porte son nom et ses réglages figés : la page ne les pose plus.
    expect(wrapper.find('[data-question="world_name"]').exists()).toBe(false);
    expect(wrapper.find('[data-question="world_size"]').exists()).toBe(false);
    expect(wrapper.find('[data-question="max_players"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it("dit le refus de l'agent pour un monde qui a disparu, sans panne", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await chooseExisting(wrapper);
    // Le monde disparaît entre la liste et le choix.
    bridge.worlds.seed("forge", "terraria", [{ ...ROOT, worlds: [] }]);
    await wrapper.get("[data-world-choose]").trigger("click");
    await settle();
    await flushPromises();
    expect(wrapper.get("[data-world-refused]").text()).toBe(
      "Ce n'est pas un monde que ce jeu reconnaît.",
    );
    wrapper.unmount();
  });

  it("ne continue pas sans monde choisi, puis « Changer de monde » rouvre la liste", async () => {
    const { wrapper } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await chooseExisting(wrapper);
    // « Continuer » est inactif tant que le monde n'est pas choisi ; « Aller au champ » montre pourquoi.
    expect(wrapper.get("[data-install-next]").attributes("aria-disabled")).toBe("true");
    await wrapper.get("[data-install-first-issue]").trigger("click");
    await settle();
    expect(wrapper.get("[data-world-missing]").text()).toBe("Choisissez un monde à reprendre.");
    expect(wrapper.find("[data-install-resources]").exists()).toBe(false);
    await wrapper.get("[data-world-choose]").trigger("click");
    await settle();
    await wrapper.get("[data-world-change]").trigger("click");
    await settle();
    expect(wrapper.find("[data-world-picker]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("bloque « Continuer » quand le disque n'a pas assez de place, sans case pour passer outre", async () => {
    const { wrapper } = await bootCatalogue((b) => {
      withWorlds()(b);
      b.worlds.setInspection("forge", FOLDER, "Worlds/Alpha.wld", { bytes: 50 * GIB });
      b.deploys.setDisk("forge", 10 * GIB);
    }, installPath("terraria"));
    await chooseAlpha(wrapper);
    expect(wrapper.find("[data-world-room]").exists()).toBe(true);
    await goNext(wrapper);
    expect(wrapper.find("[data-install-reason]").text()).toContain(
      "Le disque n'a pas assez de place pour copier ce monde.",
    );
    wrapper.unmount();
  });
});

describe("installer avec un monde (BR-MONDE-009, 013, 014, 016)", () => {
  it("envoie le monde et ses gestes sans réponse pour le nom ni les réglages figés, après « Je comprends »", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await chooseAlpha(wrapper);
    await wrapper.get("[data-world-keep] input").setValue(true);
    await openSummary(wrapper);
    // Le récapitulatif nomme le monde repris et le geste « Je comprends » est un point d'attention obligatoire.
    expect(document.querySelector("[data-sum-world]")?.textContent).toContain("Alpha");
    expect(document.querySelector("[data-sum-world-copy]")?.textContent).toContain("Copie de");
    const point = document.querySelector('[data-attention="world_version"]');
    expect(point?.textContent).toContain("Hearth ne peut pas savoir avec quelle version du jeu");
    const submit = topButton("Installer le serveur de jeu");
    expect(submit?.disabled || submit?.getAttribute("aria-disabled") === "true").toBe(true);
    point?.querySelector<HTMLInputElement>("input")?.click();
    await settle();
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const request = bridge.catalog.received[0]?.request;
    expect(request?.world).toEqual({
      folder: FOLDER,
      path: "Worlds/Alpha.wld",
      versionUnderstood: true,
      keepIfChanged: true,
    });
    const ids = (request?.answers ?? []).map((item) => item.id);
    expect(ids).not.toContain("world_name");
    expect(ids).not.toContain("world_size");
    expect(ids).toContain("max_players");
    wrapper.unmount();
  });

  it("montre « Copie du monde » entre la récupération et la création, avec « Annuler la reprise » jusqu'à la fin de la copie", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await chooseAlpha(wrapper);
    await openSummary(wrapper);
    document.querySelector<HTMLInputElement>('[data-attention="world_version"] input')?.click();
    await settle();
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const op = bridge.deploys.running("forge")[0];
    if (!op) throw new Error("aucune installation en cours");
    const text = dialogText();
    expect(text).toContain("Copie du monde");
    expect(text.indexOf("Récupération")).toBeLessThan(text.indexOf("Copie du monde"));
    expect(text.indexOf("Copie du monde")).toBeLessThan(text.indexOf("Création"));
    bridge.deploys.worldProgress("forge", op.operationId, {
      itemsDone: 1,
      bytesDone: 100_000,
      bytesTotal: 300_000,
    });
    await settle();
    expect(document.querySelector("[data-deploy-cancel]")?.textContent?.trim()).toBe(
      "Annuler la reprise",
    );
    expect(dialogText()).toContain("33 %");
    // Annuler pendant la copie : rien n'est créé, le monde d'origine n'est pas touché.
    document.querySelector<HTMLElement>("[data-deploy-cancel]")?.click();
    await settle();
    await confirmTop("Annuler la reprise");
    await vi.advanceTimersByTimeAsync(10);
    await settle();
    expect(bridge.deploys.cancels).toEqual([{ serverId: "forge", opId: op.operationId }]);
    expect(bridge.services.find("forge", op.key)).toBeUndefined();
    wrapper.unmount();
  });

  it("dit qu'une copie incomplète ne crée rien et ne touche pas au monde d'origine", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await chooseAlpha(wrapper);
    await openSummary(wrapper);
    document.querySelector<HTMLInputElement>('[data-attention="world_version"] input')?.click();
    await settle();
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const op = bridge.deploys.running("forge")[0];
    if (!op) throw new Error("aucune installation en cours");
    bridge.deploys.worldProgress("forge", op.operationId, {
      itemsDone: 0,
      bytesDone: 0,
      bytesTotal: 300_000,
    });
    await settle();
    bridge.deploys.fail("forge", op.operationId, "step_failed");
    await settle();
    expect(document.querySelector("[data-deploy-failed]")?.textContent).toContain(
      "La copie du monde n'a pas abouti. Rien n'a été créé et le monde d'origine n'a pas été touché.",
    );
    wrapper.unmount();
  });

  it("dit qu'une reprise tient déjà la place du serveur, et garde la saisie", async () => {
    const { wrapper, bridge } = await bootCatalogue((b) => {
      withWorlds()(b);
      b.deploys.heavyIoBusy = true;
    }, installPath("terraria"));
    await chooseAlpha(wrapper);
    await openSummary(wrapper);
    document.querySelector<HTMLInputElement>('[data-attention="world_version"] input')?.click();
    await settle();
    await confirmTop("Installer le serveur de jeu");
    await settle();
    expect(bridge.catalog.received).toHaveLength(0);
    expect(wrapper.text()).toContain("est déjà en cours sur ce serveur. Attendez sa fin.");
    expect(dialogButton("Installer le serveur de jeu")).toBeUndefined();
    wrapper.unmount();
  });

  it("garde la demande d'un monde neuf inchangée : aucun monde, aucune réponse retirée", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await openSummary(wrapper);
    await confirmTop("Installer le serveur de jeu");
    await settle();
    const request = bridge.catalog.received[0]?.request;
    expect(request?.world ?? null).toBeNull();
    expect((request?.answers ?? []).map((item) => item.id)).toContain("world_name");
    wrapper.unmount();
  });
});

describe("le refus de l'agent est dit par son code", () => {
  it("un agent qui ne reprend pas de monde le dit sans panne", async () => {
    const { wrapper, bridge } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await chooseAlpha(wrapper);
    // L'agent perd la capacité entre la lecture et l'installation (mise à jour à l'envers, autre agent).
    bridge.worlds.unavailable = true;
    await openSummary(wrapper);
    document.querySelector<HTMLInputElement>('[data-attention="world_version"] input')?.click();
    await settle();
    await confirmTop("Installer le serveur de jeu");
    await settle();
    expect(bridge.catalog.received).toHaveLength(0);
    expect(wrapper.text()).toContain("Ce serveur ne sait pas reprendre un monde.");
    expect(questionInput(wrapper, "max_players").element.value).toBe("8");
    wrapper.unmount();
  });
});

describe("l'aide « J'ai déjà un serveur de jeu » (BR-MONDE-021)", () => {
  it("s'ouvre depuis la carte « Monde », donne l'ordre essayer, basculer, adopter et ne nomme aucun jeu ni dossier", async () => {
    const { wrapper, router } = await bootCatalogue(withWorlds(), installPath("terraria"));
    await wrapper.get("[data-world-help]").trigger("click");
    await settle();
    expect(router.currentRoute.value.name).toBe("existing-server-help");
    const steps = wrapper
      .findAll("[data-help-step]")
      .map((step) => step.attributes("data-help-step"));
    expect(steps).toEqual(["try", "switch", "adopt"]);
    expect(wrapper.get("[data-help-foot]").text()).toBe(
      "Si votre jeu n'a pas de fiche dans le Catalogue, seule l'adoption est possible.",
    );
    expect(wrapper.get("[data-existing-help]").text()).not.toMatch(
      /terraria|minecraft|valheim|\/srv/i,
    );
    expect(wrapper.find("[data-help-adopt]").exists()).toBe(true);
    wrapper.unmount();
  });
});
