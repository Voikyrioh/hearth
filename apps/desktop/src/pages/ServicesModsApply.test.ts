import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { simConsoleView } from "@/link/simulated-console";
import { simMod } from "@/link/simulated-mods";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceOpsStore } from "@/stores/serviceOps";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { useTransfersStore } from "@/stores/transfers";
import { confirmDialog, OWN, reauthField, typeInto } from "@/test/confirm";
import { mountContext } from "@/test/mount";

// Appliquer un lot de mods (HRT-263, BR-MOD-025 à 027, 031) : « Vérifiez avant d'appliquer », le suivi, la fin et l'échec
// revenu à l'état d'avant, sur l'agent simulé qui juge comme le vrai.
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

function identity(id: string, name: string, version: string) {
  return {
    id,
    name,
    version,
    loader: "fabric",
    minecraftDeclared: true,
    environment: "both" as const,
  };
}

async function open(
  setup?: (bridge: SimulatedLinkBridge) => void,
  state: "running" | "stopped" = "running",
) {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [{ key: "h_mc", name: "Prominence", origin: "hearth", state }]);
  ctx.bridge.games.register("forge", "h_mc", { name: "Minecraft", fiche: "essai" });
  const fiche = ctx.bridge.catalog.addRich();
  fiche.mods?.ways.push("drop_jars");
  ctx.bridge.catalog.put(fiche);
  ctx.bridge.mods.seed("forge", "h_mc", {
    mods: [
      simMod({ file: "lithium.jar", identity: identity("lithium", "Lithium", "0.11.2") }),
      simMod({ file: "spark.jar", identity: identity("spark", "Spark", "1.10.53") }),
    ],
  });
  ctx.bridge.console.seed("forge", "h_mc", simConsoleView());
  ctx.bridge.console.seedPlayers("forge", "h_mc", {
    state: "ok",
    online: 2,
    max: 20,
    names: ["Alice_42", "Bram"],
    hiddenNames: 0,
    moreNames: 0,
    readAt: new Date().toISOString(),
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

async function review(ctx: Awaited<ReturnType<typeof open>>) {
  await ctx.wrapper.get("[data-mod-row='lithium.jar'] [data-mod-retire]").trigger("click");
  await ctx.wrapper.get("[data-mods-review]").trigger("click");
  await flushPromises();
}

async function submit(ctx: Awaited<ReturnType<typeof open>>) {
  await flushPromises();
  const field = reauthField();
  if (field) typeInto(field, OWN);
  await flushPromises();
  await ctx.wrapper.get("form[data-mods-apply]").trigger("submit");
  await flushPromises();
}

describe("« Vérifiez avant d'appliquer »", () => {
  it("dit les changements, les quatre étapes, les joueurs connectés, la durée et le retour automatique", async () => {
    const ctx = await open();
    await review(ctx);
    const { wrapper } = ctx;
    expect(wrapper.get("[data-mods-apply] h3").text()).toBe("Vérifiez avant d'appliquer");
    expect(wrapper.get("[data-mods-apply-change]").text()).toContain("Retrait de Lithium 0.11.2");
    expect(wrapper.findAll("[data-mods-apply-step]").map((el) => el.text().slice(0, 12))).toEqual([
      "1Arrêt du se",
      "2Sauvegarde ",
      "3Mise en pla",
      "4Redémarrage",
    ]);
    expect(wrapper.get("[data-players-count]").text()).toBe("2 sur 20 places");
    expect(wrapper.get("[data-players-names]").text()).toContain("Bram");
    expect(wrapper.get("[data-mods-apply-kick]").text()).toContain("déconnectés à l'étape 1");
    expect(wrapper.get("[data-mods-apply-duration]").text()).toContain(
      "remet tout seul les mods d'avant",
    );
    expect(wrapper.get("[data-mods-apply]").text()).toContain(
      "Un mod est un programme écrit par quelqu'un d'autre",
    );
    expect(wrapper.get("[data-mods-apply-submit]").text()).toBe("Appliquer et redémarrer");
    wrapper.unmount();
  });

  it("console muette : pas de chiffre, « seront déconnectés », et l'application reste possible", async () => {
    const ctx = await open((bridge) =>
      bridge.console.seedPlayers("forge", "h_mc", {
        state: "unavailable",
        online: null,
        max: null,
        names: [],
        hiddenNames: 0,
        moreNames: 0,
        readAt: null,
      }),
    );
    await review(ctx);
    const { wrapper } = ctx;
    expect(wrapper.find("[data-players-count]").exists()).toBe(false);
    expect(wrapper.get("[data-players-sentence]").text()).toBe(
      "Hearth n'a pas pu lire les joueurs connectés.",
    );
    expect(wrapper.get("[data-mods-apply-step]").text()).toContain("seront déconnectés");
    expect(wrapper.find("[data-mods-apply-nospace]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("serveur arrêté : deux étapes seulement, il reste arrêté", async () => {
    const ctx = await open(undefined, "stopped");
    await review(ctx);
    const { wrapper } = ctx;
    expect(wrapper.findAll("[data-mods-apply-step]")).toHaveLength(2);
    expect(wrapper.get("[data-mods-apply-stopped]").text()).toContain("il le reste");
    expect(wrapper.find("[data-players-card]").exists()).toBe(false);
    expect(wrapper.get("[data-mods-apply-submit]").text()).toBe("Appliquer");
    wrapper.unmount();
  });

  it("espace insuffisant : l'espace qui manque est dit, rien n'est lancé", async () => {
    const ctx = await open((bridge) => bridge.backups.setFree("forge", 1_000_000));
    await review(ctx);
    const { wrapper, bridge } = ctx;
    expect(wrapper.get("[data-mods-apply-nospace]").text()).toContain(
      "d'espace pour la sauvegarde",
    );
    expect(wrapper.get("[data-mods-apply-submit]").attributes("aria-disabled")).toBe("true");
    await submit(ctx);
    expect(bridge.mods.received).toHaveLength(0);
    wrapper.unmount();
  });

  it("le mot de passe est demandé, et sans lui rien n'est envoyé", async () => {
    const ctx = await open();
    await review(ctx);
    await flushPromises();
    expect(reauthField()).not.toBeNull();
    expect(ctx.wrapper.get("[data-mods-apply-submit]").attributes("aria-disabled")).toBe("true");
    ctx.wrapper.unmount();
  });
});

describe("le suivi, la fin et l'échec", () => {
  it("applique : le lot part avec le mot de passe, le suivi coche les étapes, la fin dit ce qui a changé", async () => {
    const ctx = await open();
    await review(ctx);
    await submit(ctx);
    const { wrapper, bridge } = ctx;
    expect(bridge.mods.received).toHaveLength(1);
    expect(bridge.mods.received[0]?.batch.retire).toEqual(["lithium.jar"]);
    expect(wrapper.get("[data-mods-progress]").text()).toContain("Mise en place des mods");
    const op = useServiceOpsStore().forService("forge", "h_mc")?.operation.id ?? "";
    bridge.mods.toStep("forge", op, "copy");
    await flushPromises();
    const states = wrapper
      .findAll("[data-mods-progress-step]")
      .map((el) => el.attributes("data-state"));
    expect(states).toEqual(["done", "running", "waiting", "waiting"]);
    expect(wrapper.find("[data-mods-progress-cancel]").exists()).toBe(true);
    bridge.mods.toStep("forge", op, "place");
    await flushPromises();
    expect(wrapper.find("[data-mods-progress-cancel]").exists()).toBe(false);
    bridge.mods.succeed("forge", op);
    await flushPromises();
    expect(wrapper.get("[data-mods-end]").attributes("data-mods-end")).toBe("succeeded");
    expect(wrapper.get("[data-mods-progress]").text()).toContain("Retrait de Lithium 0.11.2");
    expect(wrapper.get("[data-mods-end-players]").text()).toContain("mêmes mods que le serveur");
    await wrapper.get("[data-mods-end-close]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-mod-row='lithium.jar'][data-mod-pending]").exists()).toBe(false);
    expect(wrapper.find("[data-mod-row='lithium.jar']").exists()).toBe(true);
    wrapper.unmount();
  });

  it("l'échec revient à l'état d'avant : phrase, dernières lignes, phrase en clair, aucun choix à faire", async () => {
    const ctx = await open();
    await review(ctx);
    await submit(ctx);
    const { wrapper, bridge } = ctx;
    const op = useServiceOpsStore().forService("forge", "h_mc")?.operation.id ?? "";
    bridge.mods.toStep("forge", op, "watch");
    bridge.mods.rollBack("forge", op, {
      lines: ["Mod waystones requires balm 7.x or later, which is missing"],
      reason: "missing_dependency",
      sentence: "Le mod Waystones a besoin du mod Balm, qui n'est pas installé.",
    });
    await flushPromises();
    await flushPromises();
    const text = wrapper.get("[data-mods-progress]").text();
    expect(text).toContain(
      "Le serveur n'a pas démarré avec ces mods. Hearth a remis les mods d'avant.",
    );
    expect(wrapper.get("[data-mods-fail-tail]").text()).toContain("requires balm 7.x");
    expect(wrapper.get("[data-mods-fail-sentence]").text()).toContain(
      "En clair : Le mod Waystones",
    );
    expect(text).not.toContain("Revenir à l'état d'avant");
    expect(text).not.toContain("Laisser tel quel");
    expect(wrapper.find("[data-mods-fail-logs]").exists()).toBe(true);
    wrapper.unmount();
  });

  it("annuler avant la mise en place : les mods restent ceux d'avant", async () => {
    const ctx = await open();
    await review(ctx);
    await submit(ctx);
    const { wrapper, bridge } = ctx;
    const op = useServiceOpsStore().forService("forge", "h_mc")?.operation.id ?? "";
    bridge.mods.toStep("forge", op, "copy");
    await flushPromises();
    await wrapper.get("[data-mods-progress-cancel]").trigger("click");
    await confirmDialog("Annuler l'opération");
    await flushPromises();
    vi.advanceTimersByTime(10);
    await flushPromises();
    expect(wrapper.get("[data-mods-end]").attributes("data-mods-end")).toBe("cancelled");
    wrapper.unmount();
  });

  it("un refus de l'agent (mod du pack Modrinth) est dit dans l'écran, sans rien lancer", async () => {
    const ctx = await open();
    ctx.bridge.mods.viewOf("forge", "h_mc")?.mods.push(
      simMod({
        file: "pack.jar",
        origin: "modrinth_pack",
        identity: identity("pack", "Pack", "1"),
      }),
    );
    // Le lot est préparé sur l'écran, puis le serveur change entre-temps : le fichier est retiré de la liste côté agent.
    await review(ctx);
    ctx.bridge.mods.viewOf("forge", "h_mc")?.mods.shift();
    await submit(ctx);
    expect(ctx.wrapper.get("[data-mods-apply-error]").text()).toContain("plus dans le bon dossier");
    ctx.wrapper.unmount();
  });
});
