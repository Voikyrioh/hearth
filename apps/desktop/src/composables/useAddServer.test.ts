import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { effectScope } from "vue";
import {
  OTHER_FINGERPRINT,
  SAMPLE_AGENT,
  SAMPLE_FINGERPRINT,
  type SimAgent,
  SimulatedLinkBridge,
  setLinkBridge,
} from "@/link";
import { useServersStore } from "@/stores/servers";
import { useToastsStore } from "@/stores/toasts";
import { useAddServer } from "./useAddServer";

const PASSWORD = "Correct-Horse-9";

async function setup(options: { agents?: SimAgent[]; servers?: "sample" | "none" } = {}) {
  setActivePinia(createPinia());
  const bridge = new SimulatedLinkBridge({
    agents: options.agents ?? [SAMPLE_AGENT],
    ...(options.servers === "sample" ? {} : { servers: [] }),
  });
  setLinkBridge(bridge);
  const servers = useServersStore();
  await servers.load();
  const scope = effectScope();
  const wizard = scope.run(() => useAddServer());
  if (!wizard) throw new Error("assistant non créé");
  return { bridge, servers, wizard, scope };
}

function fill(wizard: ReturnType<typeof useAddServer>, host = "192.168.1.50", name = "Atelier") {
  wizard.name.value = name;
  wizard.host.value = host;
}

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  setLinkBridge(null);
});

describe("add-server wizard, step 1", () => {
  it("enables « Suivant » only when the name, the address and the port are valid", async () => {
    const { wizard } = await setup();
    expect(wizard.canNext.value).toBe(false);
    fill(wizard, "pas une adresse");
    expect(wizard.canNext.value).toBe(false);
    wizard.touched.value.host = true;
    expect(wizard.errors.value.host).toBe("Cette adresse n'est pas valide");
    wizard.host.value = "192.168.1.50";
    expect(wizard.canNext.value).toBe(true);
    wizard.port.value = "70000";
    wizard.touched.value.port = true;
    expect(wizard.canNext.value).toBe(false);
    expect(wizard.errors.value.port).toBe("L'emplacement saisi n'est pas valide");
    wizard.port.value = "";
    expect(wizard.canNext.value).toBe(true);
  });

  it("refuses a name already used, ignoring case", async () => {
    const { wizard } = await setup({ servers: "sample" });
    fill(wizard, "192.168.1.50", " FORGE ");
    wizard.touched.value.name = true;
    expect(wizard.errors.value.name).toBe("Un serveur porte déjà ce nom");
    expect(wizard.canNext.value).toBe(false);
  });

  it("offers the existing entry, without probing, when the address is already registered", async () => {
    const { wizard, bridge } = await setup({ servers: "sample" });
    fill(wizard, "192.168.1.120", "Autre");
    await wizard.next();
    expect(wizard.existing.value?.id).toBe("forge");
    expect(wizard.step.value).toBe("address");
    expect(bridge.calls).toEqual([]);
  });

  it("says the address is unreachable under the address field and keeps the form", async () => {
    const { wizard } = await setup();
    fill(wizard, "10.9.9.9");
    await wizard.next();
    expect(wizard.step.value).toBe("address");
    expect(wizard.errors.value.host).toBe(
      "Cette adresse n'est pas joignable. Vérifie l'adresse et essaie de nouveau.",
    );
    expect(wizard.name.value).toBe("Atelier");
    // Retoucher l'adresse efface l'échec précédent.
    wizard.edited();
    expect(wizard.errors.value.host).toBeNull();
  });

  it("shows an incompatible version as a message of the card", async () => {
    const { wizard } = await setup({
      agents: [
        { ...SAMPLE_AGENT, incompatible: "agent" },
        { host: "10.0.0.2", fingerprint: SAMPLE_FINGERPRINT, incompatible: "client" },
      ],
    });
    fill(wizard);
    await wizard.next();
    expect(wizard.cardMessage.value).toBe(
      "L'agent de ce serveur est trop ancien. Mets à jour l'agent sur le serveur.",
    );
    fill(wizard, "10.0.0.2");
    wizard.edited();
    await wizard.next();
    expect(wizard.cardMessage.value).toBe(
      "Le client est trop ancien. Mets à jour le client sur ce PC.",
    );
  });
});

describe("add-server wizard, steps 2 and 3", () => {
  it("probes, shows the fingerprint in 8 groups, registers on « Confirmer », then connects", async () => {
    const { wizard, bridge, servers } = await setup();
    fill(wizard);
    await wizard.next();
    expect(wizard.step.value).toBe("fingerprint");
    expect(wizard.probe.value?.display.split(" ")).toHaveLength(8);
    // Rien n'est enregistré, aucun identifiant n'est parti avant la confirmation (BR-CONN-001, 011).
    expect(servers.servers).toHaveLength(0);
    expect(bridge.calls.some((call) => call.startsWith("login"))).toBe(false);

    await wizard.confirm();
    expect(wizard.step.value).toBe("login");
    expect(servers.servers).toHaveLength(1);
    expect(servers.servers[0]?.name).toBe("Atelier");

    const id = await wizard.login({ username: "marie", password: PASSWORD, remember: true });
    expect(id).toBe(wizard.serverId.value);
    expect(servers.servers[0]).toMatchObject({ username: "marie", remember: true, role: "admin" });
    expect(bridge.vault.get(id ?? "")).toBe(PASSWORD);
    expect(useToastsStore().items.at(-1)?.message).toBe("Connecté à Atelier.");
    // Le journal des commandes ne contient jamais le mot de passe.
    expect(bridge.calls.join("\n")).not.toContain(PASSWORD);
  });

  it("registers nothing when the fingerprint is refused", async () => {
    const { wizard, bridge, servers } = await setup();
    fill(wizard);
    await wizard.next();
    wizard.refuse();
    expect(wizard.step.value).toBe("address");
    expect(servers.servers).toHaveLength(0);
    expect(bridge.calls).toEqual(["probe 192.168.1.50:7341"]);
  });

  it("answers a wrong password with the generic message, then lets the user retry", async () => {
    const { wizard } = await setup();
    fill(wizard);
    await wizard.next();
    await wizard.confirm();
    expect(
      await wizard.login({ username: "marie", password: "faux-faux-1", remember: true }),
    ).toBeNull();
    expect(wizard.loginError.value).toBe("Identifiant ou mot de passe incorrect.");
    expect(await wizard.login({ username: "inconnu", password: "x", remember: true })).toBeNull();
    // Même texte pour un identifiant inconnu (BR-CONN-013).
    expect(wizard.loginError.value).toBe("Identifiant ou mot de passe incorrect.");
    expect(
      await wizard.login({ username: "marie", password: PASSWORD, remember: false }),
    ).not.toBeNull();
    expect(wizard.loginError.value).toBeNull();
  });

  it("starts a visible countdown after too many attempts and refuses to try meanwhile", async () => {
    const { wizard, bridge } = await setup();
    fill(wizard);
    await wizard.next();
    await wizard.confirm();
    for (let i = 0; i < 5; i += 1) {
      await wizard.login({ username: "marie", password: `faux-${i}`, remember: true });
    }
    expect(
      await wizard.login({ username: "marie", password: PASSWORD, remember: true }),
    ).toBeNull();
    expect(wizard.lockedSeconds.value).toBe(30);
    const attempts = bridge.calls.filter((call) => call.startsWith("login")).length;
    await vi.advanceTimersByTimeAsync(3000);
    expect(wizard.lockedSeconds.value).toBe(27);
    expect(
      await wizard.login({ username: "marie", password: PASSWORD, remember: true }),
    ).toBeNull();
    expect(bridge.calls.filter((call) => call.startsWith("login")).length).toBe(attempts);
  });

  it("removes the server it registered when going back, and keeps the typed values", async () => {
    const { wizard, servers, bridge } = await setup();
    fill(wizard);
    await wizard.next();
    await wizard.confirm();
    expect(servers.servers).toHaveLength(1);
    await wizard.back();
    expect(servers.servers).toHaveLength(0);
    expect(wizard.step.value).toBe("address");
    expect(wizard.name.value).toBe("Atelier");
    expect(bridge.calls.at(-1)).toMatch(/^remove /);
  });

  it("leaves nothing behind when the user quits at step 3, but keeps a connected server", async () => {
    const quit = await setup();
    fill(quit.wizard);
    await quit.wizard.next();
    await quit.wizard.confirm();
    await quit.wizard.abandon();
    expect(quit.servers.servers).toHaveLength(0);

    const done = await setup();
    fill(done.wizard);
    await done.wizard.next();
    await done.wizard.confirm();
    await done.wizard.login({ username: "marie", password: PASSWORD, remember: true });
    await done.wizard.abandon();
    expect(done.servers.servers).toHaveLength(1);
  });

  it("tells a taken name from the library back to step 1", async () => {
    const { wizard, bridge } = await setup();
    fill(wizard);
    await wizard.next();
    // Un autre serveur du même nom apparaît entre la sonde et la confirmation.
    bridge.seedServer({
      id: "x",
      name: "Atelier",
      address: "x.lan",
      host: "x.lan",
      port: 7341,
      color: 2,
      role: "admin",
      username: "",
      remember: false,
    });
    await wizard.confirm();
    expect(wizard.step.value).toBe("address");
    expect(wizard.errors.value.name).toBe("Un serveur porte déjà ce nom");
  });
});

describe("pinned fingerprint in the simulated network", () => {
  it("refuses to register a server whose identity differs from the confirmed one", async () => {
    const { wizard, bridge } = await setup();
    fill(wizard);
    await wizard.next();
    bridge.addAgent({ ...SAMPLE_AGENT, host: "192.168.1.50", fingerprint: OTHER_FINGERPRINT });
    // Le réseau simulé répond maintenant avec l'autre agent en premier : l'ancien est retiré.
    const agents = (bridge as unknown as { agents: SimAgent[] }).agents;
    agents.shift();
    await wizard.confirm();
    expect(wizard.step.value).toBe("address");
    expect(wizard.cardMessage.value).toBe("Une erreur est survenue. Réessaie.");
  });
});
