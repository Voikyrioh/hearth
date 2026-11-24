import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { ModCheckFile, ModsGame, ModsView } from "@/link";
import { simMod } from "@/link/simulated-mods";
import { freshBridge } from "@/test/app";
import ModsCheckList from "./ModsCheckList.vue";
import ModsPlayersList from "./ModsPlayersList.vue";

// Les retours de la troisième revue de Nora sur les mods (HRT-292 : M1, M7, m1, m6).
const GAME: ModsGame = {
  loader: "fabric",
  minecraft: "1.20.1",
  minecraftFrom: "setting",
  updatesEveryStart: false,
  pack: "none",
};

function identity(id: string, name: string) {
  return {
    id,
    name,
    version: "1.0",
    loader: "fabric",
    minecraftDeclared: true,
    environment: "both" as const,
  };
}

describe("ce que vos joueurs doivent installer : M1", () => {
  it("un mod activé mais pas compatible n'est pas omis en silence : son nom et la raison sont dits", () => {
    const view: ModsView = {
      applicable: "yes",
      game: GAME,
      unread: 0,
      truncated: false,
      mods: [
        simMod({ file: "lithium.jar", identity: identity("lithium", "Lithium") }),
        simMod({
          file: "create.jar",
          identity: identity("create", "Create"),
          verdict: { code: "incompatible_minecraft", missing: [], neededBy: [] },
        }),
      ],
    } as unknown as ModsView;
    freshBridge();
    const wrapper = mount(ModsPlayersList, { props: { view, address: null } });
    const rows = wrapper.findAll("[data-mods-players-row]");
    expect(rows).toHaveLength(1);
    const note = wrapper.get("[data-mods-players-unlisted]").text();
    expect(note).toContain("Create");
    expect(note).toContain("pas compatible avec Minecraft 1.20.1");
    expect(wrapper.get("[data-mods-players-table]").text()).not.toContain("Create");
  });
});

describe("le contrôle des fichiers déposés : m6, M7", () => {
  const refused: ModCheckFile = {
    deposit: "d1",
    outcome: "refused",
    identity: identity("create", "Create"),
    targetFile: null,
    replaces: null,
    verdict: null,
    refusal: "MOD_INCOMPATIBLE",
    refusalValues: { loader: "forge", serverLoader: "fabric", serverMinecraft: "1.20.1" },
  } as unknown as ModCheckFile;

  it("sans fichier retenu, « Continuer avec 0 fichier » n'est pas proposé ; le mot est « Pas compatible »", () => {
    const wrapper = mount(ModsCheckList, {
      props: {
        files: [refused],
        game: GAME,
        nameOf: () => "create.jar",
        shaOf: () => null,
        room: 8,
      },
    });
    expect(wrapper.find("[data-mods-check-continue]").exists()).toBe(false);
    expect(wrapper.find("[data-mods-check-back]").exists()).toBe(true);
    expect(wrapper.get("[data-mods-check-pill]").text()).toBe("Pas compatible");
    expect(wrapper.text()).not.toContain("Continuer avec 0");
  });
});

describe("la sauvegarde « Avant mods » : m1", () => {
  it("est faite après l'arrêt (jamais « à chaud ») et porte la liste des mods d'avant", () => {
    const { bridge } = freshBridge();
    const backups = bridge.backups;
    backups.addModsBackup("forge", "h_mc", "b1", "Avant mods, 24/11/2026", {
      state: "recorded",
      active: 3,
      disabled: 1,
      retired: 0,
    });
    const entry = [...backups["lists"].values()][0]?.[0];
    expect(entry?.mode).toBe("stop_copy_start");
    expect(entry?.flags).toEqual([]);
    expect(entry?.mods).toEqual({ state: "recorded", active: 3, disabled: 1, retired: 0 });
  });
});
