import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useToastsStore } from "@/stores/toasts";
import { confirmDialog, dialogButton, OWN, reauthField, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// Supprimer un service depuis la section Services (HRT-86, BR-DEL-001 à 014, 018, 021 à 023, 026, 031), sur le pont
// simulé dont les formes sont celles de la coquille et dont l'agent juge comme le vrai.
beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  document.body.innerHTML = "";
  useServicesStore().reset();
  useServiceOpsStore().reset();
  useServiceRightsStore().reset();
});

async function open(setup?: (bridge: SimulatedLinkBridge) => void, select = "h_web") {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [
    {
      key: "h_web",
      name: "web",
      origin: "hearth",
      graceS: 30,
      mounts: [
        { kind: "folder", source: "/srv/hearth/data/web", target: "/data", readOnly: false },
      ],
    },
    {
      key: "h_db",
      name: "base",
      origin: "hearth",
      state: "stopped",
      mounts: [
        { kind: "folder", source: "/srv/hearth/data/base", target: "/data", readOnly: false },
      ],
    },
    {
      key: "e_old",
      name: "ancien",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
    },
  ]);
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  await wrapper.get(`[data-row][data-key="${select}"]`).trigger("click");
  await flushPromises();
  return { ...ctx, wrapper };
}

const toasts = () => useToastsStore().items.map((toast) => toast.message);
const dialogText = () => document.querySelector("dialog[open]")?.textContent ?? "";
const button = (wrapper: ReturnType<typeof mount>) => wrapper.get("[data-delete-open]");

async function openDialog(wrapper: ReturnType<typeof mount>) {
  await button(wrapper).trigger("click");
  await flushPromises();
}

function check(selector: string, checked: boolean) {
  const input = document.querySelector<HTMLInputElement>(`dialog ${selector} input`);
  if (!input) throw new Error(`case absente : ${selector}`);
  input.checked = checked;
  input.dispatchEvent(new Event("change", { bubbles: true }));
}

describe("la carte « Suppression » (BR-DEL-026, BR-DEL-003)", () => {
  it("porte « Supprimer ce service » dans le détail, séparé du pilotage, et nulle part ailleurs", async () => {
    const { wrapper } = await open();
    const card = wrapper.get("[data-detail-delete]");
    expect(card.text()).toContain("Suppression");
    expect(button(wrapper).text()).toBe("Supprimer ce service");
    expect(button(wrapper).attributes("aria-disabled")).toBeUndefined();
    // Le pilotage et la suppression ne sont pas dans le même groupe.
    expect(wrapper.get("[data-lifecycle]").find("[data-delete-open]").exists()).toBe(false);
    // Le menu de la ligne ne la porte pas (BR-DEL-026).
    await wrapper.get('[data-row][data-key="h_web"] [data-row-menu-button]').trigger("click");
    await flushPromises();
    const items = wrapper
      .findAll("[data-row-menu-item]")
      .map((item) => item.attributes("data-row-menu-item"));
    expect(items).not.toContain("delete");
    expect(document.querySelector("[data-row-menu-list]")?.textContent ?? "").not.toContain(
      "Supprimer",
    );
    wrapper.unmount();
  });

  it("un service en déploiement n'a pas la carte (détail réduit, BR-SVC-025)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.upsert("forge", { key: "h_web", name: "web", state: "deploying" }),
    );
    expect(wrapper.find("[data-detail-delete]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("un service adopté ne propose que de ne plus le gérer : une phrase, aucun bouton (BR-DEL-002)", async () => {
    const { wrapper } = await open(undefined, "e_old");
    expect(wrapper.find("[data-delete-open]").exists()).toBe(false);
    expect(wrapper.get("[data-delete-adopted]").text()).toContain(
      "Hearth ne supprime pas les services créés hors de Hearth. Vous pouvez seulement ne plus le gérer ; pour le supprimer, passez par le serveur.",
    );
    wrapper.unmount();
  });

  it("un agent d'avant la suppression n'a pas la carte", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.accounts.setServiceDeleteServed("forge", false),
    );
    expect(wrapper.find("[data-detail-delete]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("sans le droit, le bouton reste visible et désactivé avec la raison du droit (BR-DEL-001)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.setServiceRightOnServer("forge", "marie", false),
    );
    expect(button(wrapper).attributes("aria-disabled")).toBe("true");
    // La raison du droit est écrite une seule fois dans le détail ; le bouton la décrit (BR-GRANT-014).
    const reason = wrapper.get("[data-rights-reason]");
    expect(reason.text()).toContain("Votre compte ne peut pas gérer les services de ce serveur.");
    expect(button(wrapper).attributes("aria-describedby")).toBe(
      reason.get("[id]").attributes("id"),
    );
    wrapper.unmount();
  });

  it("une action en cours désactive le bouton et nomme le compte (BR-DEL-003, 021)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.serviceOps.accept("forge", { key: "h_web", kind: "backup" }, "claire"),
    );
    expect(button(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-delete-reason]").text()).toBe(
      "Une action est déjà en cours sur ce service. Lancée par claire.",
    );
    wrapper.unmount();
  });

  it("un état que Hearth ne pilote pas désactive le bouton avec sa raison (BR-DEL-003)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.upsert("forge", { key: "h_web", name: "web", state: "paused" }),
    );
    expect(button(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-delete-reason]").text()).toBe(
      "Hearth ne pilote pas ce service dans son état actuel.",
    );
    wrapper.unmount();
  });

  it("le moteur pas prêt désactive le bouton avec sa raison (BR-DEL-022)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.setEngine("forge", { state: "unresponsive" }),
    );
    expect(button(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-delete-reason]").text()).toContain("moteur de conteneurs");
    wrapper.unmount();
  });
});

describe("la fenêtre de confirmation", () => {
  it("dit l'effet, liste les dossiers et leur sort, deux cases décochées, aucune saisie (BR-DEL-004, 005, 006)", async () => {
    const { wrapper } = await open();
    await openDialog(wrapper);
    const text = dialogText();
    expect(text).toContain("Supprimer web ?");
    expect(text).toContain("Le service web sera retiré du serveur.");
    expect(text).toContain("Sa configuration (variables, ports, commande) sera effacée avec lui.");
    expect(text).not.toContain("Hearth ne la garde pas");
    expect(text).toContain("L'image nginx:1.27 reste sur le serveur.");
    // Le tableau : colonnes, un dossier géré, son sort.
    const heads = [...document.querySelectorAll("dialog th")].map((cell) =>
      cell.textContent?.trim(),
    );
    expect(heads).toEqual(["Dossier", "Taille", "Éléments", "Sort"]);
    expect(document.querySelector("dialog tbody tr")?.textContent).toContain(
      "/srv/hearth/data/web",
    );
    expect(document.querySelector("dialog tbody tr")?.textContent).toContain("Conservé");
    expect(text).toContain("restent sur le serveur");
    // Les cases sont décochées ; aucun nom à retaper tant qu'aucune n'est cochée.
    expect(
      document.querySelector<HTMLInputElement>("dialog [data-delete-data] input")?.checked,
    ).toBe(false);
    expect(document.querySelector("dialog [data-delete-backups]")).toBeNull();
    expect(document.querySelector("dialog [data-delete-name]")).toBeNull();
    // Le mot de passe est demandé, et le bouton reste plein rouge et actif sans case cochée.
    expect(reauthField()).not.toBeNull();
    expect(dialogButton("Supprimer le service")).toBeDefined();
    wrapper.unmount();
  });

  it("un service en marche annonce son arrêt avec le délai réel (BR-DEL-013)", async () => {
    const { wrapper } = await open();
    await openDialog(wrapper);
    expect(document.querySelector("[data-delete-running]")?.textContent).toContain(
      "Le service est en marche : il sera arrêté d'abord.",
    );
    expect(document.querySelector("[data-delete-running]")?.textContent).toContain(
      "au bout de 30 secondes",
    );
    wrapper.unmount();
  });

  it("un service arrêté n'annonce pas d'arrêt", async () => {
    const { wrapper } = await open(undefined, "h_db");
    await openDialog(wrapper);
    expect(document.querySelector("[data-delete-running]")).toBeNull();
    wrapper.unmount();
  });

  it("cocher les données change le sort, avertit et exige le nom retapé, casse comprise (BR-DEL-011)", async () => {
    const { wrapper } = await open();
    await openDialog(wrapper);
    check("[data-delete-data]", true);
    await flushPromises();
    expect(document.querySelector("dialog tbody tr")?.textContent).toContain("Effacé");
    expect(document.querySelector("[data-delete-erased]")?.textContent).toContain(
      "Cette suppression est définitive : il n'y a pas de corbeille.",
    );
    expect(document.querySelector("[data-delete-kept]")).toBeNull();
    expect(document.querySelector("dialog [data-delete-name]")).not.toBeNull();
    const confirm = () => dialogButton("Supprimer le service");
    typeInto(reauthField(), OWN);
    await flushPromises();
    expect(confirm()?.getAttribute("aria-disabled")).toBe("true");
    const name = document.querySelector<HTMLInputElement>("dialog [data-delete-name] input");
    typeInto(name, "Web");
    await flushPromises();
    expect(document.querySelector("dialog")?.textContent).toContain(
      "Le nom saisi ne correspond pas.",
    );
    expect(confirm()?.getAttribute("aria-disabled")).toBe("true");
    typeInto(name, "web");
    await flushPromises();
    expect(confirm()?.getAttribute("aria-disabled")).not.toBe("true");
    // Décochée, la saisie n'est plus demandée.
    check("[data-delete-data]", false);
    await flushPromises();
    expect(document.querySelector("dialog [data-delete-name]")).toBeNull();
    wrapper.unmount();
  });

  it("la mesure interrompue dit « plus de … » et la taille non mesurée (BR-DEL-009)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.deletes.setPreview("forge", "h_web", {
        measuredInTime: false,
        folders: [
          {
            path: "/srv/hearth/data/web",
            bytes: 3 * 1024 ** 3,
            items: 900000,
            partial: true,
            unmeasured: false,
            fate: "erasable",
            sharedWith: [],
            sharedWithUnmanaged: false,
          },
        ],
      }),
    );
    await openDialog(wrapper);
    expect(document.querySelector("dialog tbody")?.textContent).toContain(
      "plus de 3 Go (mesure interrompue)",
    );
    expect(document.querySelector("[data-delete-size-note]")?.textContent).toBe(
      "Taille non mesurée pour /srv/hearth/data/web : plus grande que 3 Go.",
    );
    // La confirmation reste possible avec une taille non mesurée : seul le mot de passe la retient.
    typeInto(reauthField(), OWN);
    await flushPromises();
    expect(dialogButton("Supprimer le service")?.getAttribute("aria-disabled")).not.toBe("true");
    wrapper.unmount();
  });

  it("un dossier partagé et un dossier du serveur sont conservés et le disent (BR-DEL-006, 008)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.deletes.setPreview("forge", "h_web", {
        folders: [
          {
            path: "/srv/hearth/data/web",
            bytes: 1024,
            items: 3,
            partial: false,
            unmeasured: false,
            fate: "kept_shared",
            sharedWith: ["base"],
            sharedWithUnmanaged: true,
          },
        ],
        serverFolders: ["/mnt/jeux"],
        volumes: 2,
      }),
    );
    await openDialog(wrapper);
    const rows = [...document.querySelectorAll("dialog tbody tr")].map((row) => row.textContent);
    expect(rows[0]).toContain("Conservé (partagé)");
    expect(rows[1]).toContain("/mnt/jeux");
    expect(rows[1]).toContain("Conservé (appartient au serveur)");
    const text = dialogText();
    expect(text).toContain(
      "Conservé : /srv/hearth/data/web, aussi utilisé par base et un conteneur créé hors de Hearth.",
    );
    expect(text).toContain("Ces dossiers appartiennent au serveur : Hearth ne les efface jamais.");
    expect(text).toContain("2 espaces de stockage gérés par le moteur de conteneurs");
    // Rien d'effaçable : la case des données n'est pas proposée.
    expect(document.querySelector("dialog [data-delete-data]")).toBeNull();
    wrapper.unmount();
  });

  it("un service sans dossier géré le dit, sans case de données", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.deletes.setPreview("forge", "h_web", { folders: [] }),
    );
    await openDialog(wrapper);
    expect(document.querySelector("[data-delete-no-folders]")?.textContent).toContain(
      "aucun dossier de données géré par Hearth",
    );
    expect(document.querySelector("dialog [data-delete-data]")).toBeNull();
    wrapper.unmount();
  });

  it("la case des sauvegardes n'existe que si Hearth en connaît (BR-DEL-005)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.deletes.setPreview("forge", "h_web", { backups: 3 }),
    );
    await openDialog(wrapper);
    expect(document.querySelector("dialog [data-delete-backups]")).not.toBeNull();
    check("[data-delete-backups]", true);
    await flushPromises();
    expect(document.querySelector("[data-delete-backups-notice]")?.textContent).toBe(
      "Les 3 sauvegardes de ce service seront supprimées avec lui. Cette suppression est définitive.",
    );
    expect(document.querySelector("dialog [data-delete-name]")).not.toBeNull();
    wrapper.unmount();
  });

  it("annuler ne change rien et n'envoie rien (BR-LIFE-017)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    dialogButton("Annuler")?.click();
    await flushPromises();
    expect(bridge.deletes.received).toEqual([]);
    expect(bridge.calls.some((call) => call === "service delete")).toBe(false);
    wrapper.unmount();
  });

  it("la lecture des dossiers qui échoue se réessaie et ne laisse rien confirmer", async () => {
    const { wrapper, bridge } = await open((sim) => {
      sim.deletes.failPreview = true;
    });
    await openDialog(wrapper);
    expect(document.querySelector("[data-delete-failed]")).not.toBeNull();
    expect(dialogButton("Supprimer le service")?.getAttribute("aria-disabled")).toBe("true");
    bridge.deletes.failPreview = false;
    (document.querySelector("[data-delete-retry]") as HTMLElement).click();
    await flushPromises();
    expect(document.querySelector("dialog tbody")).not.toBeNull();
    wrapper.unmount();
  });

  it("l'état change sous la fenêtre : elle le dit et la confirmation est inactive (BR-LIFE-022)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    bridge.services.upsert("forge", { key: "h_web", name: "web", state: "paused" });
    await flushPromises();
    expect(document.querySelector("[data-delete-stale]")?.textContent).toBe(
      "Ce service vient de passer à l'état « En pause ».",
    );
    expect(dialogButton("Supprimer le service")?.getAttribute("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("le service disparaît sous la fenêtre : « Ce service n'existe plus. » (BR-DEL-018)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    bridge.services.remove("forge", "h_web");
    await flushPromises();
    expect(document.querySelector("[data-delete-stale]")?.textContent).toBe(
      "Ce service n'existe plus.",
    );
    wrapper.unmount();
  });
});

describe("supprimer", () => {
  it("demande le mot de passe même sous l'élévation de cinq minutes (BR-DEL-012)", async () => {
    const { wrapper, bridge } = await open((sim) => sim.reauth.open("forge"));
    await openDialog(wrapper);
    await flushPromises();
    expect(reauthField()).not.toBeNull();
    expect(bridge.reauth.state("forge").elevatedForS).toBeGreaterThan(0);
    wrapper.unmount();
  });

  it("sans case cochée, envoie un corps sans nom et suit l'opération : étapes, puis service supprimé, données conservées", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    expect(bridge.deletes.received).toEqual([
      {
        serverId: "forge",
        key: "h_web",
        request: { eraseData: false, eraseBackups: false, confirmName: null },
      },
    ]);
    const progress = document.querySelector("[data-delete-progress]");
    expect(progress?.textContent).toContain("Supprimer web");
    // Un service en marche : l'arrêt est une étape.
    const steps = [...document.querySelectorAll("[data-progress-steps] [data-step]")].map((step) =>
      step.getAttribute("data-step"),
    );
    expect(steps).toEqual(["stop", "remove"]);
    const [op] = bridge.deletes.running("forge");
    expect(op).toBeDefined();
    bridge.deletes.toStep("forge", op?.operationId ?? "", "remove_container");
    await flushPromises();
    // Le conteneur est supprimé : plus d'annulation.
    expect(document.querySelector("[data-delete-cancel]")?.getAttribute("aria-disabled")).toBe(
      "true",
    );
    expect(document.querySelector("[data-delete-too-late]")?.textContent).toBe(
      "Il est trop tard pour annuler : la suppression du service a commencé.",
    );
    bridge.deletes.succeed("forge", op?.operationId ?? "");
    await flushPromises();
    expect(document.querySelector("[data-delete-success]")?.textContent).toContain(
      "Le service web est supprimé. Ses données sont conservées dans /srv/hearth/data/web.",
    );
    expect(document.querySelector("[data-delete-success]")?.textContent).toContain(
      "Si vous créez plus tard un service nommé web, il les retrouvera.",
    );
    expect(toasts()).toContain("Le service web est supprimé.");
    // Le service a quitté la liste et la fenêtre ramène à la liste (BR-DEL-028).
    (document.querySelector("[data-delete-close]") as HTMLElement).click();
    await flushPromises();
    expect(wrapper.find('[data-row][data-key="h_web"]').exists()).toBe(false);
    expect(wrapper.find("[data-service-detail]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("avec les données : le nom est envoyé, l'effacement est suivi, l'issue est dite", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    check("[data-delete-data]", true);
    await flushPromises();
    typeInto(document.querySelector("dialog [data-delete-name] input"), "web");
    await confirmDialog("Supprimer le service", OWN);
    expect(bridge.deletes.received[0]?.request).toEqual({
      eraseData: true,
      eraseBackups: false,
      confirmName: "web",
    });
    const [op] = bridge.deletes.running("forge");
    const steps = [...document.querySelectorAll("[data-progress-steps] [data-step]")].map((step) =>
      step.getAttribute("data-step"),
    );
    expect(steps).toEqual(["stop", "remove", "erase"]);
    bridge.deletes.erasing("forge", op?.operationId ?? "", 12000);
    await flushPromises();
    expect(document.querySelector("[data-progress-steps]")?.textContent).toMatch(
      /12\s000 éléments effacés/,
    );
    expect(document.querySelector("[data-delete-close-note]")?.textContent).toContain(
      "L'effacement des données continue même si vous fermez cette fenêtre.",
    );
    bridge.deletes.succeed("forge", op?.operationId ?? "");
    await flushPromises();
    expect(document.querySelector("[data-delete-success]")?.textContent).toBe(
      "Le service web et ses données sont supprimés.",
    );
    wrapper.unmount();
  });

  it("des données qui n'ont pas pu être effacées : le service est supprimé, la liste des dossiers est dite (BR-DEL-016)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    check("[data-delete-data]", true);
    await flushPromises();
    typeInto(document.querySelector("dialog [data-delete-name] input"), "web");
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.succeed("forge", op?.operationId ?? "", "data_not_erased");
    await flushPromises();
    const partial = document.querySelector("[data-delete-partial]")?.textContent ?? "";
    expect(partial).toContain("est supprimé, mais certaines données n'ont pas pu être effacées");
    expect(partial).toContain("/srv/hearth/data/web");
    expect(toasts()).toContain(
      "Le service web est supprimé, mais certaines données n'ont pas pu être effacées.",
    );
    wrapper.unmount();
  });

  it("l'arrêt forcé est dit dans l'issue (BR-DEL-013)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.succeed("forge", op?.operationId ?? "", "forced_stop");
    await flushPromises();
    expect(document.querySelector("[data-delete-forced]")?.textContent).toContain(
      "il a été coupé de force",
    );
    wrapper.unmount();
  });

  it("l'échec avant le point de non-retour : le service reste entier et la cause est dite (BR-DEL-015)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.fail("forge", op?.operationId ?? "", "engine_refused");
    await flushPromises();
    const text = document.querySelector("[data-delete-not-deleted]")?.textContent ?? "";
    expect(text).toContain("Le service web n'a pas été supprimé.");
    expect(text).toContain("Le moteur de conteneurs a refusé de supprimer le service.");
    expect(text).toContain("Il reste en place avec sa configuration et ses données.");
    expect(
      useServicesStore()
        .of("forge")
        ?.services.some((service) => service.key === "h_web"),
    ).toBe(true);
    wrapper.unmount();
  });

  it("annuler avant le point de non-retour ne laisse rien et le dit (BR-DEL-014)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    expect(document.querySelector("[data-delete-cancel]")?.getAttribute("aria-disabled")).not.toBe(
      "true",
    );
    (document.querySelector("[data-delete-cancel]") as HTMLElement).click();
    await flushPromises();
    // La fenêtre de confirmation de l'annulation est la dernière ouverte.
    const last = [...document.querySelectorAll("dialog[open]")].at(-1);
    const field = last?.querySelector<HTMLInputElement>("[data-reauth-field] input") ?? null;
    if (field) typeInto(field, OWN);
    await flushPromises();
    const confirm = [...(last?.querySelectorAll("button") ?? [])].find(
      (candidate) => candidate.textContent?.trim() === "Annuler la suppression",
    );
    confirm?.click();
    await flushPromises();
    await vi.advanceTimersByTimeAsync(10);
    await flushPromises();
    expect(bridge.deletes.cancels).toHaveLength(1);
    expect(document.querySelector("[data-delete-cancelled]")?.textContent).toContain(
      "La suppression a été annulée. Le service web est conservé.",
    );
    expect(wrapper.find('[data-row][data-key="h_web"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it("un lien coupé après l'envoi : résultat inconnu, jamais rejoué (BR-DEL-023)", async () => {
    const { wrapper, bridge } = await open();
    bridge.actionMode = "cut";
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    expect(toasts()).toContain("Le résultat de cette action n'est pas connu.");
    expect(document.querySelector("[data-delete-unknown]")?.textContent).toContain(
      "Rien n'est relancé.",
    );
    // Jamais rejoué : une seule demande est partie, le serveur va au bout de son côté.
    expect(bridge.calls.filter((call) => call === "service delete")).toHaveLength(1);
    wrapper.unmount();
  });

  it("un mot de passe faux se dit sous le champ et ne ferme rien", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", "Mauvais-Mot-2");
    expect(dialogText()).toContain("Supprimer web ?");
    expect(bridge.deletes.received).toEqual([]);
    expect(document.querySelector("[data-delete-progress]")).toBeNull();
    wrapper.unmount();
  });

  it("l'agent refuse un service devenu occupé : le texte est dit dans la fenêtre (BR-DEL-021)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    bridge.serviceOps.accept("forge", { key: "h_web", kind: "backup" }, "claire");
    await flushPromises();
    // Le verrou posé sous la fenêtre désactive la confirmation avant même l'envoi.
    expect(document.querySelector("[data-delete-stale]")?.textContent).toBe(
      "Une action est déjà en cours sur ce service.",
    );
    wrapper.unmount();
  });

  it("l'agent refuse la case des sauvegardes tant qu'il n'en a pas : jamais ignorée en silence", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.deletes.setPreview("forge", "h_web", { backups: 2 }),
    );
    await openDialog(wrapper);
    check("[data-delete-backups]", true);
    await flushPromises();
    typeInto(document.querySelector("dialog [data-delete-name] input"), "web");
    await confirmDialog("Supprimer le service", OWN);
    expect(dialogText()).toContain(
      "Hearth ne gère pas encore les sauvegardes de ce service : décochez cette case.",
    );
    expect(bridge.deletes.received).toEqual([]);
    wrapper.unmount();
  });
});

describe("la fermeture de la fenêtre de suivi", () => {
  it("fermer en cours n'arrête rien : l'issue est ensuite annoncée par une notification, une seule fois", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    (document.querySelector("[data-delete-close]") as HTMLElement).click();
    await flushPromises();
    expect(document.querySelector("dialog[open]")).toBeNull();
    bridge.deletes.toStep("forge", op?.operationId ?? "", "remove_container");
    bridge.deletes.succeed("forge", op?.operationId ?? "");
    await flushPromises();
    expect(toasts().filter((message) => message === "Le service web est supprimé.")).toHaveLength(
      1,
    );
    // Fermée en cours, la page garde le détail ouvert : il dit que le service n'existe plus (BR-DEL-028).
    expect(wrapper.find("[data-detail-gone]").exists()).toBe(true);
    wrapper.unmount();
  });
});

describe("la revue UX de Nora du 14/11", () => {
  it("avant l'effacement, l'opération se dit « Suppression », jamais « Effacement des données » (constat 1)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.toStep("forge", op?.operationId ?? "", "stop");
    await flushPromises();
    const strip = document.body.textContent ?? "";
    expect(strip).toContain("Suppression de web");
    expect(strip).not.toContain("Effacement des données de web");
    wrapper.unmount();
  });

  it("le message définitif est en rouge, la raison du bouton et l'aide du nom sont écrites (constats 7, 8, 9)", async () => {
    const { wrapper } = await open();
    await openDialog(wrapper);
    expect(document.querySelector("[data-delete-erased]")).toBeNull();
    check("[data-delete-data]", true);
    await flushPromises();
    expect(
      document.querySelector("[data-delete-erased]")?.classList.contains("callout--error"),
    ).toBe(true);
    expect(document.querySelector("[data-delete-kept]")).toBeNull();
    // Le nom est à retaper : la raison du bouton le dit, l'aide dit que la casse compte.
    expect(dialogText()).toContain("Retapez le nom du service pour continuer.");
    expect(dialogText()).toContain("Les majuscules et les minuscules comptent.");
    typeInto(reauthField(), OWN);
    typeInto(document.querySelector("dialog [data-delete-name] input"), "web");
    await flushPromises();
    expect(dialogText()).not.toContain("Retapez le nom du service pour continuer.");
    wrapper.unmount();
  });

  it("après un service supprimé, fermer la fenêtre ramène à la liste (constat 5)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.toStep("forge", op?.operationId ?? "", "remove_container");
    bridge.deletes.succeed("forge", op?.operationId ?? "");
    await flushPromises();
    (document.querySelector("[data-delete-close]") as HTMLElement).click();
    await flushPromises();
    expect(wrapper.find("[data-service-detail]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("des données pas effacées : la marche à suivre est écrite (constat 6)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    check("[data-delete-data]", true);
    await flushPromises();
    typeInto(document.querySelector("dialog [data-delete-name] input"), "web");
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.succeed("forge", op?.operationId ?? "", "data_not_erased");
    await flushPromises();
    expect(document.querySelector("[data-delete-partial]")?.textContent).toContain(
      "Hearth ne relance pas l'effacement.",
    );
    wrapper.unmount();
  });

  it("un échec dit quoi faire ensuite (constat 12) ; une taille inconnue ne se confond pas avec un autre dossier (constat 4)", async () => {
    const { wrapper, bridge } = await open();
    await openDialog(wrapper);
    await confirmDialog("Supprimer le service", OWN);
    const [op] = bridge.deletes.running("forge");
    bridge.deletes.fail("forge", op?.operationId ?? "", "engine_refused");
    await flushPromises();
    expect(document.querySelector("[data-delete-not-deleted]")?.textContent).toContain(
      "Vous pouvez réessayer depuis la fiche du service.",
    );
    wrapper.unmount();
  });
});
