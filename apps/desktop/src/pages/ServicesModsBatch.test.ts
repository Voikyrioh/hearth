import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { simMod } from "@/link/simulated-mods";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useTransfersStore } from "@/stores/transfers";
import { confirmDialog } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// Ajouter, désactiver et retirer des mods (HRT-262, BR-MOD-022 à 024) : choisir des fichiers, les voir partir, lire le
// contrôle, décider des remplacements, et préparer le lot. Rien n'est écrit avant « Appliquer » (HRT-263).
beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
  useServicesStore().reset();
  useServiceOpsStore().reset();
  useServiceRightsStore().reset();
  useTransfersStore().reset();
});

function identity(id: string, name: string, version: string, loader = "fabric") {
  return { id, name, version, loader, minecraftDeclared: true, environment: "both" as const };
}

async function open(setup?: (bridge: SimulatedLinkBridge) => void) {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [{ key: "h_mc", name: "Prominence", origin: "hearth" }]);
  ctx.bridge.games.register("forge", "h_mc", { name: "Minecraft", fiche: "essai" });
  const fiche = ctx.bridge.catalog.addRich();
  fiche.mods?.ways.push("drop_jars");
  ctx.bridge.catalog.put(fiche);
  ctx.bridge.mods.seed("forge", "h_mc", {
    mods: [
      simMod({
        file: "lithium-0.11.2.jar",
        origin: "server_pack",
        identity: identity("lithium", "Lithium", "0.11.2"),
      }),
    ],
  });
  ctx.bridge.mods.willRead("chunky-fabric-1.3.146.jar", {
    identity: identity("chunky", "Chunky", "1.3.146"),
  });
  ctx.bridge.mods.willRead("lithium-fabric-mc1.20.1-0.11.4.jar", {
    identity: identity("lithium", "Lithium", "0.11.4"),
  });
  ctx.bridge.mods.willRead("create-1.20.1-0.5.1.f.jar", {
    identity: identity("create", "Create", "0.5.1", "forge"),
  });
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  await wrapper.get('[data-row][data-key="h_mc"]').trigger("click");
  await flushPromises();
  await wrapper.get("[data-tab='mods']").trigger("click");
  await flushPromises();
  return { ...ctx, wrapper };
}

/** Choisit des fichiers, confirme l'envoi (mot de passe) et finit tous les envois. */
async function deposit(ctx: Awaited<ReturnType<typeof open>>, paths: string[]) {
  ctx.bridge.transfers.pickedMany = paths;
  await ctx.wrapper.get("[data-mods-add]").trigger("click");
  await ctx.wrapper.get("[data-mods-deposit-choose]").trigger("click");
  await flushPromises();
  await confirmDialog("Envoyer");
  await flushPromises();
  for (const transfer of ctx.bridge.transfers.list()) ctx.bridge.transfers.complete(transfer.id);
  await useTransfersStore().refresh();
  await flushPromises();
}

describe("déposer des fichiers", () => {
  it("3 fichiers : envois suivis, puis « Contrôler les fichiers » lit un verdict par fichier", async () => {
    const ctx = await open();
    await deposit(ctx, [
      "C:\\dl\\chunky-fabric-1.3.146.jar",
      "C:\\dl\\lithium-fabric-mc1.20.1-0.11.4.jar",
      "C:\\dl\\create-1.20.1-0.5.1.f.jar",
    ]);
    const { wrapper } = ctx;
    expect(wrapper.findAll("[data-mods-deposit-file]")).toHaveLength(3);
    expect(wrapper.findAll("[data-mods-deposit-status]").map((el) => el.text())).toEqual([
      "Envoyé",
      "Envoyé",
      "Envoyé",
    ]);
    await wrapper.get("[data-mods-deposit-check]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-mods-check-title]").text()).toBe(
      "Contrôle terminé : 2 fichiers prêts, 1 fichier à revoir",
    );
    const [chunky, lithium, create] = wrapper.findAll("[data-mods-check-file]");
    expect(chunky?.attributes("data-outcome")).toBe("add");
    expect(lithium?.attributes("data-outcome")).toBe("replace");
    expect(create?.attributes("data-outcome")).toBe("refused");
    expect(create?.get("[data-mods-check-sentence]").text()).toContain("fait pour Forge");
    expect(create?.text()).not.toMatch(/1\.\d+\.\d+ à|\[1\./);
    wrapper.unmount();
  });

  it("une autre version du mod déposé dans « Mods retirés » est dite au contrôle", async () => {
    const ctx = await open((bridge) => {
      bridge.mods.seed("forge", "h_mc", {
        mods: [
          simMod({
            file: "chunky-fabric-1.2.0.jar",
            place: "retired",
            origin: "hearth",
            identity: identity("chunky", "Chunky", "1.2.0"),
          }),
        ],
      });
      bridge.mods.willRead("chunky-fabric-1.3.146.jar", {
        identity: identity("chunky", "Chunky", "1.3.146"),
      });
    });
    await deposit(ctx, ["C:\\dl\\chunky-fabric-1.3.146.jar"]);
    const { wrapper } = ctx;
    await wrapper.get("[data-mods-deposit-check]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-mods-check-retired]").text()).toContain(
      "Une autre version de ce mod (1.2.0) est dans « Mods retirés »",
    );
    wrapper.unmount();
  });

  it("la case « Remplacer… » est décochée d'office ; non cochée, le fichier est laissé de côté", async () => {
    const ctx = await open();
    await deposit(ctx, [
      "C:\\dl\\chunky-fabric-1.3.146.jar",
      "C:\\dl\\lithium-fabric-mc1.20.1-0.11.4.jar",
    ]);
    const { wrapper } = ctx;
    await wrapper.get("[data-mods-deposit-check]").trigger("click");
    await flushPromises();
    const box = wrapper.get("[data-mods-check-replace] input");
    expect((box.element as HTMLInputElement).checked).toBe(false);
    expect(wrapper.get("[data-mods-check-continue]").text()).toBe("Continuer avec 1 fichier");
    await wrapper.get("[data-mods-check-continue]").trigger("click");
    await flushPromises();
    expect(wrapper.findAll("[data-mods-adds] [data-mod-row]")).toHaveLength(1);
    expect(wrapper.get("[data-mods-adds]").text()).toContain("Sera ajouté au prochain redémarrage");
    expect(wrapper.get("[data-mods-pending-bar]").text()).toContain("1 changement prêt");
    wrapper.unmount();
  });

  it("cochée, la case met le remplacement dans le lot et l'ancienne ligne dit par quoi elle sera remplacée", async () => {
    const ctx = await open();
    await deposit(ctx, ["C:\\dl\\lithium-fabric-mc1.20.1-0.11.4.jar"]);
    const { wrapper, bridge } = ctx;
    await wrapper.get("[data-mods-deposit-check]").trigger("click");
    await flushPromises();
    await wrapper.get("[data-mods-check-replace] input").setValue(true);
    await wrapper.get("[data-mods-check-continue]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-mods-adds]").text()).toContain("Remplacera lithium-0.11.2.jar");
    expect(
      wrapper.get("[data-mod-row='lithium-0.11.2.jar'] [data-mod-pending-text]").text(),
    ).toContain("Sera remplacé par la version 0.11.4");
    // Rien n'est écrit sur le serveur avant « Appliquer ».
    expect(bridge.mods.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("un fichier qui n'est pas un .jar est refusé avant l'envoi, un neuvième aussi", async () => {
    const ctx = await open();
    const nine = Array.from({ length: 9 }, (_, i) => `C:\\dl\\mod-${i}.jar`);
    await deposit(ctx, ["C:\\dl\\notes.txt", ...nine]);
    const statuses = ctx.wrapper.findAll("[data-mods-deposit-status]").map((el) => el.text());
    expect(statuses[0]).toContain("n'est pas un fichier .jar");
    expect(statuses.at(-1)).toContain("8 fichiers au plus");
    expect(ctx.bridge.transfers.list()).toHaveLength(8);
    ctx.wrapper.unmount();
  });

  it("« Contrôler les fichiers » reste inactif tant qu'un envoi court", async () => {
    const ctx = await open();
    ctx.bridge.transfers.pickedMany = ["C:\\dl\\chunky-fabric-1.3.146.jar"];
    await ctx.wrapper.get("[data-mods-add]").trigger("click");
    await ctx.wrapper.get("[data-mods-deposit-choose]").trigger("click");
    await flushPromises();
    await confirmDialog("Envoyer");
    await flushPromises();
    const button = ctx.wrapper.get("[data-mods-deposit-check]");
    expect(button.attributes("aria-disabled")).toBe("true");
    ctx.wrapper.unmount();
  });
});

describe("le lot", () => {
  it("« Tout annuler » vide le lot ; la barre compte les gestes et les ajouts", async () => {
    const ctx = await open();
    const { wrapper } = ctx;
    await wrapper.get("[data-mod-row='lithium-0.11.2.jar'] [data-mod-switch]").trigger("click");
    expect(wrapper.get("[data-mods-pending-bar]").text()).toContain("1 changement prêt");
    await deposit(ctx, ["C:\\dl\\chunky-fabric-1.3.146.jar"]);
    await wrapper.get("[data-mods-deposit-check]").trigger("click");
    await flushPromises();
    await wrapper.get("[data-mods-check-continue]").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-mods-pending-bar]").text()).toContain("2 changements prêts");
    await wrapper.get("[data-mods-cancel-all]").trigger("click");
    expect(wrapper.find("[data-mods-pending-bar]").exists()).toBe(false);
    expect(wrapper.find("[data-mod-pending-text]").exists()).toBe(false);
    wrapper.unmount();
  });
});
