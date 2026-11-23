import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackupMods, ModIdentity, SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { mountContext } from "@/test/mount";

// La colonne « Mods » de l'onglet Sauvegardes et la ligne dépliée (HRT-294, BR-MOD-034 à 036, maquette Sauvegardes) : la
// liste enregistrée dans chaque sauvegarde, sa comparaison avec les mods actuels (calculée par l'agent) et « Copier la liste ».
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

const DAY = "2026-11-17T20:00:00Z";

function identity(id: string, name: string, version: string, environment = "both"): ModIdentity {
  return {
    id,
    name,
    version,
    loader: "fabric",
    minecraftDeclared: true,
    environment: environment as ModIdentity["environment"],
  };
}

function listOf(patch: Partial<BackupMods> = {}): BackupMods {
  return {
    state: "recorded",
    recordedAt: DAY,
    game: { loader: "fabric", minecraft: "1.20.1" },
    counts: { active: 3, disabled: 1, retired: 1, notListed: 0 },
    mods: [
      {
        file: "sodium.jar",
        rank: 0,
        place: "active",
        origin: "hearth",
        identity: identity("sodium", "Sodium", "0.5.3"),
      },
      {
        file: "carpet.jar.disabled",
        rank: 1,
        place: "disabled",
        origin: "manual",
        identity: identity("carpet", "Carpet", "1.4", "server"),
      },
      {
        file: "iris.jar",
        rank: 2,
        place: "active",
        origin: "server_pack",
        identity: identity("iris", "Iris", "1.6"),
      },
      { file: null, rank: 3, place: "active", origin: "manual", identity: null },
      {
        file: "old.jar",
        rank: 4,
        place: "retired",
        origin: "manual",
        identity: identity("old", "Old", "0.1"),
      },
    ],
    compare: null,
    ...patch,
  };
}

async function open(setup?: (bridge: SimulatedLinkBridge) => void) {
  const ctx = await mountContext();
  const mounts = [
    { kind: "folder" as const, source: "/srv/hearth/data/mc", target: "/data", readOnly: false },
  ];
  ctx.bridge.services.seed("forge", [
    { key: "h_mc", name: "Prominence", origin: "hearth", mounts },
    { key: "h_plain", name: "Terraria", origin: "hearth", mounts },
  ]);
  ctx.bridge.backups.seed("forge", "h_mc", [
    {
      id: "01SIMBKAAAAAAA",
      createdAt: DAY,
      mods: { state: "recorded", active: 3, disabled: 1, retired: 1 },
    },
    {
      id: "01SIMBKBBBBBBB",
      createdAt: "2026-11-10T10:00:00Z",
      mods: { state: "not_recorded", active: 0, disabled: 0, retired: 0 },
    },
    {
      id: "01SIMBKCCCCCCC",
      createdAt: "2026-11-09T10:00:00Z",
      mods: { state: "unreadable", active: 0, disabled: 0, retired: 0 },
    },
    {
      id: "01SIMBKDDDDDDD",
      createdAt: "2026-11-08T10:00:00Z",
      mods: { state: "not_in_backup", active: 0, disabled: 0, retired: 0 },
    },
  ]);
  ctx.bridge.backups.seed("forge", "h_plain", [{ id: "01SIMBKEEEEEEE", createdAt: DAY }]);
  ctx.bridge.mods.seedBackupMods("forge", "h_mc", "01SIMBKAAAAAAA", listOf());
  for (const [id, state] of [
    ["01SIMBKBBBBBBB", "not_recorded"],
    ["01SIMBKCCCCCCC", "unreadable"],
    ["01SIMBKDDDDDDD", "not_in_backup"],
  ] as const) {
    ctx.bridge.mods.seedBackupMods(
      "forge",
      "h_mc",
      id,
      listOf({ state, mods: [], counts: { active: 0, disabled: 0, retired: 0, notListed: 0 } }),
    );
  }
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

async function openTab(wrapper: ReturnType<typeof mount>, key: string) {
  await wrapper.get(`[data-row][data-key="${key}"]`).trigger("click");
  await flushPromises();
  await wrapper.get("[data-tab='backups']").trigger("click");
  await flushPromises();
}

const rowOf = (wrapper: ReturnType<typeof mount>, id: string) =>
  wrapper.get(`[data-backup="${id}"]`);

describe("la colonne « Mods »", () => {
  it("dit « 3 actifs, 1 désactivé » ou un mot court quand il n'y a pas de liste", async () => {
    const { wrapper } = await open();
    await openTab(wrapper, "h_mc");
    const text = (id: string) => rowOf(wrapper, id).get("[data-backup-mods-column]").text();
    expect(text("01SIMBKAAAAAAA")).toBe("3 actifs, 1 désactivé");
    expect(text("01SIMBKBBBBBBB")).toBe("Non enregistrée");
    expect(text("01SIMBKCCCCCCC")).toBe("Illisible");
    expect(text("01SIMBKDDDDDDD")).toBe("Non sauvegardés");
    wrapper.unmount();
  });

  it("n'existe pas pour un serveur sans mods", async () => {
    const { wrapper } = await open();
    await openTab(wrapper, "h_plain");
    expect(wrapper.find("[data-backup-mods-column]").exists()).toBe(false);
    await rowOf(wrapper, "01SIMBKEEEEEEE").get("[data-backup-toggle]").trigger("click");
    expect(wrapper.find("[data-backup-mods]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("la ligne dépliée", () => {
  it("dit le moteur et la version, la liste triée par nom, les mods retirés à part et le rang d'un nom illisible", async () => {
    const { wrapper, bridge } = await open();
    await openTab(wrapper, "h_mc");
    await rowOf(wrapper, "01SIMBKAAAAAAA").get("[data-backup-toggle]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-backup-mods-engine]").text()).toBe(
      "Au moment de la sauvegarde : Fabric, Minecraft 1.20.1.",
    );
    const names = wrapper.findAll("[data-backup-mods-row] strong").map((el) => el.text());
    expect(names).toEqual(["Carpet", "Iris", "Non reconnu dans cette sauvegarde", "Sodium"]);
    expect(wrapper.get("[data-backup-mods-retired]").text()).toBe("Mods retirés : 1");
    expect(wrapper.get("[data-backup-mods-table]").text()).toContain(
      "Fichier au nom illisible n° 4",
    );
    expect(bridge.mods.backupReads).toEqual([
      { serverId: "forge", backupId: "01SIMBKAAAAAAA", compare: false },
    ]);
    wrapper.unmount();
  });

  it("une sauvegarde sans liste dit pourquoi, sans rien reconstituer", async () => {
    const { wrapper } = await open();
    await openTab(wrapper, "h_mc");
    for (const [id, phrase] of [
      ["01SIMBKBBBBBBB", "faite avant que Hearth enregistre les mods"],
      ["01SIMBKCCCCCCC", "illisible pour cette sauvegarde"],
      ["01SIMBKDDDDDDD", "le dossier mods est ignoré"],
    ] as const) {
      await rowOf(wrapper, id).get("[data-backup-toggle]").trigger("click");
      await flushPromises();
      expect(wrapper.get("[data-backup-mods-sentence]").text()).toContain(phrase);
      expect(wrapper.find("[data-backup-mods-table]").exists()).toBe(false);
      await rowOf(wrapper, id).get("[data-backup-toggle]").trigger("click");
    }
    wrapper.unmount();
  });

  it("« Copier la liste » copie les mods activés au format de la liste des joueurs, sous le titre de la sauvegarde", async () => {
    const written: string[] = [];
    vi.stubGlobal("navigator", {
      clipboard: { writeText: async (text: string) => void written.push(text) },
    });
    const { wrapper } = await open();
    await openTab(wrapper, "h_mc");
    await rowOf(wrapper, "01SIMBKAAAAAAA").get("[data-backup-toggle]").trigger("click");
    await flushPromises();
    await wrapper.get("[data-backup-mods-copy]").trigger("click");
    await flushPromises();
    const lines = (written[0] ?? "").split("\n");
    expect(lines[0]).toMatch(/^Mods de la sauvegarde du /);
    expect(lines.slice(1)).toEqual(
      ["Iris 1.6 (Fabric, Minecraft 1.20.1)", "Sodium 0.5.3 (Fabric, Minecraft 1.20.1)"].filter(
        (line) => lines.includes(line),
      ),
    );
    expect(written[0]).not.toContain("Carpet");
    expect(written[0]).not.toContain("Old");
    wrapper.unmount();
  });
});

describe("la comparaison avec les mods actuels", () => {
  it("regroupe les changements par genre, et dit quand rien n'a changé", async () => {
    const { wrapper, bridge } = await open();
    bridge.mods.seedBackupMods(
      "forge",
      "h_mc",
      "01SIMBKAAAAAAA",
      listOf({
        compare: {
          complete: true,
          unread: 0,
          same: false,
          changes: [
            {
              kind: "added",
              id: "chunky",
              name: "Chunky",
              file: null,
              backupVersion: null,
              currentVersion: "1.3",
            },
            {
              kind: "version_changed",
              id: "iris",
              name: "Iris",
              file: null,
              backupVersion: "1.6",
              currentVersion: "1.7",
            },
            {
              kind: "disabled",
              id: "sodium",
              name: "Sodium",
              file: null,
              backupVersion: null,
              currentVersion: null,
            },
          ],
        },
      }),
    );
    await openTab(wrapper, "h_mc");
    await rowOf(wrapper, "01SIMBKAAAAAAA").get("[data-backup-toggle]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-backup-compare]").exists()).toBe(false);
    await wrapper.get("[data-backup-mods-compare]").trigger("click");
    await flushPromises();
    const groups = wrapper
      .findAll("[data-backup-compare-group]")
      .map((el) => el.attributes("data-backup-compare-group"));
    expect(groups).toEqual(["added", "version_changed", "disabled"]);
    expect(wrapper.get("[data-backup-compare-group='version_changed']").text()).toContain(
      "Iris : 1.6 dans la sauvegarde, 1.7 aujourd'hui",
    );
    expect(bridge.mods.backupReads.at(-1)).toMatchObject({ compare: true });
    wrapper.unmount();
  });

  it("rien de changé : la phrase dite ; comparaison partielle : dite et relue seule jusqu'à la fin, plus après le repli", async () => {
    const { wrapper, bridge } = await open();
    const partial = listOf({ compare: { complete: false, unread: 3, same: false, changes: [] } });
    bridge.mods.seedBackupMods("forge", "h_mc", "01SIMBKAAAAAAA", partial);
    await openTab(wrapper, "h_mc");
    await rowOf(wrapper, "01SIMBKAAAAAAA").get("[data-backup-toggle]").trigger("click");
    await flushPromises();
    await wrapper.get("[data-backup-mods-compare]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-backup-compare-partial]").text()).toBe(
      "Comparaison partielle : 3 mods ne sont pas encore lus.",
    );
    bridge.mods.seedBackupMods(
      "forge",
      "h_mc",
      "01SIMBKAAAAAAA",
      listOf({ compare: { complete: true, unread: 0, same: true, changes: [] } }),
    );
    vi.advanceTimersByTime(3_000);
    await flushPromises();
    expect(wrapper.get("[data-backup-compare-same]").text()).toBe(
      "Les mods actuels sont les mêmes que dans cette sauvegarde.",
    );
    const reads = bridge.mods.backupReads.length;
    vi.advanceTimersByTime(30_000);
    await flushPromises();
    expect(bridge.mods.backupReads).toHaveLength(reads);
    wrapper.unmount();
  });

  it("replier la ligne arrête la relecture d'une comparaison partielle", async () => {
    const { wrapper, bridge } = await open();
    bridge.mods.seedBackupMods(
      "forge",
      "h_mc",
      "01SIMBKAAAAAAA",
      listOf({ compare: { complete: false, unread: 2, same: false, changes: [] } }),
    );
    await openTab(wrapper, "h_mc");
    await rowOf(wrapper, "01SIMBKAAAAAAA").get("[data-backup-toggle]").trigger("click");
    await flushPromises();
    await wrapper.get("[data-backup-mods-compare]").trigger("click");
    await flushPromises();
    await rowOf(wrapper, "01SIMBKAAAAAAA").get("[data-backup-toggle]").trigger("click");
    const reads = bridge.mods.backupReads.length;
    vi.advanceTimersByTime(30_000);
    await flushPromises();
    expect(bridge.mods.backupReads).toHaveLength(reads);
    wrapper.unmount();
  });
});
