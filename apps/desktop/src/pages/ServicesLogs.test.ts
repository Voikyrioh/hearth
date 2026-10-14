import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceEnvStore } from "@/stores/serviceEnv";
import { useServiceLogsStore } from "@/stores/serviceLogs";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { mountContext } from "@/test/mount";

// L'onglet Journaux d'un service (HRT-67) sur le pont simulé : tous les états de la conception (écran D). La mise en
// page réelle (fenêtre de lignes, défilement) se prouve en navigateur (`e2e/hrt67-logs.spec.ts`) : happy-dom ne
// calcule aucune taille.
afterEach(() => {
  useServicesStore().reset();
  useServiceRightsStore().reset();
  useServiceEnvStore().clear();
  useServiceLogsStore().close();
});

const WEB = { key: "c_web", name: "web", origin: "hearth" as const };

async function open(setup?: (bridge: SimulatedLinkBridge) => void, key = "c_web") {
  const ctx = await mountContext();
  ctx.bridge.services.seed("forge", [WEB, { key: "c_db", name: "base" }]);
  setup?.(ctx.bridge);
  useServersStore().setCurrent("forge");
  await ctx.router.push({
    name: "services",
    params: { id: "forge" },
    query: { service: key, onglet: "journaux" },
  });
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

const store = () => useServiceLogsStore();
const logsText = (wrapper: ReturnType<typeof mount>) => wrapper.get("[data-logs]").text();

describe("onglet Journaux : états", () => {
  it("un service créé par Hearth se lit par tous ; le bandeau de prudence est permanent", async () => {
    const { wrapper } = await open();
    await vi.waitFor(() => expect(store().phase).toBe("reading"));
    await flushPromises();
    expect(wrapper.get("[data-logs-notice]").text()).toContain("secrets compris");
    expect(wrapper.get("[data-tab='logs']").attributes("aria-selected")).toBe("true");
    // Aucune ligne reçue : « rien écrit », pas une erreur.
    expect(wrapper.find("[data-logs-empty]").text()).toContain("Ce service n'a encore rien écrit");
  });

  it("la liste est masquée sur l'onglet Journaux", async () => {
    const { wrapper } = await open();
    expect(wrapper.get("[data-services-work]").classes()).toContain("svc__work--logs");
  });

  it("un service adopté sans le droit : phrase réservée, aucune lecture ouverte", async () => {
    const { wrapper, bridge } = await open((b) => {
      b.services.upsert("forge", {
        key: "c_web",
        name: "web",
        origin: "external",
        adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
      });
      b.setServiceRightOnServer("forge", "marie", false);
    });
    await vi.waitFor(() => expect(wrapper.find("[data-detail-logs-reserved]").exists()).toBe(true));
    expect(wrapper.get("[data-detail-logs-reserved]").text()).toBe(
      "Réservé aux comptes qui peuvent gérer les services.",
    );
    expect(wrapper.find("[data-logs]").exists()).toBe(false);
    expect(bridge.logs.opened).toHaveLength(0);
  });

  it("plafond du compte : dit combien et quoi faire, avec « Réessayer »", async () => {
    const { wrapper } = await open((b) => b.logs.refuseNext("logs_account_limit"));
    await vi.waitFor(() => expect(wrapper.find("[data-logs-notice-end]").exists()).toBe(true));
    const notice = wrapper.get("[data-logs-notice-end]");
    expect(notice.attributes("role")).toBe("alert");
    expect(notice.text()).toContain("Tu lis déjà les journaux de 3 services");
    expect(notice.text()).toContain("Ferme un onglet Journaux");
    expect(wrapper.find("[data-logs-retry]").exists()).toBe(true);
    expect(wrapper.find("[data-logs-scroller]").exists()).toBe(false);
  });

  it("frein : dit d'attendre, avec le délai, et le bouton reste inactif tant qu'il court", async () => {
    const { wrapper, bridge } = await open((b) => b.logs.refuseNext("logs_rate_limited", 7));
    await vi.waitFor(() => expect(wrapper.find("[data-logs-notice-end]").exists()).toBe(true));
    expect(wrapper.get("[data-logs-notice-end]").text()).toMatch(/Attends [67] s/);
    expect(wrapper.get("[data-logs-retry]").attributes("aria-disabled")).toBe("true");
    await wrapper.get("[data-logs-retry]").trigger("click");
    await flushPromises();
    expect(bridge.logs.opened).toHaveLength(1);
  });

  it("refus du moteur, lecture impossible, session finie, agent ancien : chacun son texte, jamais d'état inventé", async () => {
    const cases: [Parameters<SimulatedLinkBridge["logs"]["refuseNext"]>[0], string][] = [
      ["logs_unreadable", "Le moteur ne permet pas de relire les journaux"],
      ["logs_limit", "Trop de journaux sont ouverts en même temps sur ce serveur (10)"],
      ["engine_unavailable", "Le moteur de conteneurs n'est pas prêt"],
      ["unknown", "La lecture des journaux s'est arrêtée."],
    ];
    for (const [refusal, text] of cases) {
      const { wrapper } = await open((b) => b.logs.refuseNext(refusal));
      await vi.waitFor(() => expect(wrapper.find("[data-logs-notice-end]").exists()).toBe(true));
      expect(wrapper.get("[data-logs-notice-end]").text()).toContain(text);
      wrapper.unmount();
      store().close();
    }
  });

  it("service arrêté : bandeau et code ; redémarrage : repère et bandeau retiré", async () => {
    const { wrapper, bridge } = await open((b) => {
      b.logs.seed("forge", "c_web", [{ text: "fin" }]);
      b.logs.setStatus("forge", "c_web", {
        state: "ended",
        exitCode: 137,
        endedAt: Date.now() - 120_000,
      });
    });
    await vi.waitFor(() => expect(wrapper.find("[data-logs-stopped]").exists()).toBe(true));
    expect(wrapper.get("[data-logs-stopped]").text()).toContain("Ce service est arrêté");
    expect(wrapper.get("[data-logs-stopped-code]").text()).toContain("code 137");
    expect(wrapper.get("[data-logs-state]").text()).toContain("Arrêté");
    bridge.logs.restart("forge", "c_web");
    await vi.waitFor(() => expect(wrapper.find("[data-logs-stopped]").exists()).toBe(false));
    expect(wrapper.get("[data-logs-state]").text()).toContain("En direct");
  });

  it("conteneur supprimé à chaque arrêt : le moteur n'a rien gardé", async () => {
    const { wrapper } = await open((b) =>
      b.logs.setStatus("forge", "c_web", { state: "ended", notKept: true }),
    );
    await vi.waitFor(() => expect(wrapper.find("[data-logs-not-kept]").exists()).toBe(true));
    expect(wrapper.get("[data-logs-not-kept]").text()).toContain(
      "son conteneur est supprimé à chaque arrêt",
    );
  });

  it("sorties mélangées : le repère d'erreur n'existe pas et un bandeau le dit", async () => {
    const { wrapper } = await open((b) => {
      b.logs.seed("forge", "c_web", [{ text: "a", source: "err" }]);
      b.logs.setStatus("forge", "c_web", { mixed: true });
    });
    await vi.waitFor(() => expect(wrapper.find("[data-logs-mixed]").exists()).toBe(true));
  });

  it("valeur inconnue : l'état est dit neutre, aucune affirmation", async () => {
    const { wrapper } = await open((b) => b.logs.setStatus("forge", "c_web", { state: "unknown" }));
    await vi.waitFor(() => expect(wrapper.find("[data-logs-state]").exists()).toBe(true));
    expect(wrapper.get("[data-logs-state]").text()).toBe("État du service inconnu");
  });

  it("lignes reçues puis service disparu : lignes gardées, phrase dite, lecture fermée", async () => {
    const { wrapper, bridge } = await open((b) =>
      b.logs.seed("forge", "c_web", [{ text: "dernière" }]),
    );
    await vi.waitFor(() => expect(store().received).toBe(true));
    bridge.services.remove("forge", "c_web");
    await flushPromises();
    await vi.waitFor(() => expect(store().gone).toBe(true));
    expect(bridge.logs.openCount()).toBe(0);
    expect(store().entries().length).toBeGreaterThan(0);
    expect(wrapper.find("[data-detail-gone]").exists()).toBe(true);
  });
});

describe("quitter l'onglet efface et ferme (BR-SLOG-019, BR-SLOG-020)", () => {
  it("revenir sur Détail, ou démonter, ferme la lecture et vide la mémoire", async () => {
    const { wrapper, bridge } = await open((b) =>
      b.logs.seed("forge", "c_web", [{ text: "secret" }]),
    );
    await vi.waitFor(() => expect(store().received).toBe(true));
    expect(bridge.logs.openCount()).toBe(1);
    await wrapper.get("[data-tab='detail']").trigger("click");
    await flushPromises();
    expect(bridge.logs.openCount()).toBe(0);
    expect(store().entries()).toEqual([]);
    expect(wrapper.find("[data-logs]").exists()).toBe(false);
  });

  it("aucune ligne n'est dans une zone live ni dans un attribut", async () => {
    const { wrapper } = await open((b) =>
      b.logs.seed("forge", "c_web", [{ text: "SENTINELLE-LIGNE" }]),
    );
    await vi.waitFor(() => expect(store().received).toBe(true));
    const region = wrapper.get("[data-logs-scroller]").html();
    expect(region).not.toContain('role="log"');
    expect(region).not.toContain('aria-live="polite"');
    // Le texte d'une ligne n'est jamais dans un attribut (info-bulle, nom accessible).
    expect(region.replace(/>[^<]*SENTINELLE-LIGNE[^<]*</, "><")).not.toContain("SENTINELLE");
    expect(logsText(wrapper)).toContain("SENTINELLE-LIGNE");
    expect(wrapper.get("[data-logs-scroller]").attributes("aria-live")).toBe("off");
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });
});
