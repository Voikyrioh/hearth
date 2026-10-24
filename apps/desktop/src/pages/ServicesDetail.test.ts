import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SimulatedLinkBridge } from "@/link";
import Services from "@/pages/Services.vue";
import { useServersStore } from "@/stores/servers";
import { useServiceEnvStore } from "@/stores/serviceEnv";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";
import { mountContext } from "@/test/mount";

// Le détail d'un service à côté de la liste (HRT-63, BR-SVC-007, 008, 009, 016, 023, 027, 028) sur le pont simulé,
// dont les formes sont comparées à la coquille. Les valeurs de variables : jamais dans le document avant une
// demande, effacées au remasquage.
beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => {
  vi.useRealTimers();
  useServicesStore().reset();
  useServiceRightsStore().reset();
  useServiceEnvStore().clear();
});

const SECRET = "sentinelle-valeur-secrete";
const WEB = {
  key: "c_web",
  name: "web",
  origin: "hearth" as const,
  command: ["nginx", "-g", "daemon off;"],
  graceS: 30,
  ports: [
    { hostPort: 8080, containerPort: 80, protocol: "tcp" as const, scope: "all" as const },
    { hostPort: 5353, containerPort: 53, protocol: "udp" as const, scope: "local" as const },
  ],
  mounts: [
    { kind: "folder" as const, source: "/srv/web", target: "/site", readOnly: true },
    { kind: "volume" as const, source: "cache", target: "/cache", readOnly: false },
  ],
};

async function open(
  setup?: (bridge: SimulatedLinkBridge) => void,
  options: { server?: "forge" | "salon"; query?: string } = {},
) {
  const ctx = await mountContext();
  setup?.(ctx.bridge);
  useServersStore().setCurrent(options.server ?? "forge");
  if (options.query) {
    await ctx.router.push({
      name: "services",
      params: { id: options.server ?? "forge" },
      query: { service: options.query },
    });
  }
  const wrapper = mount(Services, { global: ctx.global, attachTo: document.body });
  await flushPromises();
  return { ...ctx, wrapper };
}

function seedWeb(bridge: SimulatedLinkBridge, extra: Record<string, unknown> = {}) {
  bridge.services.seed("forge", [
    { ...WEB, ...extra },
    { key: "c_db", name: "base" },
  ]);
  bridge.services.setEnv("forge", "c_web", [
    { name: "SERVER_NAME", value: "forge.exemple" },
    { name: "MOT_DE_PASSE", value: SECRET },
    { name: "VIDE", value: "" },
  ]);
}

const detail = (wrapper: ReturnType<typeof mount>) => wrapper.find("[data-service-detail]");
const row = (wrapper: ReturnType<typeof mount>, key: string) =>
  wrapper.get(`[data-row][data-key='${key}']`);

async function openWeb(wrapper: ReturnType<typeof mount>) {
  await row(wrapper, "c_web").trigger("click");
  await flushPromises();
}

async function reveal(wrapper: ReturnType<typeof mount>) {
  await wrapper.get("[data-env-toggle]").trigger("click");
  await flushPromises();
}

describe("ouvrir et fermer le détail", () => {
  it("un clic ouvre le détail de CE service, la clé stable est dans l'adresse, le focus va au titre", async () => {
    const { wrapper, router } = await open(seedWeb);
    expect(detail(wrapper).exists()).toBe(false);
    await openWeb(wrapper);
    expect(router.currentRoute.value.query.service).toBe("c_web");
    expect(detail(wrapper).exists()).toBe(true);
    expect(wrapper.get("[data-detail-title]").text()).toBe("web");
    expect(document.activeElement).toBe(wrapper.get("[data-detail-title]").element);
    expect(row(wrapper, "c_web").attributes("aria-selected")).toBe("true");
    expect(row(wrapper, "c_db").attributes("aria-selected")).toBe("false");
    wrapper.unmount();
  });

  it("Entrée sur la ligne l'ouvre ; « Retour à la liste » le ferme, garde la liste et rend le focus à la ligne", async () => {
    const { wrapper, router } = await open(seedWeb);
    await row(wrapper, "c_web").trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(detail(wrapper).exists()).toBe(true);
    await wrapper.get("[data-detail-back]").trigger("click");
    await flushPromises();
    expect(detail(wrapper).exists()).toBe(false);
    expect(router.currentRoute.value.query.service).toBeUndefined();
    expect(wrapper.findAll("[data-row]")).toHaveLength(2);
    expect((document.activeElement as HTMLElement | null)?.dataset.key).toBe("c_web");
    wrapper.unmount();
  });

  it("Échap dans le détail le ferme", async () => {
    const { wrapper } = await open(seedWeb);
    await openWeb(wrapper);
    await detail(wrapper).trigger("keydown", { key: "Escape" });
    await flushPromises();
    expect(detail(wrapper).exists()).toBe(false);
    wrapper.unmount();
  });

  it("ouvert par l'adresse (rechargement) : le détail est là, sans voler le focus", async () => {
    const { wrapper } = await open(seedWeb, { query: "c_web" });
    expect(wrapper.get("[data-detail-title]").text()).toBe("web");
    expect(document.activeElement).not.toBe(wrapper.get("[data-detail-title]").element);
    wrapper.unmount();
  });

  it("le détail suit la CLÉ : un changement de nom ne le ferme pas et une mise à jour ne le ferme pas", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    bridge.services.upsert("forge", { key: "c_web", name: "web-renomme", state: "stopped" });
    await flushPromises();
    expect(wrapper.get("[data-detail-title]").text()).toBe("web-renomme");
    expect(detail(wrapper).attributes("data-gone")).toBeUndefined();
    wrapper.unmount();
  });
});

describe("ce que dit le détail (BR-SVC-007)", () => {
  it("image, commande, ports, dossiers, délai d'arrêt, redémarrage, ressources d'un service créé par Hearth", async () => {
    const { wrapper } = await open(seedWeb);
    await openWeb(wrapper);
    const text = detail(wrapper).text();
    expect(text).toContain("nginx:1.27");
    expect(wrapper.get("[data-detail-command]").text()).toBe("nginx -g daemon off;");
    expect(wrapper.get("[data-detail-ports]").text()).toContain("8080 vers 80 (TCP)");
    expect(wrapper.get("[data-detail-ports]").text()).toContain(
      "Joignable depuis : Tout le réseau du serveur",
    );
    expect(wrapper.get("[data-detail-ports]").text()).toContain(
      "Joignable depuis : Ce serveur seulement",
    );
    const mounts = wrapper.get("[data-detail-mounts]").text();
    expect(mounts).toContain("/srv/web");
    expect(mounts).toContain("Lecture seule");
    expect(mounts).toContain("Stockage géré par le moteur");
    expect(mounts).toContain("Lecture et écriture");
    expect(text).toContain("30 secondes");
    expect(text).toContain("Redémarre tout seul sauf si vous l'arrêtez");
    expect(text).toContain("Ressources : non limitées");
    expect(text).toContain("Créé le");
    wrapper.unmount();
  });

  it("sans commande propre : « Commande par défaut du programme » ; réseau du serveur : aucun port inventé ; aucun dossier", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [
        { key: "c_dns", name: "dns", hostNetwork: true },
        { key: "c_nu", name: "nu" },
      ]),
    );
    await row(wrapper, "c_dns").trigger("click");
    await flushPromises();
    expect(detail(wrapper).text()).toContain("Commande par défaut du programme");
    expect(wrapper.get("[data-detail-ports]").text()).toContain(
      "Utilise directement le réseau du serveur : aucun port individuel.",
    );
    expect(wrapper.get("[data-detail-mounts]").text()).toContain("Aucun dossier de données");
    await row(wrapper, "c_nu").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-detail-ports]").text()).toContain("Aucun port");
    expect(detail(wrapper).text()).not.toContain("Ressources : non limitées");
    wrapper.unmount();
  });

  it("le code de sortie est expliqué pour 0, 137 et 143, seul sinon ; en boucle, la bande des redémarrages", async () => {
    const { wrapper, bridge } = await open((sim) =>
      sim.services.seed("forge", [
        { key: "c_a", name: "a", state: "stopped", exitCode: 137 },
        { key: "c_b", name: "b", state: "error", exitCode: 2 },
        { key: "c_c", name: "c", state: "restart_loop", exitCode: 1, restarts10m: 4 },
      ]),
    );
    await row(wrapper, "c_a").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-detail-exit]").text()).toContain(
      "Code 137 : coupé de force (arrêt forcé ou manque de mémoire)",
    );
    bridge.services.upsert("forge", { key: "c_a", name: "a", exitCode: 143 });
    await flushPromises();
    expect(wrapper.get("[data-detail-exit]").text()).toContain("Code 143 : arrêt demandé");
    bridge.services.upsert("forge", { key: "c_a", name: "a", exitCode: 0 });
    await flushPromises();
    expect(wrapper.get("[data-detail-exit]").text()).toContain("Code 0 : terminé normalement");
    await row(wrapper, "c_b").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-detail-exit]").text().replace(/\s+/g, " ")).toContain("Code 2");
    expect(wrapper.get("[data-detail-exit]").text()).not.toContain(":");
    await row(wrapper, "c_c").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-detail-loop]").text()).toBe(
      "A redémarré 4 fois en 10 min. Dernier code de sortie : 1",
    );
    wrapper.unmount();
  });

  it("repères : créé hors de Hearth, adopté par {compte} le {date}, géré par le système, pile Compose", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [
        {
          key: "u_docker-x.service",
          name: "x",
          origin: "external",
          driver: "supervisor",
          unit: "docker-x.service",
          composeProject: "boutique",
          restart: "supervisor",
          adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
        },
      ]),
    );
    await row(wrapper, "u_docker-x.service").trigger("click");
    await flushPromises();
    const head = detail(wrapper).text();
    expect(head).toContain("Créé hors de Hearth");
    expect(head).toContain("Adopté");
    expect(head).toContain("Géré par le système (docker-x.service)");
    expect(head).toContain("Pile Compose boutique");
    expect(wrapper.get("[data-detail-adopted]").text()).toMatch(/^Adopté par claire le /);
    expect(wrapper.get("[data-detail-adopted-note]").text()).toBe(
      "Ce service a été créé hors de Hearth : sa configuration reste celle de son créateur.",
    );
    expect(head).toContain("Relancé par le système du serveur");
    wrapper.unmount();
  });

  it("un compte de l'agent qui n'a pas la forme d'un identifiant n'est jamais affiché", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [
        {
          key: "c_a",
          name: "a",
          origin: "external",
          adopted: { by: "<b>pirate</b> ‮", at: "2026-10-13T08:00:00Z" },
        },
      ]),
    );
    await row(wrapper, "c_a").trigger("click");
    await flushPromises();
    expect(detail(wrapper).text()).not.toContain("pirate");
    expect(wrapper.get("[data-detail-adopted]").text()).toMatch(/^Adopté le /);
    wrapper.unmount();
  });

  it("les boutons de pilotage sont ceux de l'état (HRT-65) ; l'onglet Journaux est actif (HRT-67)", async () => {
    const { wrapper, router } = await open(seedWeb);
    await openWeb(wrapper);
    expect(wrapper.find("[data-detail-actions-soon]").exists()).toBe(false);
    const buttons = detail(wrapper)
      .findAll("[data-lifecycle-action]")
      .map((button) => button.text());
    expect(buttons.join("|")).toBe("Arrêter|Redémarrer");
    const logs = wrapper.get("[data-tab='logs']");
    expect(logs.attributes("aria-disabled")).toBeUndefined();
    expect(wrapper.find("[data-detail-logs-soon]").exists()).toBe(false);
    await logs.trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.onglet).toBe("journaux");
    expect(wrapper.get("[data-tab='logs']").attributes("aria-selected")).toBe("true");
    expect(wrapper.find("[data-logs]").exists()).toBe(true);
    wrapper.unmount();
  });
});

describe("valeurs que ce client ne connaît pas : affichage neutre, jamais d'écran cassé", () => {
  it("état, protocole, périmètre, genre de dossier et redémarrage inconnus", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [
        {
          key: "c_f",
          name: "futur",
          state: "unknown",
          restart: "unknown",
          origin: "unknown",
          ports: [{ hostPort: 4433, containerPort: 443, protocol: "unknown", scope: "unknown" }],
          mounts: [{ kind: "unknown", source: "/s", target: "/t", readOnly: true }],
        },
      ]),
    );
    await row(wrapper, "c_f").trigger("click");
    await flushPromises();
    const text = detail(wrapper).text();
    expect(text).toContain("État non reconnu");
    expect(text).toContain("4433 vers 443");
    expect(text).not.toMatch(/unknown/i);
    expect(text).not.toContain("Joignable depuis");
    expect(text).toContain("Hearth ne reconnaît pas la façon dont ce service redémarre.");
    expect(wrapper.get("[data-detail-mounts]").text()).toContain("/s");
    expect(text).not.toContain("Créé hors de Hearth");
    wrapper.unmount();
  });
});

describe("les variables d'environnement (BR-SVC-008, BR-SVC-009)", () => {
  it("les noms sont là, les valeurs ABSENTES du document : masque fixe, aucune longueur révélée", async () => {
    const { wrapper } = await open(seedWeb);
    await openWeb(wrapper);
    const section = wrapper.get("[data-env-section]");
    expect(section.text()).toContain("SERVER_NAME");
    expect(section.text()).toContain("MOT_DE_PASSE");
    expect(section.text()).toContain("Valeurs masquées");
    expect(wrapper.html()).not.toContain(SECRET);
    expect(wrapper.html()).not.toContain("forge.exemple");
    const masks = section.findAll("[data-env-mask]");
    expect(masks).toHaveLength(3);
    for (const mask of masks) expect(mask.text()).toContain("••••••••");
    expect(section.find("[data-env-value]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("révéler : lecture consignée, valeurs visibles, « Valeurs visibles » ; masquer les efface de la mémoire", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    await reveal(wrapper);
    const section = wrapper.get("[data-env-section]");
    expect(bridge.services.reveals).toEqual([{ serverId: "forge", key: "c_web" }]);
    expect(section.text()).toContain(SECRET);
    expect(section.text()).toContain("forge.exemple");
    expect(section.text()).toContain("(valeur vide)");
    expect(wrapper.get("[data-env-status]").text()).toContain("Valeurs visibles");
    expect(wrapper.get("[data-env-toggle]").text()).toBe("Masquer les valeurs");
    await reveal(wrapper);
    expect(wrapper.html()).not.toContain(SECRET);
    expect(useServiceEnvStore().variables).toBeNull();
    expect(wrapper.get("[data-env-status]").text()).toContain("Valeurs masquées");
    wrapper.unmount();
  });

  it("jamais dans un titre, une infobulle, le stockage du navigateur ni le presse-papiers", async () => {
    const writeText = vi.fn();
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const { wrapper } = await open(seedWeb);
    await openWeb(wrapper);
    await reveal(wrapper);
    for (const element of wrapper.element.querySelectorAll("[title],[aria-label]")) {
      expect(element.getAttribute("title") ?? "").not.toContain(SECRET);
      expect(element.getAttribute("aria-label") ?? "").not.toContain(SECRET);
    }
    expect(JSON.stringify({ ...localStorage })).not.toContain(SECRET);
    expect(JSON.stringify({ ...sessionStorage })).not.toContain(SECRET);
    expect(writeText).not.toHaveBeenCalled();
    expect(wrapper.get("[data-env-section]").find("button:not([data-env-toggle])").exists()).toBe(
      false,
    );
    wrapper.unmount();
  });

  it.each([
    [
      "fermer le détail",
      async (w: ReturnType<typeof mount>) => w.get("[data-detail-back]").trigger("click"),
    ],
    [
      "ouvrir un autre service",
      async (w: ReturnType<typeof mount>) => row(w, "c_db").trigger("click"),
    ],
  ])("remasquage et effacement : %s", async (_label, act) => {
    const { wrapper } = await open(seedWeb);
    await openWeb(wrapper);
    await reveal(wrapper);
    expect(useServiceEnvStore().variables).not.toBeNull();
    await act(wrapper);
    await flushPromises();
    expect(useServiceEnvStore().variables).toBeNull();
    expect(useServiceEnvStore().phase).toBe("idle");
    expect(wrapper.html()).not.toContain(SECRET);
    wrapper.unmount();
  });

  it("remasquage : le lien n'est plus « Connecté »", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    await reveal(wrapper);
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(useServiceEnvStore().variables).toBeNull();
    expect(wrapper.html()).not.toContain(SECRET);
    expect(wrapper.get("[data-env-toggle]").attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("remasquage : le service disparaît, le détail reste lisible et grisé ; le démontage efface aussi", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    await reveal(wrapper);
    bridge.services.remove("forge", "c_web");
    await flushPromises();
    expect(useServiceEnvStore().variables).toBeNull();
    expect(wrapper.html()).not.toContain(SECRET);
    wrapper.unmount();
    expect(useServiceEnvStore().phase).toBe("idle");
  });

  it("une réponse tardive d'un service quitté entre-temps est jetée, jamais montrée", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const original = bridge.revealServiceEnv.bind(bridge);
    bridge.revealServiceEnv = async (serverId, key) => {
      await gate;
      return original(serverId, key);
    };
    await wrapper.get("[data-env-toggle]").trigger("click");
    expect(useServiceEnvStore().phase).toBe("reading");
    expect(wrapper.get("[data-env-toggle]").text()).toBe("Lecture des valeurs…");
    await row(wrapper, "c_db").trigger("click");
    await flushPromises();
    release();
    await flushPromises();
    expect(useServiceEnvStore().variables).toBeNull();
    expect(wrapper.html()).not.toContain(SECRET);
    wrapper.unmount();
  });

  it("lecture en échec : « Impossible de lire les valeurs. Réessayez. », valeurs masquées, on peut réessayer", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    bridge.services.failReveal = true;
    await reveal(wrapper);
    expect(wrapper.get("[data-env-failed]").text()).toBe(
      "Impossible de lire les valeurs. Réessayez.",
    );
    expect(wrapper.html()).not.toContain(SECRET);
    bridge.services.failReveal = false;
    await reveal(wrapper);
    expect(wrapper.find("[data-env-failed]").exists()).toBe(false);
    expect(wrapper.get("[data-env-section]").text()).toContain(SECRET);
    wrapper.unmount();
  });

  it("droit retiré entre-temps : le serveur refuse, « Tu n'as plus le droit… », le droit est relu et le bouton se désactive", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    const reads = bridge.rightsReads.length;
    bridge.setServiceRightOnServer("forge", "marie", false);
    await reveal(wrapper);
    expect(wrapper.get("[data-env-failed]").text()).toBe(
      "Vous n'avez plus le droit de gérer les services de ce serveur.",
    );
    expect(bridge.refusedReveals).toEqual(["forge"]);
    expect(bridge.rightsReads.length).toBeGreaterThan(reads);
    expect(bridge.services.reveals).toEqual([]);
    expect(wrapper.html()).not.toContain(SECRET);
    expect(wrapper.get("[data-env-toggle]").attributes("aria-disabled")).toBe("true");
    wrapper.unmount();
  });

  it("sans le droit (administrateur) : bouton désactivé avec la raison et la commande, jamais une valeur ni une demande", async () => {
    const { wrapper, bridge } = await open((sim) => {
      seedWeb(sim);
      sim.setServiceRightOnServer("forge", "marie", false);
    });
    await openWeb(wrapper);
    const toggle = wrapper.get("[data-env-toggle]");
    expect(toggle.attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-env-reason]").text()).toContain(
      "Votre compte ne peut pas gérer les services de ce serveur.",
    );
    expect(wrapper.get("[data-env-reason] pre").text()).toBe(
      "sudo hearth-agent account services marie --grant",
    );
    expect(toggle.attributes("aria-describedby")).toBeTruthy();
    await toggle.trigger("click");
    await flushPromises();
    expect(bridge.services.reveals).toEqual([]);
    expect(bridge.refusedReveals).toEqual([]);
    expect(wrapper.html()).not.toContain(SECRET);
    wrapper.unmount();
  });

  it("compte en lecture seule sans le droit : « Demandez à la personne qui administre ce serveur. »", async () => {
    const { wrapper } = await open(
      (sim) => {
        sim.services.seed("salon", [{ ...WEB }]);
        sim.services.setEnv("salon", "c_web", [{ name: "A", value: SECRET }]);
        sim.setServiceRightOnServer("salon", "paul", false);
      },
      { server: "salon" },
    );
    await openWeb(wrapper);
    expect(wrapper.get("[data-env-reason]").text()).toContain(
      "Demandez à la personne qui administre ce serveur.",
    );
    expect(wrapper.get("[data-env-reason]").find("pre").exists()).toBe(false);
    wrapper.unmount();
  });

  it("aucune variable : « Aucune variable » et pas de bouton superflu bloqué", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [{ key: "c_a", name: "a" }]),
    );
    await row(wrapper, "c_a").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-env-none]").text()).toBe("Aucune variable");
    wrapper.unmount();
  });

  it("variables coupées par l'agent : le reste est compté, jamais listé", async () => {
    const { wrapper } = await open((bridge) => {
      bridge.services.seed("forge", [
        { key: "c_a", name: "a", envNames: ["A"], envNamesOmitted: 7 },
      ]);
    });
    await row(wrapper, "c_a").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-env-omitted]").text()).toBe("et 7 autres non listées");
    wrapper.unmount();
  });

  it("service adopté, détail réservé : commande et variables remplacées par la phrase, le reste reste lisible", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [
        {
          key: "c_a",
          name: "adopte",
          origin: "external",
          adopted: { by: "claire", at: "2026-10-13T08:00:00Z" },
          detailsReserved: true,
        },
      ]),
    );
    await row(wrapper, "c_a").trigger("click");
    await flushPromises();
    const phrase = "Réservé aux comptes qui peuvent gérer les services.";
    expect(wrapper.get("[data-detail-command-reserved]").text()).toContain(phrase);
    expect(wrapper.get("[data-env-reserved]").text()).toContain(phrase);
    expect(wrapper.find("[data-env-toggle]").exists()).toBe(false);
    expect(wrapper.find("[data-detail-ports]").exists()).toBe(true);
    expect(detail(wrapper).text()).not.toContain("Commande par défaut du programme");
    wrapper.unmount();
  });
});

describe("le service disparaît (BR-SVC-016)", () => {
  it("lisible, grisé, « Ce service n'existe plus. » et le retour à la liste", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    bridge.services.remove("forge", "c_web");
    await flushPromises();
    expect(wrapper.get("[data-detail-gone]").text()).toBe("Ce service n'existe plus.");
    expect(detail(wrapper).attributes("data-gone")).toBeDefined();
    expect(wrapper.get("[data-detail-title]").text()).toBe("web");
    expect(wrapper.find("[data-detail-open-other]").exists()).toBe(false);
    await wrapper.get("[data-detail-back]").trigger("click");
    await flushPromises();
    expect(detail(wrapper).exists()).toBe(false);
    wrapper.unmount();
  });

  it("un homonyme créé ensuite n'est jamais ouvert d'office : il est proposé", async () => {
    const { wrapper, bridge, router } = await open(seedWeb);
    await openWeb(wrapper);
    bridge.services.remove("forge", "c_web");
    await flushPromises();
    bridge.services.upsert("forge", { key: "c_nouveau", name: "web" });
    await flushPromises();
    expect(router.currentRoute.value.query.service).toBe("c_web");
    expect(wrapper.find("[data-detail-gone]").exists()).toBe(true);
    expect(wrapper.get("[data-detail-recreated]").text()).toContain(
      "Un service nommé « web » existe de nouveau.",
    );
    await wrapper.get("[data-detail-open-other]").trigger("click");
    await flushPromises();
    expect(router.currentRoute.value.query.service).toBe("c_nouveau");
    expect(detail(wrapper).attributes("data-gone")).toBeUndefined();
    wrapper.unmount();
  });

  it("ouvert par l'adresse sur une clé qui n'existe pas : dit qu'il n'existe plus, sans nom inventé", async () => {
    const { wrapper } = await open(seedWeb, { query: "c_inconnu" });
    expect(wrapper.get("[data-detail-gone]").text()).toBe("Ce service n'existe plus.");
    expect(wrapper.find("[data-detail-title]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("liste pas encore arrivée : lignes esquissées, ni « disparu » ni faux détail", async () => {
    const { wrapper } = await open(undefined, { query: "c_web" });
    expect(wrapper.find("[data-detail-gone]").exists()).toBe(false);
    expect(wrapper.find("[data-service-detail]").exists()).toBe(false);
    wrapper.unmount();
  });
});

describe("fraîcheur : le détail partage le grisé et la datation de la liste", () => {
  it("moteur qui ne répond plus, lien bon : liste et détail grisés ensemble, datés, révélation désactivée avec sa raison", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    await reveal(wrapper);
    vi.setSystemTime(Date.now() + 90_000);
    bridge.services.setEngine("forge", { state: "unresponsive" });
    vi.advanceTimersByTime(1000);
    await flushPromises();
    const work = wrapper.get("[data-services-work]");
    expect(work.classes()).toContain("svc__work--grey");
    expect(work.find("[data-service-detail]").exists()).toBe(true);
    expect(wrapper.get("[data-services-stale]").text()).toBe("Liste du moteur d'il y a 1 min");
    expect(useServiceEnvStore().variables).toBeNull();
    expect(wrapper.get("[data-env-toggle]").attributes("aria-disabled")).toBe("true");
    expect(wrapper.get("[data-env-engine]").text()).toBe(
      "Indisponible tant que le moteur de conteneurs n'est pas prêt.",
    );
    wrapper.unmount();
  });

  it("lien coupé : le détail reste ouvert et lisible (données périmées), pas de seconde carte de droit", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    await openWeb(wrapper);
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(detail(wrapper).exists()).toBe(true);
    expect(wrapper.get("[data-detail-title]").text()).toBe("web");
    expect(wrapper.find("[data-services-rights]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("la carte « droit » de la section ne double pas la raison de la section Variables", async () => {
    const { wrapper } = await open((sim) => {
      seedWeb(sim);
      sim.setServiceRightOnServer("forge", "marie", false);
    });
    expect(wrapper.find("[data-services-rights]").exists()).toBe(true);
    await openWeb(wrapper);
    expect(wrapper.find("[data-services-rights]").exists()).toBe(false);
    expect(wrapper.findAll("[data-rights-reason]")).toHaveLength(1);
    wrapper.unmount();
  });
});

describe("la liste (HRT-137)", () => {
  it("« Créé hors de Hearth » s'explique au survol (et dans le détail pour le clavier)", async () => {
    const { wrapper } = await open((bridge) =>
      bridge.services.seed("forge", [{ key: "c_a", name: "a", origin: "external" }]),
    );
    const tag = wrapper.get("[data-row] .tag");
    expect(tag.attributes("title")).toBe(
      "Lancé en dehors de Hearth. Hearth le suit mais ne l'a pas créé.",
    );
    wrapper.unmount();
  });

  it("l'état vide dit ce que la liste montre et propose de déployer", async () => {
    const { wrapper } = await open((bridge) => bridge.services.seed("forge", []));
    const empty = wrapper.get("[data-services-empty]").text();
    expect(empty).toContain("Aucun service pour l'instant");
    expect(empty).toContain("Hearth affiche ici les services qu'il a créés");
    expect(empty).toContain("Déployer un service");
    wrapper.unmount();
  });

  it("serveur hors ligne : un seul message, pas de carte « Impossible de lire ton droit »", async () => {
    const { wrapper, bridge } = await open((sim) => {
      seedWeb(sim);
      sim.failRightsReads = true;
    });
    bridge.setState("forge", "offline");
    await flushPromises();
    expect(wrapper.find("[data-services-rights]").exists()).toBe(false);
    wrapper.unmount();
  });
});

// Une liste relue du disque au redémarrage hors ligne est écrite SANS la commande ni les noms de variables (ils
// peuvent porter des secrets) : leur absence n'y est pas « aucune variable » ni « commande par défaut » (revue de la PR 93).
describe("liste relue du disque : une donnée absente n'est pas une donnée vide", () => {
  const NOT_READ_COMMAND =
    "Pas encore lue : elle se lira quand le lien avec le serveur sera revenu.";
  const NOT_READ_ENV = "Pas encore lus : ils se liront quand le lien avec le serveur sera revenu.";

  it("sans lecture fraîche, ni « Commande par défaut » ni « Aucune variable » : une phrase qui dit pourquoi", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    // Ce que fait la coquille d'un fichier du disque : ni commande ni noms, et l'événement le dit.
    bridge.services.setDetailsKnown("forge", false);
    await flushPromises();
    await openWeb(wrapper);
    const text = detail(wrapper).text();
    expect(text).toContain(NOT_READ_COMMAND);
    expect(text).toContain(NOT_READ_ENV);
    expect(text).not.toContain("Commande par défaut du programme");
    expect(text).not.toContain("Aucune variable");
    expect(wrapper.find("[data-env-mask]").exists()).toBe(false);
    wrapper.unmount();
  });

  it("le lien revient et le serveur répond : la commande et les noms reviennent, la phrase part", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    bridge.services.setDetailsKnown("forge", false);
    await flushPromises();
    await openWeb(wrapper);
    expect(detail(wrapper).text()).toContain(NOT_READ_COMMAND);
    // Un état reçu du serveur (la liste est relue en entier) porte ses détails.
    seedWeb(bridge);
    await flushPromises();
    const text = detail(wrapper).text();
    expect(text).not.toContain(NOT_READ_COMMAND);
    expect(text).not.toContain(NOT_READ_ENV);
    expect(wrapper.get("[data-detail-command]").text()).toBe("nginx -g daemon off;");
    expect(wrapper.findAll("[data-env-mask]")).toHaveLength(3);
    wrapper.unmount();
  });

  it("le magasin porte le repère de chaque service (état restauré depuis le disque)", async () => {
    const { wrapper, bridge } = await open(seedWeb);
    bridge.services.setDetailsKnown("forge", false);
    await flushPromises();
    const services = useServicesStore().of("forge")?.services ?? [];
    expect(services.length).toBeGreaterThan(0);
    expect(services.every((service) => !service.detailsKnown)).toBe(true);
    expect(
      services.every((service) => service.command === null && service.envNames.length === 0),
    ).toBe(true);
    wrapper.unmount();
  });
});

// Règle prudente en attendant la décision du propriétaire : la commande (elle peut porter un secret) et les noms de
// variables ne sont montrés qu'aux comptes qui peuvent gérer les services, quel que soit le service.
describe("commande et noms de variables : réservés au droit de gérer les services", () => {
  it("un service créé par Hearth, compte sans le droit : ni commande ni noms, une phrase qui dit pourquoi", async () => {
    const { wrapper } = await open((sim) => {
      seedWeb(sim);
      sim.setServiceRightOnServer("forge", "marie", false);
    });
    await openWeb(wrapper);
    expect(wrapper.find("[data-detail-command]").exists()).toBe(false);
    expect(detail(wrapper).text()).not.toContain("daemon off");
    expect(detail(wrapper).text()).not.toContain("SERVER_NAME");
    expect(wrapper.get("[data-detail-command-reserved]").text()).toContain(
      "Réservé aux comptes qui peuvent gérer les services.",
    );
    expect(wrapper.get("[data-env-note]").text()).toContain(
      "Réservé aux comptes qui peuvent gérer les services.",
    );
    wrapper.unmount();
  });

  it("avec le droit : la commande et les noms sont montrés", async () => {
    const { wrapper } = await open(seedWeb);
    await openWeb(wrapper);
    expect(wrapper.get("[data-detail-command]").text()).toBe("nginx -g daemon off;");
    expect(wrapper.get("[data-env-list]").text()).toContain("SERVER_NAME");
    wrapper.unmount();
  });
});

describe("texte venu du serveur : rendu inoffensif et borné", () => {
  const BIDI = String.fromCharCode(0x202e);
  const ZERO = String.fromCharCode(0x200b);
  const MARK = String.fromCharCode(0xb7);

  it("un caractère bidirectionnel ou invisible devient un repère visible, dans le détail ET dans la liste", async () => {
    const { wrapper } = await open((bridge) => {
      bridge.services.seed("forge", [
        {
          key: "c_h",
          name: `hostile${BIDI}etoN`,
          origin: "hearth",
          image: { ref: `reg${ZERO}istre/img`, tag: `1${BIDI}` },
          command: [`run${BIDI}`, `--x=${ZERO}y`],
          mounts: [{ kind: "folder", source: `/a${BIDI}b`, target: `/c${ZERO}d`, readOnly: false }],
          unit: `u${BIDI}`,
          driver: "supervisor",
          composeProject: `p${ZERO}q`,
        },
      ]);
      bridge.services.setEnv("forge", "c_h", [
        { name: `N${BIDI}`, value: `v${BIDI}${ZERO}\nligne` },
      ]);
    });
    await row(wrapper, "c_h").trigger("click");
    await flushPromises();
    await reveal(wrapper);
    const unsafe = new RegExp(`[${BIDI}${ZERO}]`);
    expect(wrapper.element.textContent ?? "").not.toMatch(unsafe);
    expect(wrapper.html()).not.toMatch(unsafe);
    expect(wrapper.get("[data-detail-title]").text()).toBe(`hostile${MARK}etoN`);
    expect(wrapper.get("[data-detail-command]").text()).toBe(`run${MARK} --x=${MARK}y`);
    // La valeur révélée garde ses retours à la ligne, rien d'autre.
    expect(wrapper.get("[data-env-value]").text()).toBe(`v${MARK}${MARK}\nligne`);
    expect(wrapper.get("[data-row][data-key='c_h']").attributes("aria-label")).not.toMatch(unsafe);
    wrapper.unmount();
  });

  it("un texte de 64 Ko : la commande, l'image, les noms et les valeurs sont dans une zone bornée", async () => {
    const BIG = "x".repeat(65_536);
    const { wrapper } = await open((bridge) => {
      bridge.services.seed("forge", [
        { key: "c_big", name: "gros", origin: "hearth", command: [BIG], image: { ref: BIG } },
      ]);
      bridge.services.setEnv("forge", "c_big", [{ name: BIG, value: BIG }]);
    });
    await row(wrapper, "c_big").trigger("click");
    await flushPromises();
    // La mise en page est MESURÉE par Playwright (hrt63-detail.spec.ts) ; ici : chaque texte long est dans une zone bornée.
    expect(wrapper.find("[data-detail-command] [data-bounded]").exists()).toBe(true);
    expect(wrapper.find("[data-detail-image] [data-bounded]").exists()).toBe(true);
    expect(wrapper.find("[data-env-list] [data-bounded]").exists()).toBe(true);
    wrapper.unmount();
  });
});
