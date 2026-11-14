import { describe, expect, it } from "vitest";
import type { FicheStop, GamePort, GameView, ServiceOperation } from "@/link";
import {
  addressOf,
  decisionReason,
  gameEffectText,
  isStopOperation,
  portRows,
  portsChanged,
  portsListText,
  roleText,
  sequenceSteps,
  stagesOf,
  stopCardOf,
  stopWay,
  worldSaved,
} from "@/services/game";

// Le quotidien d'un serveur de jeu (HRT-104, BR-JEU-004, 010, 017 à 021) : des fonctions pures.
const stop = (patch: Partial<FicheStop> = {}): FicheStop => ({
  graceSeconds: 30,
  worldSaved: "by_sequence",
  sequenceTested: true,
  ...patch,
});
const view = (patch: Partial<FicheStop> = {}): GameView => ({
  fiche: "terraria",
  name: "Terraria",
  ficheVersion: 1,
  stop: stop(patch),
  ports: [],
  hasPassword: false,
});
const port = (hostPort: number, patch: Partial<GamePort> = {}): GamePort => ({
  hostPort,
  protocol: "tcp",
  role: "game",
  changed: false,
  ...patch,
});
const operation = (patch: Partial<ServiceOperation> = {}): ServiceOperation => ({
  id: "op1",
  key: "h_web",
  kind: "stop",
  status: "running",
  step: null,
  steps: [],
  percent: null,
  itemsDone: null,
  bytesDone: null,
  bytesTotal: null,
  etaS: null,
  cancellable: true,
  decision: null,
  cause: null,
  ...patch,
});

describe("la façon d'arrêter (BR-JEU-002, BR-JEU-034)", () => {
  it("une suite essayée s'envoie, une image qui enregistre n'a rien à recevoir, le reste se décide", () => {
    expect(stopWay(stop())).toBe("sequence");
    expect(stopWay(stop({ worldSaved: "by_image_on_signal", sequenceTested: false }))).toBe(
      "image",
    );
    // Une suite non essayée n'est jamais envoyée (BR-JEU-038), un jeu qui n'enregistre pas non plus.
    expect(stopWay(stop({ sequenceTested: false }))).toBe("ask");
    expect(stopWay(stop({ worldSaved: "not_saved" }))).toBe("ask");
    expect(stopWay(stop({ worldSaved: "unknown" }))).toBe("ask");
  });
});

describe("la confirmation d'un arrêt (BR-JEU-004, BR-JEU-007)", () => {
  it("dit la phrase de la fiche, avec le délai de la fiche", () => {
    expect(gameEffectText("stop", view())).toBe(
      "Hearth va d'abord demander à Terraria d'enregistrer son monde et attendre qu'il l'ait fait, puis arrêter le service (au plus 30 secondes pour l'arrêt). Tout ce qui est arrivé jusqu'à cet enregistrement sera conservé.",
    );
    expect(gameEffectText("stop", view({ worldSaved: "by_image_on_signal" }))).toBe(
      "Terraria enregistre son monde en s'arrêtant. Il lui faut jusqu'à 30 secondes ; passé ce délai, le service est coupé de force et ce qu'il n'a pas eu le temps d'enregistrer peut être perdu.",
    );
    const asking = gameEffectText("stop", view({ sequenceTested: false }));
    expect(asking).toContain("Hearth ne peut pas demander à Terraria d'enregistrer son monde.");
    expect(asking).toContain("arrêter quand même ou d'annuler");
  });

  it("n'écrit jamais la phrase générale d'un service sans ordre de sauvegarde", () => {
    for (const patch of [
      {},
      { worldSaved: "by_image_on_signal" as const },
      { sequenceTested: false },
    ]) {
      for (const gesture of ["stop", "restart"] as const) {
        expect(gameEffectText(gesture, view(patch))).not.toContain("ne peut pas encore envoyer");
      }
    }
  });

  it("un redémarrage dit qu'il relance le service", () => {
    expect(gameEffectText("restart", view())).toContain("et le relancer");
    expect(gameEffectText("restart", view({ sequenceTested: false }))).toContain(
      "redémarrez maintenant",
    );
  });

  it("un nom de jeu venu du serveur est rendu inoffensif", () => {
    const hostile = { ...view(), name: "Terra‮ria" };
    expect(gameEffectText("stop", hostile)).not.toContain("‮");
  });
});

describe("la carte « Arrêt du jeu » (BR-JEU-010)", () => {
  it("dit en permanence qu'un arrêt hors de Hearth n'enregistre que par l'enregistrement automatique", () => {
    const card = stopCardOf(view());
    expect(card.main).toBe(
      "Ce jeu n'enregistre pas son monde en s'arrêtant : Hearth lui demande de l'enregistrer avant.",
    );
    expect(card.outside).toBe(
      "Si le serveur ou le moteur arrête ce jeu sans passer par Hearth, le monde n'est enregistré que par l'enregistrement automatique du jeu.",
    );
    expect(card.grace).toBe("Délai d'arrêt : 30 secondes");
  });

  it("un jeu qui enregistre lui-même dit le délai et n'a pas la phrase de l'arrêt extérieur", () => {
    const card = stopCardOf(view({ worldSaved: "by_image_on_signal", graceSeconds: 1 }));
    expect(card.main).toBe(
      "Ce jeu enregistre son monde en s'arrêtant. Il lui faut jusqu'à 1 seconde.",
    );
    expect(card.outside).toBeNull();
  });

  it("un jeu à qui Hearth ne peut rien demander le dit", () => {
    expect(stopCardOf(view({ sequenceTested: false })).main).toContain(
      "Hearth ne peut pas encore lui demander",
    );
  });
});

describe("les ports à ouvrir (BR-JEU-017, BR-JEU-019)", () => {
  it("sont rangés par numéro, avec protocole et rôle en mots", () => {
    const rows = portRows([port(7778, { protocol: "udp", role: "query" }), port(7777)]);
    expect(rows.map((row) => [row.port, row.protocol, row.role])).toEqual([
      [7777, "TCP", "Port du jeu"],
      [7778, "UDP", "Port de requêtes"],
    ]);
  });

  it("n'invente rien pour un rôle ou un protocole que ce client ne connaît pas", () => {
    expect(roleText("something-new")).toBe("Port du service");
    expect(roleText("")).toBe("Port du service");
    expect(portRows([port(7777, { protocol: "quic" })])).toEqual([]);
  });

  it("copie « port/protocole », une ligne par port", () => {
    expect(portsListText([port(7778, { protocol: "udp" }), port(7777)])).toBe("7777/tcp\n7778/udp");
  });

  it("le rappel existe dès qu'un port diffère de celui de la fiche", () => {
    expect(portsChanged([port(7777), port(7778)])).toBe(false);
    expect(portsChanged([port(7777), port(7778, { changed: true })])).toBe(true);
  });
});

describe("l'adresse pour les joueurs (BR-JEU-020)", () => {
  it("prend le port de base du jeu et dit quand d'autres ports servent aussi", () => {
    expect(addressOf("192.168.1.20", [port(7777)])).toEqual({
      text: "192.168.1.20:7777",
      others: false,
    });
    expect(
      addressOf("forge.local", [
        port(27016, { role: "query" }),
        port(27015),
        port(27015, { protocol: "udp" }),
      ]),
    ).toEqual({ text: "forge.local:27015", others: true });
  });

  it("préfère le port de rôle « jeu », sinon le plus petit", () => {
    expect(addressOf("h", [port(5000, { role: "query" }), port(6000)])?.text).toBe("h:6000");
    expect(
      addressOf("h", [port(6000, { role: "query" }), port(5000, { role: "voice" })])?.text,
    ).toBe("h:5000");
  });

  it("écrit une adresse IPv6 entre crochets et ne dit rien sans port publié", () => {
    expect(addressOf("fd00::1", [port(7777)])?.text).toBe("[fd00::1]:7777");
    expect(addressOf("forge", [])).toBeNull();
    expect(addressOf("forge", [port(7777, { protocol: "quic" })])).toBeNull();
  });
});

describe("la séquence d'un arrêt (BR-JEU-003)", () => {
  const plan = { save: true, restart: false };

  it("a « Sauvegarde du monde » puis « Arrêt », et « Démarrage » pour un redémarrage", () => {
    expect(stagesOf(null, plan)).toEqual(["save_world", "stop"]);
    expect(stagesOf(null, { save: true, restart: true })).toEqual(["save_world", "stop", "start"]);
    // Un jeu qui enregistre lui-même n'a pas d'étape d'enregistrement.
    expect(stagesOf(null, { save: false, restart: false })).toEqual(["stop"]);
  });

  it("suit les étapes que l'agent a réellement ouvertes plutôt que ce que la vue annonçait", () => {
    const opened = operation({
      step: "save_world",
      steps: [{ step: "save_world", state: "intent" }],
    });
    expect(stagesOf(opened, { save: false, restart: false })).toEqual(["save_world", "stop"]);
  });

  it("marque faites les étapes d'avant l'étape en cours", () => {
    const steps = sequenceSteps(operation({ step: "stop" }), plan, null);
    expect(steps.map((step) => [step.key, step.state])).toEqual([
      ["save_world", "done"],
      ["stop", "now"],
    ]);
    expect(steps.map((step) => step.label)).toEqual(["Sauvegarde du monde", "Arrêt"]);
  });

  it("ne met la barre que sur l'étape en cours demandée", () => {
    const steps = sequenceSteps(operation({ step: "stop" }), plan, {
      stage: "stop",
      text: "12 secondes",
    });
    expect(steps[0]?.bar).toBeUndefined();
    expect(steps[1]?.bar).toEqual({ ratio: null, text: "12 secondes" });
  });

  it("sait que le monde est enregistré quand l'étape est faite", () => {
    expect(worldSaved(operation({ steps: [{ step: "save_world", state: "done" }] }))).toBe(true);
    expect(worldSaved(operation({ steps: [{ step: "save_world", state: "intent" }] }))).toBe(false);
    expect(worldSaved(operation())).toBe(false);
  });
});

describe("la décision d'un arrêt (BR-JEU-006, BR-JEU-007)", () => {
  it("avant tout envoi, Hearth ne peut pas demander ; après l'ordre, il n'a pas pu s'assurer", () => {
    expect(decisionReason(operation({ status: "awaiting_decision" }))).toBe("cannot-ask");
    expect(
      decisionReason(
        operation({
          status: "awaiting_decision",
          steps: [{ step: "save_world", state: "intent" }],
        }),
      ),
    ).toBe("not-confirmed");
  });

  it("seuls l'arrêt et le redémarrage sont des arrêts", () => {
    expect(isStopOperation(operation({ kind: "stop" }))).toBe(true);
    expect(isStopOperation(operation({ kind: "restart" }))).toBe(true);
    expect(isStopOperation(operation({ kind: "update" }))).toBe(false);
  });
});
