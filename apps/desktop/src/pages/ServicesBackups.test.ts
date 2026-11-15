import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { confirmDialog, dialogButton, OWN, reauthField, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// L'onglet « Sauvegardes » d'un service (HRT-98, BR-BACKUP-001 à 015, 026 à 028), sur le pont simulé dont les formes
// sont celles de la coquille et dont l'agent juge comme le vrai.
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

const DAY = "2026-11-13T21:07:00Z";

async function open(setup?: (bridge: SimulatedLinkBridge) => void, select = "h_web") {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [
    {
      key: "h_web",
      name: "web",
      origin: "hearth",
      mounts: [
        { kind: "folder", source: "/srv/hearth/data/web", target: "/data", readOnly: false },
      ],
    },
    {
      key: "e_old",
      name: "ancien",
      origin: "external",
      adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
    },
  ]);
  ctx.bridge.backups.seed("forge", "h_web", [
    {
      id: "01SIMBKAAAAAAA",
      createdAt: DAY,
      archiveBytes: 2_000_000_000,
      note: "avant mise à jour",
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

async function openTab(wrapper: ReturnType<typeof mount>) {
  await wrapper.get("[data-tab='backups']").trigger("click");
  await flushPromises();
}

describe("l'onglet « Sauvegardes » : existence (BR-BACKUP-028)", () => {
  it("existe pour un service créé par Hearth et liste ses sauvegardes", async () => {
    const { wrapper, bridge } = await open();
    await openTab(wrapper);
    expect(wrapper.find("[data-backups-tab]").exists()).toBe(true);
    expect(wrapper.findAll("[data-backup]")).toHaveLength(1);
    expect(wrapper.get("[data-backup-note]").text()).toContain("avant mise à jour");
    expect(bridge.backups.listed).toEqual([{ serverId: "forge", key: "h_web" }]);
    wrapper.unmount();
  });

  it("n'existe pas pour un service adopté", async () => {
    const { wrapper } = await open(undefined, "e_old");
    expect(wrapper.find("[data-tab='backups']").exists()).toBe(false);
    wrapper.unmount();
  });

  it("n'existe pas quand l'agent n'annonce pas la capacité", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.accounts.setServiceBackupsServed("forge", false),
    );
    expect(wrapper.find("[data-tab='backups']").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("créer une sauvegarde (BR-BACKUP-006 à 012)", () => {
  it("lit l'estimation, envoie la note, suit l'opération et dit l'issue", async () => {
    const { wrapper, bridge } = await open();
    await openTab(wrapper);
    await wrapper.get("[data-backups-save]").trigger("click");
    await flushPromises();
    expect(bridge.backups.previewed).toHaveLength(1);
    expect(document.querySelector("[data-backup-estimate]")).not.toBeNull();
    typeInto(
      document.querySelector("[data-backup-note] input, [data-backup-note] textarea"),
      "ma note",
    );
    await flushPromises();
    await confirmDialog("Sauvegarder");
    expect(bridge.backups.received).toHaveLength(1);
    expect(bridge.backups.received[0]?.request.note).toBe("ma note");
    expect(wrapper.find("[data-backups-running]").exists()).toBe(true);

    const [held] = bridge.backups.running("forge");
    if (!held) throw new Error("aucune sauvegarde en cours");
    bridge.backups.succeed("forge", held.operationId);
    await flushPromises();
    expect(wrapper.find("[data-backups-result]").exists()).toBe(true);
    expect(wrapper.findAll("[data-backup]")).toHaveLength(2);
    await wrapper.get("[data-backups-dismiss]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-backups-result]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("refuse faute de place : la fenêtre le dit et ne lance rien", async () => {
    const { wrapper, bridge } = await open((b) => b.backups.setFree("forge", 1_000_000));
    await openTab(wrapper);
    await wrapper.get("[data-backups-save]").trigger("click");
    await flushPromises();
    expect(document.querySelector("[data-backup-no-space]")).not.toBeNull();
    expect(bridge.backups.received).toHaveLength(0);
    wrapper.unmount();
  });
});

describe("vérifier et supprimer (BR-BACKUP-013, 014, 027)", () => {
  it("vérifier est une consultation, dite dans la ligne", async () => {
    const { wrapper, bridge } = await open();
    await openTab(wrapper);
    await wrapper.get("[data-backup-verify]").trigger("click");
    await flushPromises();
    expect(bridge.backups.verified).toHaveLength(1);
    expect(wrapper.get("[data-backup-verdict]").text()).toContain("intacte");
    wrapper.unmount();
  });

  it("une sauvegarde corrompue est dite corrompue", async () => {
    const { wrapper } = await open((b) => {
      b.backups.verifyResult = "corrupt";
    });
    await openTab(wrapper);
    await wrapper.get("[data-backup-verify]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-backup-verdict]").text()).toContain("corrompue");
    wrapper.unmount();
  });

  it("supprimer redemande toujours le mot de passe, même sous élévation", async () => {
    const { wrapper, bridge } = await open();
    await openTab(wrapper);
    await wrapper.get("[data-backup-remove]").trigger("click");
    await flushPromises();
    // Une première confirmation lève l'élévation ; la suppression redemande quand même le mot de passe.
    await flushPromises();
    expect(reauthField()).not.toBeNull();
    // Sans mot de passe : refus, rien n'est supprimé.
    dialogButton("Supprimer la sauvegarde")?.click();
    await flushPromises();
    expect(bridge.backups.deleted).toHaveLength(0);
    await confirmDialog("Supprimer la sauvegarde", OWN);
    expect(bridge.backups.deleted).toHaveLength(1);
    expect(wrapper.findAll("[data-backup]")).toHaveLength(0);
    wrapper.unmount();
  });

  it("une sauvegarde utilisée par une opération ne se supprime pas", async () => {
    const { wrapper, bridge } = await open((b) => {
      b.backups.deleteInUse = true;
    });
    await openTab(wrapper);
    await wrapper.get("[data-backup-remove]").trigger("click");
    await flushPromises();
    await confirmDialog("Supprimer la sauvegarde", OWN);
    expect(bridge.backups.deleted).toHaveLength(1);
    expect(wrapper.findAll("[data-backup]")).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("la liste en défaut (BR-BACKUP-028)", () => {
  it("dit l'échec de lecture et propose de réessayer", async () => {
    const { wrapper, bridge } = await open((b) => {
      b.backups.failList = true;
    });
    await openTab(wrapper);
    expect(wrapper.find("[data-backups-failed]").exists()).toBe(true);
    bridge.backups.failList = false;
    await wrapper.get("[data-backups-retry]").trigger("click");
    await flushPromises();
    expect(wrapper.findAll("[data-backup]")).toHaveLength(1);
    wrapper.unmount();
  });
});
