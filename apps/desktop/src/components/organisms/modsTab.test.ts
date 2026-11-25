import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { hasKeptBatch } from "@/composables/useServiceMods";
import type { ModItem, ModsView } from "@/link";
import { simMod } from "@/link/simulated-mods";
import type { RightsGate } from "@/services/rights";
import { freshBridge, startedApp } from "@/test/app";
import { service } from "@/test/service";
import ModsTab from "./ModsTab.vue";

// L'onglet « Mods » (HRT-261, HRT-262) contre l'agent simulé : la liste, ses phrases, l'origine, l'interrupteur et le lot.
const ALLOWED: RightsGate = { kind: "allowed" };
const DENIED: RightsGate = { kind: "denied", isAdmin: false, command: null };

function identity(id: string, name: string, version: string, loader = "fabric") {
  return {
    id,
    name,
    version,
    loader,
    minecraftDeclared: true,
    environment: "both" as const,
  };
}

function mod(file: string, patch: Partial<ModItem> = {}): ModItem {
  const base = file.replace(/\.jar(\.disabled)?$/, "");
  return simMod({
    file,
    identity: identity(base, base.toUpperCase(), "1.0"),
    origin: "server_pack",
    ...patch,
  });
}

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  });
});
afterEach(() => vi.useRealTimers());

async function open(
  view: Omit<Partial<ModsView>, "game"> & { game?: Partial<ModsView["game"]> },
  gate = ALLOWED,
) {
  const { bridge } = freshBridge();
  const svc = service({ key: "h_mc", name: "Prominence II", origin: "hearth" });
  bridge.services.seed("forge", [{ key: "h_mc", name: "Prominence II", origin: "hearth" }]);
  bridge.mods.seed("forge", "h_mc", view);
  const wrapper = mount(ModsTab, {
    props: { serverId: "forge", service: svc, gate, rightsReasonId: "why", connected: true },
  });
  await flushPromises();
  return { bridge, wrapper };
}

const rowOf = (wrapper: ReturnType<typeof mount>, file: string) =>
  wrapper.find(`[data-mod-row="${file}"]`);

describe("la liste", () => {
  it("dit nom, version, moteur, état et origine de chaque ligne, avec le titre et la version de Minecraft", async () => {
    const { wrapper } = await open({
      mods: [
        mod("lithium.jar"),
        mod("spark.jar", { origin: "hearth" }),
        mod("carpet.jar.disabled", { origin: "manual" }),
      ],
    });
    expect(wrapper.find("[data-mods-heading]").text()).toBe("3 mods installés, 2 activés");
    expect(wrapper.find("[data-mods-sub]").text()).toBe(
      "2 compatibles avec Fabric et Minecraft 1.20.1, 1 désactivé. Les changements demandent un redémarrage du serveur.",
    );
    expect(wrapper.find("[data-mods-version]").text()).toBe(
      "Version de Minecraft du serveur : 1.20.1 (choisie pour ce serveur).",
    );
    const lithium = rowOf(wrapper, "lithium.jar");
    expect(lithium.text()).toContain("LITHIUM");
    expect(lithium.text()).toContain("Fabric");
    expect(lithium.find("[data-mod-state]").text()).toBe("Compatible");
    expect(lithium.find("[data-mod-origin]").text()).toBe("Pack d'origine");
    expect(rowOf(wrapper, "spark.jar").find("[data-mod-origin]").text()).toBe("Ajouté avec Hearth");
    const carpet = rowOf(wrapper, "carpet.jar.disabled");
    expect(carpet.find("[data-mod-state]").text()).toBe("Désactivé");
    expect(carpet.find("[data-mod-switch]").attributes("aria-checked")).toBe("false");
    expect(wrapper.find("[data-mods-legend]").text()).toContain("Ajouté à la main");
  });

  it("chaque verdict a sa phrase, sans jamais dire une plage de versions", async () => {
    const { wrapper } = await open({
      mods: [
        mod("a.jar", { verdict: { code: "incompatible_minecraft", missing: [], neededBy: [] } }),
        mod("b.jar", { verdict: { code: "incompatible_loader", missing: [], neededBy: [] } }),
        mod("c.jar", { verdict: { code: "to_complete", missing: ["balm"], neededBy: [] } }),
        mod("d.jar", { verdict: { code: "unverified", missing: [], neededBy: [] } }),
        mod("e.jar", {
          verdict: { code: "unrecognized", missing: [], neededBy: [] },
          identity: null,
        }),
      ],
    });
    const say = (file: string) => rowOf(wrapper, file).find("[data-mod-sentence]").text();
    expect(say("a.jar")).toBe("Ce mod n'est pas compatible avec Minecraft 1.20.1.");
    expect(say("b.jar")).toBe("Ce mod n'est pas compatible avec Fabric.");
    expect(say("c.jar")).toBe("Ce mod a besoin du mod Balm, qui n'est pas installé.");
    expect(say("d.jar")).toContain("Hearth ne sait pas");
    expect(say("e.jar")).toBe("Hearth ne lit pas la fiche d'identité de ce fichier.");
    expect(rowOf(wrapper, "a.jar").find("[data-mod-state]").text()).toBe("Pas compatible");
    expect(rowOf(wrapper, "c.jar").find("[data-mod-state]").text()).toBe("À compléter");
    expect(rowOf(wrapper, "e.jar").find("[data-mod-state]").text()).toBe("Non reconnu");
    expect(wrapper.text()).not.toMatch(/1\.21|\[1\./);
  });

  it("le bandeau ne parle que d'un mod incompatible ACTIVÉ, ou installé deux fois", async () => {
    const { wrapper } = await open({
      mods: [
        mod("bad.jar", { verdict: { code: "incompatible_minecraft", missing: [], neededBy: [] } }),
        mod("off.jar.disabled", {
          verdict: { code: "incompatible_minecraft", missing: [], neededBy: [] },
        }),
        mod("twin.jar", { verdict: { code: "duplicate", missing: [], neededBy: [] } }),
      ],
    });
    const banners = wrapper.findAll("[data-mods-banner]").map((banner) => banner.text());
    expect(banners).toHaveLength(2);
    expect(banners[0]).toContain("BAD ne peut pas démarrer ici");
    expect(banners[1]).toContain("installé deux fois");
  });

  it("la version inconnue, « dernière version à chaque démarrage » et le pack Modrinth sont dits", async () => {
    const { wrapper } = await open({
      mods: [mod("a.jar")],
      game: {
        minecraft: null,
        minecraftFrom: "unknown",
        updatesEveryStart: true,
        pack: "modrinth_pack",
      },
    });
    expect(wrapper.find("[data-mods-version]").text()).toContain("démarrez-le une fois");
    const notes = wrapper.findAll("[data-mods-note]").map((note) => note.text());
    expect(notes).toHaveLength(2);
    expect(notes[0]).toContain("dernière version de Minecraft à chaque démarrage");
    expect(notes[1]).toContain("pack Modrinth reviennent à chaque démarrage");
  });

  it("un mod retiré est dans « Mods retirés » avec « Remettre »", async () => {
    const { wrapper } = await open({ mods: [mod("a.jar"), mod("old.jar", { place: "retired" })] });
    const retired = wrapper.find("[data-mods-retired]");
    expect(retired.text()).toContain("Mods retirés");
    expect(rowOf(wrapper, "old.jar").find("[data-mod-state]").text()).toBe("Retiré");
    expect(rowOf(wrapper, "old.jar").find("[data-mod-restore]").exists()).toBe(true);
    expect(wrapper.find("[data-mods-heading]").text()).toBe("1 mod installé, 1 activé");
  });

  it("un nom de fichier illisible se montre par son rang et ne se manipule pas", async () => {
    const { wrapper } = await open({
      mods: [simMod({ file: "x.jar", readableName: false, identity: null })].map((item) => ({
        ...item,
        file: null,
      })),
    });
    const row = wrapper.find("[data-mod-row='rang-0']");
    expect(row.text()).toContain("Fichier au nom illisible n° 1");
    expect(row.find("[data-mod-switch]").attributes("aria-disabled")).toBe("true");
    expect(row.find("[data-mod-reason]").text()).toContain("ne manipule pas");
  });
});

describe("les moteurs sans mods et les lectures", () => {
  it("Vanilla : « Ce serveur n'a pas de mods » et « Ajouter un serveur de jeu »", async () => {
    const { wrapper } = await open({ applicable: "no_mods" });
    expect(wrapper.find("[data-mods-none]").text()).toContain("Ce serveur n'a pas de mods");
    await wrapper.find("[data-mods-add-server]").trigger("click");
    expect(wrapper.emitted("addServer")).toHaveLength(1);
  });

  it("Paper : la phrase des plugins, sans « Ajouter un serveur de jeu »", async () => {
    const { wrapper } = await open({ applicable: "plugins" });
    expect(wrapper.find("[data-mods-none]").text()).toContain("plugins, pas des mods");
    expect(wrapper.find("[data-mods-add-server]").exists()).toBe(false);
  });

  it("un gros serveur : « Lecture des mods en cours » puis relecture automatique jusqu'à tout lire", async () => {
    const { bridge, wrapper } = await open({ mods: [mod("a.jar")], unread: 4 });
    bridge.mods.unreadSteps = [2, 0];
    expect(wrapper.find("[data-mods-reading]").text()).toContain("Lecture des mods en cours");
    expect(wrapper.find("[data-mods-reading]").text()).toContain("4 mods ne sont pas encore lus");
    vi.advanceTimersByTime(3_000);
    await flushPromises();
    expect(wrapper.find("[data-mods-reading]").text()).toContain("2 mods");
    vi.advanceTimersByTime(3_000);
    await flushPromises();
    expect(wrapper.find("[data-mods-reading]").exists()).toBe(false);
    expect(bridge.mods.listed).toHaveLength(3);
    vi.advanceTimersByTime(30_000);
    await flushPromises();
    expect(bridge.mods.listed).toHaveLength(3);
  });

  it("une lecture qui échoue le dit et propose de réessayer", async () => {
    const { bridge } = await startedApp();
    const svc = service({ key: "h_mc", name: "Prominence II", origin: "hearth" });
    bridge.services.seed("forge", [{ key: "h_mc", name: "Prominence II", origin: "hearth" }]);
    bridge.mods.seed("forge", "h_mc", { mods: [mod("a.jar")] });
    bridge.mods.failList = true;
    const wrapper = mount(ModsTab, {
      props: {
        serverId: "forge",
        service: svc,
        gate: ALLOWED,
        rightsReasonId: "why",
        connected: true,
      },
    });
    await flushPromises();
    expect(wrapper.find("[data-mods-failed]").text()).toContain("pas pu lire les mods");
    bridge.mods.failList = false;
    await wrapper.find("[data-mods-retry]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-mods-table]").exists()).toBe(true);
  });
});

describe("l'interrupteur, « Retirer » et le lot", () => {
  it("désactiver met le geste dans le lot : la ligne le dit, « Annuler » le sort", async () => {
    const { bridge, wrapper } = await open({ mods: [mod("a.jar"), mod("b.jar")] });
    await rowOf(wrapper, "a.jar").find("[data-mod-switch]").trigger("click");
    const row = rowOf(wrapper, "a.jar");
    expect(row.find("[data-mod-pending-text]").text()).toBe(
      "Sera désactivé au prochain redémarrage",
    );
    expect(row.find("[data-mod-switch]").attributes("aria-checked")).toBe("false");
    expect(hasKeptBatch("forge", "h_mc")).toBe(true);
    await row.find("[data-mod-undo]").trigger("click");
    expect(rowOf(wrapper, "a.jar").find("[data-mod-pending-text]").exists()).toBe(false);
    expect(hasKeptBatch("forge", "h_mc")).toBe(false);
    // Rien n'est écrit sur le serveur : aucun lot reçu.
    expect(bridge.mods.received).toHaveLength(0);
  });

  it("retirer et remettre entrent dans le lot ; un fichier n'est que dans un seul geste", async () => {
    const { wrapper } = await open({ mods: [mod("a.jar"), mod("old.jar", { place: "retired" })] });
    await rowOf(wrapper, "a.jar").find("[data-mod-switch]").trigger("click");
    await rowOf(wrapper, "a.jar").find("[data-mod-undo]").trigger("click");
    await rowOf(wrapper, "a.jar").find("[data-mod-retire]").trigger("click");
    expect(rowOf(wrapper, "a.jar").find("[data-mod-pending-text]").text()).toBe(
      "Sera mis de côté au prochain redémarrage",
    );
    await rowOf(wrapper, "old.jar").find("[data-mod-restore]").trigger("click");
    expect(rowOf(wrapper, "old.jar").find("[data-mod-pending-text]").text()).toBe(
      "Sera remis au prochain redémarrage",
    );
  });

  it("désactiver un mod dont un autre a besoin le DIT, sans bloquer", async () => {
    const { wrapper } = await open({
      mods: [
        mod("lib.jar", { verdict: { code: "compatible", missing: [], neededBy: ["app"] } }),
        mod("app.jar"),
      ],
    });
    await rowOf(wrapper, "lib.jar").find("[data-mod-switch]").trigger("click");
    expect(rowOf(wrapper, "lib.jar").find("[data-mod-note]").text()).toBe(
      "Le mod APP a besoin de celui-ci.",
    );
    expect(rowOf(wrapper, "lib.jar").find("[data-mod-pending-text]").exists()).toBe(true);
  });

  it("réactiver un mod dont une autre version est active est refusé avant l'envoi", async () => {
    const { wrapper } = await open({
      mods: [
        mod("carpet-1.jar", { identity: identity("carpet", "Carpet", "1.0") }),
        mod("carpet-2.jar.disabled", { identity: identity("carpet", "Carpet", "2.0") }),
      ],
    });
    await rowOf(wrapper, "carpet-2.jar.disabled").find("[data-mod-switch]").trigger("click");
    const row = rowOf(wrapper, "carpet-2.jar.disabled");
    expect(row.find("[data-mod-note]").text()).toBe(
      "Ce mod est déjà actif dans une autre version : désactivez-la d'abord.",
    );
    expect(row.find("[data-mod-pending-text]").exists()).toBe(false);
  });

  it("un mod du pack Modrinth a l'interrupteur et « Retirer » inactifs, avec la raison", async () => {
    const { wrapper } = await open({ mods: [mod("pack.jar", { origin: "modrinth_pack" })] });
    const row = rowOf(wrapper, "pack.jar");
    expect(row.find("[data-mod-switch]").attributes("aria-disabled")).toBe("true");
    expect(row.find("[data-mod-retire]").attributes("aria-disabled")).toBe("true");
    expect(row.find("[data-mod-reason]").text()).toContain("pack Modrinth");
    await row.find("[data-mod-switch]").trigger("click");
    expect(wrapper.find("[data-mod-pending-text]").exists()).toBe(false);
  });

  it("sans le droit de gérer les services, les gestes sont inactifs et ne préparent rien", async () => {
    const { wrapper } = await open({ mods: [mod("a.jar")] }, DENIED);
    const row = rowOf(wrapper, "a.jar");
    expect(row.find("[data-mod-switch]").attributes("aria-disabled")).toBe("true");
    expect(row.find("[data-mod-switch]").attributes("aria-describedby")).toBe("why");
    expect(row.find("[data-mod-retire]").attributes("aria-describedby")).toBe("why");
    await row.find("[data-mod-retire]").trigger("click");
    expect(wrapper.find("[data-mod-pending-text]").exists()).toBe(false);
  });

  it("une relecture écarte un geste qui vise un fichier qui n'existe plus", async () => {
    const { bridge, wrapper } = await open({ mods: [mod("a.jar"), mod("b.jar")] });
    await rowOf(wrapper, "a.jar").find("[data-mod-retire]").trigger("click");
    bridge.mods.viewOf("forge", "h_mc")?.mods.shift();
    // Un lien qui revient relit la liste.
    await wrapper.setProps({ connected: false });
    await wrapper.setProps({ connected: true });
    await flushPromises();
    expect(rowOf(wrapper, "a.jar").exists()).toBe(false);
    expect(hasKeptBatch("forge", "h_mc")).toBe(false);
  });
});
