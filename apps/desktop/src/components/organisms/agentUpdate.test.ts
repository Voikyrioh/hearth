import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import { useAgentUpdatesStore } from "@/stores/agentUpdates";
import { useToastsStore } from "@/stores/toasts";
import { startedApp } from "@/test/app";
import { confirmDialog, dialogButton } from "@/test/confirm";
import AgentUpdateCard from "./AgentUpdateCard.vue";

// HRT-17, l'écran « État du serveur » : le bouton selon le rôle, la confirmation au texte exact, les
// étapes une à une, la coupure attendue du redémarrage (« Reconnexion… » sans erreur), le résultat
// (réussi, annulé, échoué), les refus, l'installation gérée, l'incompatibilité de versions. Pont
// simulé : l'agent y est piloté étape par étape ; les règles réelles sont prouvées contre un vrai
// agent côté Rust (`tests/agent_update_runtime.rs`).

beforeEach(() => {
  // `<dialog>` n'a pas de `showModal` dans happy-dom.
  HTMLDialogElement.prototype.showModal = function showModal() {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function close() {
    this.removeAttribute("open");
  };
});

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

async function card(serverId: "forge" | "salon" = "forge") {
  const ctx = await startedApp();
  const store = useAgentUpdatesStore();
  await store.start();
  const server = ctx.servers.byId(serverId);
  if (!server) throw new Error("serveur d'exemple absent");
  const wrapper = mount(AgentUpdateCard, {
    props: { server },
    global: { plugins: [ctx.pinia] },
    attachTo: document.body,
  });
  await flushPromises();
  return { ...ctx, wrapper, store, toasts: useToastsStore() };
}

const sim = (bridge: SimulatedLinkBridge) => bridge.agentUpdates;
const update = (wrapper: VueWrapper) => wrapper.find("[data-agent-update-button]");
/** Confirme la mise à jour dans la fenêtre : le mot de passe est toujours demandé (jamais couvert). */
async function confirmUpdate(wrapper: VueWrapper) {
  await update(wrapper).trigger("click");
  await confirmDialog("Oui, mettre à jour");
}

describe("le bouton « Mettre à jour l'agent »", () => {
  it("is visible to an administrator when a newer version is available, with both versions and the tag", async () => {
    const { wrapper } = await card();
    expect(wrapper.get("h3").text()).toBe("État du serveur : forge");
    expect(wrapper.get("[data-agent-versions]").text()).toContain("Agent 0.1.0");
    expect(wrapper.get("[data-agent-tag]").text()).toBe("Mise à jour disponible");
    expect(wrapper.get("[data-agent-available]").text()).toBe(
      "Mise à jour disponible pour l'agent",
    );
    expect(wrapper.get("[data-agent-versions]").text()).toBe(
      "Client indisponible · Agent 0.1.0 · Disponible 0.2.0",
    );
    const button = update(wrapper);
    expect(button.text()).toBe("Mettre à jour l'agent");
    expect(button.attributes("aria-disabled")).toBeUndefined();
  });

  it("is not there at all when nothing newer exists, and says the agent is up to date", async () => {
    const ctx = await card();
    sim(ctx.bridge).seed("forge", { target: "0.1.0" });
    await ctx.store.refresh("forge");
    await flushPromises();
    expect(ctx.wrapper.find("[data-agent-update-button]").exists()).toBe(false);
    expect(ctx.wrapper.find("[data-agent-tag]").exists()).toBe(false);
    expect(ctx.wrapper.get("[data-agent-uptodate]").text()).toBe("L'agent est à jour.");
    // Jamais de rétrogradation : une version plus ancienne n'est pas « disponible » non plus.
    sim(ctx.bridge).seed("forge", { target: "0.0.9" });
    await ctx.store.refresh("forge");
    await flushPromises();
    expect(ctx.wrapper.find("[data-agent-update-button]").exists()).toBe(false);
  });

  it("is disabled for a read-only account, with the exact explanation, and never opens the confirmation", async () => {
    const { wrapper, bridge } = await card("salon");
    const button = update(wrapper);
    expect(button.exists()).toBe(true);
    expect(button.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe(
      "Seul un administrateur peut mettre à jour l'agent",
    );
    await button.trigger("click");
    await flushPromises();
    expect(document.querySelector("dialog")).toBeNull();
    expect(bridge.calls.filter((call) => /^agent-update \d/.test(call)).length).toBe(0);
  });

  it("asks for confirmation with the exact texts, and « Annuler » sends nothing", async () => {
    const { wrapper, bridge } = await card();
    await update(wrapper).trigger("click");
    await flushPromises();
    const dialog = document.querySelector("dialog");
    expect(dialog?.querySelector("h2")?.textContent).toBe("Mettre à jour l'agent ?");
    expect(dialog?.textContent).toContain("Cette opération redémarrera l'agent brièvement.");
    const labels = [...(dialog?.querySelectorAll("button") ?? [])].map((b) =>
      b.textContent?.trim(),
    );
    // Le bouton qui montre/masque le mot de passe n'a pas de texte : seuls les deux boutons de la fenêtre.
    expect(labels.filter(Boolean)).toEqual(["Annuler", "Oui, mettre à jour"]);
    dialogButton("Annuler")?.click();
    await flushPromises();
    expect(document.querySelector("dialog")).toBeNull();
    expect(bridge.calls.filter((call) => /^agent-update \d/.test(call)).length).toBe(0);
  });

  it("sends only the number of the version that is shown, never an address, a signature or a checksum", async () => {
    const { wrapper, bridge } = await card();
    await confirmUpdate(wrapper);
    expect(bridge.calls.filter((call) => /^agent-update \d/.test(call))).toEqual([
      "agent-update 0.2.0",
    ]);
  });
});

describe("l'avancement : étapes discrètes, une à la fois", () => {
  it("shows the five steps with the download percentage, then the restart as « Reconnexion… » without any error", async () => {
    const ctx = await card();
    const { wrapper, bridge, toasts } = ctx;
    await confirmUpdate(wrapper);
    // Acceptée : les étapes s'affichent tout de suite.
    expect(wrapper.get("[data-agent-progress]").text()).toBe(
      "Mise à jour de l'agent en cours. Étape : téléchargement…",
    );
    expect(
      wrapper.findAll("[data-agent-steps] li").map((li) => li.attributes("data-step")),
    ).toEqual(["download", "verify", "install", "restart", "check"]);
    sim(bridge).advance("forge", "download", 35);
    await flushPromises();
    expect(wrapper.get("[data-agent-progress]").text()).toBe(
      "Mise à jour de l'agent en cours. Étape : téléchargement (35 %)…",
    );
    const states = () =>
      wrapper.findAll("[data-agent-steps] li").map((li) => li.attributes("data-state"));
    expect(states()).toEqual(["now", "later", "later", "later", "later"]);
    expect(wrapper.get('[data-step="download"]').text()).toContain("Téléchargement : 35 %");
    // Le bouton est inerte pendant l'opération, la mention disparaît.
    expect(update(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.find("[data-agent-tag]").exists()).toBe(false);
    // Les étapes suivantes, une à la fois.
    for (const [step, expected] of [
      ["verify", ["done", "now", "later", "later", "later"]],
      ["install", ["done", "done", "now", "later", "later"]],
    ] as const) {
      sim(bridge).advance("forge", step);
      await flushPromises();
      expect(states()).toEqual(expected);
    }
    // Le redémarrage : le lien tombe, « Reconnexion… », ni message d'erreur ni notification.
    sim(bridge).advance("forge", "restart");
    await flushPromises();
    expect(states()).toEqual(["done", "done", "done", "now", "later"]);
    expect(wrapper.get("[data-state]").attributes("data-state")).toBe("reconnecting");
    expect(wrapper.get(".pill").text()).toBe("Reconnexion…");
    expect(wrapper.text()).toContain("Le lien avec le serveur sera coupé brièvement");
    expect(toasts.items.filter((toast) => toast.kind === "error")).toEqual([]);
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    // Le contrôle du nouvel agent.
    sim(bridge).advance("forge", "check");
    await flushPromises();
    expect(states()).toEqual(["done", "done", "done", "done", "now"]);
  });

  it("shows the result when the link is back: the new version, « Connecté », and the button is gone", async () => {
    const { wrapper, bridge } = await card();
    await confirmUpdate(wrapper);
    for (const step of ["download", "verify", "install", "restart", "check"] as const) {
      sim(bridge).advance("forge", step, step === "download" ? 100 : null);
    }
    await flushPromises();
    sim(bridge).complete("forge", "succeeded");
    await flushPromises();
    expect(wrapper.get(".pill").text()).toBe("Connecté");
    expect(wrapper.get("[data-agent-result]").attributes("data-tone")).toBe("ok");
    expect(wrapper.get("[data-agent-result]").text()).toContain(
      "Mise à jour de l'agent réussie. L'agent est en version 0.2.0.",
    );
    expect(wrapper.get("[data-agent-versions]").text()).toContain("Agent 0.2.0");
    expect(wrapper.find("[data-agent-running]").exists()).toBe(false);
    expect(wrapper.find("[data-agent-update-button]").exists()).toBe(false);
    expect(wrapper.find("[data-agent-tag]").exists()).toBe(false);
  });

  it("keeps the steps through a cut of the link and reads the real result when it returns", async () => {
    const { wrapper, bridge, toasts } = await card();
    await confirmUpdate(wrapper);
    sim(bridge).advance("forge", "install");
    await flushPromises();
    // Coupure du lien (le réseau, le redémarrage) : pas d'alarme, les étapes restent.
    bridge.setState("forge", "reconnecting");
    await flushPromises();
    expect(wrapper.find("[data-agent-running]").exists()).toBe(true);
    expect(toasts.items.filter((toast) => toast.kind === "error")).toEqual([]);
    // Le lien revient pendant que la mise à jour continue chez l'agent : la lecture reprend l'état.
    sim(bridge).seed("forge", {
      progress: { version: "0.2.0", step: "check", percent: null, outcome: null, reason: null },
    });
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(wrapper.get('[data-state="now"]').attributes("data-step")).toBe("check");
    // Elle s'est terminée pendant la coupure : au retour, le résultat réel s'affiche.
    sim(bridge).seed("forge", {
      progress: null,
      current: "0.2.0",
      last: {
        version: "0.2.0",
        previous: "0.1.0",
        outcome: "succeeded",
        reason: null,
        at: new Date().toISOString(),
        recent: true,
        announced: false,
      },
    });
    bridge.setState("forge", "reconnecting");
    await flushPromises();
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(wrapper.find("[data-agent-running]").exists()).toBe(false);
    expect(wrapper.get("[data-agent-result]").text()).toContain("réussie");
    expect(wrapper.get("[data-agent-versions]").text()).toContain("Agent 0.2.0");
  });
});

describe("les résultats, avec les textes de la spécification", () => {
  it.each([
    [
      "rolled_back",
      "no_answer",
      "warn",
      "Mise à jour de l'agent annulée. Le nouvel agent n'a pas répondu. Retour à la version précédente.",
    ],
    [
      "failed",
      "unreachable",
      "crit",
      "Le serveur n'a pas accès à Internet pour télécharger la mise à jour de l'agent.",
    ],
    ["failed", "bad_checksum", "crit", "corrompu"],
    ["failed", "bad_signature", "crit", "La signature de la mise à jour de l'agent est refusée."],
    ["failed", "interrupted", "crit", "interrompue"],
    ["failed", "rollback_failed", "crit", "le retour à la version précédente n'est pas confirmé"],
    ["failed", "unknown", "crit", "n'a pas abouti"],
  ] as const)("%s / %s", async (outcome, reason, tone, text) => {
    const { wrapper, bridge } = await card();
    await confirmUpdate(wrapper);
    sim(bridge).advance("forge", "check");
    sim(bridge).complete("forge", outcome, reason);
    await flushPromises();
    const result = wrapper.get("[data-agent-result]");
    expect(result.attributes("data-tone")).toBe(tone);
    expect(result.text()).toContain(text);
    // L'agent n'a pas changé de version (retour arrière ou échec) : la version disponible reste proposée.
    expect(wrapper.get("[data-agent-versions]").text()).toContain("Agent 0.1.0");
    expect(wrapper.find("[data-agent-update-button]").exists()).toBe(true);
  });

  it("says nothing of the version when the agent could not tell which one was aimed at", async () => {
    const { wrapper, bridge } = await card();
    sim(bridge).completeUnknownVersion("forge", "interrupted");
    await flushPromises();
    const text = wrapper.get("[data-agent-result]").text();
    expect(text).toContain("interrompue");
    expect(text).not.toMatch(/\d+\.\d+\.\d+/);
  });

  it("can be dismissed, and the next identical result shows again after a new update", async () => {
    const { wrapper, bridge } = await card();
    sim(bridge).seed("forge", { target: "0.2.0" });
    await confirmUpdate(wrapper);
    sim(bridge).complete("forge", "failed", "unreachable");
    await flushPromises();
    await wrapper.get("[data-agent-result] button").trigger("click");
    expect(wrapper.find("[data-agent-result]").exists()).toBe(false);
    // Une relecture ne la remontre pas.
    await useAgentUpdatesStore().refresh("forge");
    await flushPromises();
    expect(wrapper.find("[data-agent-result]").exists()).toBe(false);
    // Une nouvelle mise à jour, même résultat : il se montre de nouveau.
    await confirmUpdate(wrapper);
    sim(bridge).complete("forge", "failed", "unreachable");
    await flushPromises();
    expect(wrapper.find("[data-agent-result]").exists()).toBe(true);
  });

  it("shows an old result as a plain history line, not as a message", async () => {
    const { wrapper, bridge, store } = await card();
    sim(bridge).seed("forge", {
      last: {
        version: "0.1.0",
        previous: "0.0.9",
        outcome: "succeeded",
        reason: null,
        at: "2026-09-01T10:00:00Z",
        recent: false,
        announced: false,
      },
    });
    await store.refresh("forge");
    await flushPromises();
    expect(wrapper.find("[data-agent-result]").exists()).toBe(false);
    expect(wrapper.get("[data-agent-history]").text()).toContain(
      "Dernière mise à jour de l'agent : réussie",
    );
  });
});

describe("les refus", () => {
  it("says an update is already running when another administrator started one (the agent decides)", async () => {
    const { wrapper, bridge, toasts } = await card();
    // Une autre session l'a lancée entre la lecture et le clic : la carte ne le sait pas encore.
    sim(bridge).seed("forge", {
      progress: { version: "0.2.0", step: "download", percent: 10, outcome: null, reason: null },
    });
    await confirmUpdate(wrapper);
    expect(toasts.items.map((toast) => toast.message)).toContain(
      "Une mise à jour de l'agent est déjà en cours. Réessaye plus tard.",
    );
    // L'état se relit : l'avancement de l'autre est affiché.
    await flushPromises();
    expect(wrapper.find("[data-agent-running]").exists()).toBe(true);
  });

  it("explains a managed installation and offers no button, even if a version is published", async () => {
    const { wrapper, bridge, store } = await card();
    sim(bridge).seed("forge", { managed: true });
    await store.refresh("forge");
    await flushPromises();
    expect(wrapper.get("[data-agent-managed]").text()).toBe(
      "Cette installation est gérée par le système : l'agent ne se met pas à jour à distance. Mets-le à jour par la configuration du système.",
    );
    expect(wrapper.find("[data-agent-update-button]").exists()).toBe(false);
    expect(wrapper.find("[data-agent-tag]").exists()).toBe(false);
  });

  it("refuses a forced request of a read-only account as the agent does, with the exact sentence", async () => {
    const ctx = await card("salon");
    const { bridge, toasts } = ctx;
    // Appel forcé par le code (le bouton est inerte) : l'agent est l'arbitre.
    const store = useAgentUpdatesStore();
    await store.refresh("salon");
    await expect(bridge.updateAgent("salon", "0.2.0", "Correct-Horse-9")).rejects.toMatchObject({
      failure: { kind: "forbidden" },
    });
    expect(toasts.items).toEqual([]);
  });

  it("does not replay an update cut before its answer, says so once, and reads the state when the link returns", async () => {
    const { wrapper, bridge, toasts } = await card();
    bridge.actionMode = "cut";
    bridge.executeBeforeCut = true;
    await confirmUpdate(wrapper);
    expect(bridge.calls.filter((call) => call === "agent-update 0.2.0").length).toBe(1);
    expect(toasts.items.map((toast) => toast.message)).toContain(
      "Le lien est tombé avant la réponse du serveur : on ne sait pas si la mise à jour a démarré. Rien n'est relancé.",
    );
    // Le lien revient : l'état réel (elle a bien démarré chez l'agent) est relu, jamais rejouée.
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(bridge.calls.filter((call) => call === "agent-update 0.2.0").length).toBe(1);
    expect(wrapper.find("[data-agent-running]").exists()).toBe(true);
  });

  it("keeps the button inert while the link is not connected, with the reason", async () => {
    const { wrapper, bridge } = await card();
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(update(wrapper).attributes("aria-disabled")).toBe("true");
    expect(wrapper.get('[role="tooltip"]').text()).toBe(
      "Indisponible tant que le serveur est hors ligne.",
    );
  });
});

describe("versions incompatibles (BR-UPDATE-020, 021)", () => {
  it("tells an administrator to update the agent, and a read-only account to ask an administrator", async () => {
    const admin = await card("forge");
    admin.bridge.publish("forge", "offline", { blocked: "incompatible_agent" });
    await flushPromises();
    expect(admin.wrapper.get("[data-agent-incompat]").text()).toBe(
      "Les versions du client et de l'agent ne sont pas compatibles. Mets à jour l'agent.",
    );
    document.body.innerHTML = "";
    const reader = await card("salon");
    reader.bridge.publish("salon", "offline", { blocked: "incompatible_agent" });
    await flushPromises();
    expect(reader.wrapper.get("[data-agent-incompat]").text()).toBe(
      "Les versions du client et de l'agent ne sont pas compatibles. Demande à un administrateur de mettre à jour l'agent.",
    );
  });

  it("says when the client is the one to update", async () => {
    const { wrapper, bridge } = await card();
    bridge.publish("forge", "offline", { blocked: "incompatible_client" });
    await flushPromises();
    expect(wrapper.get("[data-agent-incompat]").text()).toBe(
      "Les versions du client et de l'agent ne sont pas compatibles. Mets à jour le client.",
    );
  });
});

describe("la mention de la liste des serveurs (BR-UPDATE-023)", () => {
  it("names only the servers that have a newer agent available", async () => {
    const { bridge, store } = await card();
    sim(bridge).seed("salon", { target: null });
    await store.refresh("forge");
    await store.refresh("salon");
    expect(store.withUpdate).toEqual(["forge"]);
  });
});
