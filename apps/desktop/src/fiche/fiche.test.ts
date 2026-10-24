import { describe, expect, it } from "vitest";
import type { FicheDetail, FicheQuestion } from "@/link";
import { rich, terraria } from "@/link/simulated-fiches";
import { type EngineCapacity, GIB } from "@/resources";
import { answerIssue, answersToSend, visibleQuestions } from "./answers";
import { acceptOf, allValidated, attentionPoints } from "./attention";
import {
  defaultVariant,
  newFicheDraft,
  shownHostPort,
  toInstallRequest,
  wipeFicheDraft,
} from "./draft";
import { expectedBytes, firstStartView } from "./firstStart";
import { coverage, diskVerdict, memoryNotices, minimumVerdicts, placement, tooSmall } from "./fit";
import { registryOf } from "./image";
import { answerIssueText, ficheFormRefusal, minimumText, stopTexts } from "./messages";
import { proposedName, slugOf } from "./name";
import { duplicateHostPorts, parseHostPort, publishedPorts, readableHostPorts } from "./ports";
import { isMeasured, proofOf, proofUrl, proven } from "./proof";

// Les règles pures de l'installation depuis une fiche (HRT-102) : la preuve d'un chiffre, les réponses jugées comme
// l'agent, les ports, le nom proposé, la machine et la place, les points d'attention, le premier démarrage. Aucun jeu
// n'est nommé dans les règles : les fiches d'essai portent tout.

const machine = (cpus: number, memoryGib: number, committedGib = 0): EngineCapacity => ({
  cpus,
  memoryBytes: memoryGib * GIB,
  reserveBytes: GIB,
  committedMemoryBytes: committedGib * GIB,
});

const question = (patch: Partial<FicheQuestion>): FicheQuestion => ({
  id: "q",
  label: "Question",
  help: "",
  secret: false,
  proposed: null,
  kind: "text",
  choices: [],
  min: null,
  max: null,
  minLen: null,
  maxLen: null,
  charset: null,
  frozenAfterCreation: false,
  variants: [],
  secretKind: null,
  obtainUrl: null,
  ...patch,
});

describe("la preuve d'un chiffre (BR-FICHE-008)", () => {
  it("ne donne un nombre qu'avec une preuve, et lit une preuve inconnue comme non vérifiée", () => {
    expect(proven(2048, { kind: "measured" })).toBe(2048);
    expect(proven(2048, { kind: "publisher", url: "https://example.org/x" })).toBe(2048);
    expect(proven(2048, { kind: "unmeasured" })).toBeNull();
    expect(proven(2048, { kind: "unknown" })).toBeNull();
    expect(proven(null, { kind: "measured" })).toBeNull();
    expect(proofOf({ kind: "unknown" })).toBe("unmeasured");
  });

  it("ne tient pour mesuré que ce que Hearth a mesuré, au repos compris", () => {
    expect(isMeasured({ kind: "measured" })).toBe(true);
    expect(isMeasured({ kind: "measured_idle" })).toBe(true);
    expect(isMeasured({ kind: "publisher", url: "https://example.org/x" })).toBe(false);
    expect(proofUrl({ kind: "publisher", url: "https://example.org/x" })).toBe(
      "https://example.org/x",
    );
    expect(proofUrl({ kind: "measured" })).toBeNull();
  });
});

describe("les réponses, jugées sur ce que la fiche dit (BR-FICHE-016, 017)", () => {
  const fileStem = question({ kind: "text", charset: "file_stem", maxLen: 10, minLen: 1 });

  it("juge un texte sur le jeu de caractères et la longueur de la fiche, et cite UN caractère refusé", () => {
    expect(answerIssue(fileStem, "Monde_1")).toBeNull();
    expect(answerIssue(fileStem, "")).toEqual({ code: "required" });
    expect(answerIssue(fileStem, "Mon monde")).toEqual({ code: "charset", character: " " });
    expect(answerIssue(fileStem, "é")).toEqual({ code: "charset", character: "é" });
    expect(answerIssue(fileStem, "a".repeat(11))).toEqual({ code: "too_long", max: 10 });
    const line = question({ charset: "line", maxLen: 5 });
    expect(answerIssue(line, "")).toBeNull();
    expect(answerIssue(line, "a\nb")).toEqual({ code: "multiline" });
    expect(answerIssue(line, "a\u200Bb")).toEqual({ code: "charset", character: "\u200B" });
  });

  it("refuse un argument qui commence comme un drapeau", () => {
    const arg = question({ charset: "arg_safe" });
    expect(answerIssue(arg, "base-1.2")).toBeNull();
    expect(answerIssue(arg, "-x")).toEqual({ code: "charset", character: "-" });
    expect(answerIssue(arg, "+x")).toEqual({ code: "charset", character: "+" });
  });

  it("juge un nombre entier dans les bornes de la fiche, sans zéros ni signe de trop", () => {
    const int = question({ kind: "int", min: 1, max: 255 });
    expect(answerIssue(int, "8")).toBeNull();
    expect(answerIssue(int, "")).toEqual({ code: "required" });
    expect(answerIssue(int, "08")).toEqual({ code: "not_integer" });
    expect(answerIssue(int, "1.5")).toEqual({ code: "not_integer" });
    expect(answerIssue(int, "0")).toEqual({ code: "out_of_range", min: 1, max: 255 });
    expect(answerIssue(int, "256")).toEqual({ code: "out_of_range", min: 1, max: 255 });
  });

  it("n'accepte qu'un choix de la fiche, et jamais un caractère d'un secret", () => {
    const choice = question({
      kind: "choice",
      choices: [
        { value: "1", label: "Petit" },
        { value: "2", label: "Moyen" },
      ],
    });
    expect(answerIssue(choice, "2")).toBeNull();
    expect(answerIssue(choice, "9")).toEqual({ code: "bad_choice" });
    const secret = question({ kind: "secret", secret: true, maxLen: 8, minLen: 0 });
    expect(answerIssue(secret, "")).toBeNull();
    expect(answerIssue(secret, "a\u0007b")).toEqual({ code: "charset", character: null });
    expect(answerIssue(secret, "x".repeat(9))).toEqual({ code: "too_long", max: 8 });
    const token = question({ kind: "secret", secret: true, minLen: 1 });
    expect(answerIssue(token, "")).toEqual({ code: "required" });
  });

  it("met chaque défaut en phrase sans jamais citer la valeur d'un secret", () => {
    expect(answerIssueText({ code: "charset", character: "é" })).toContain("é");
    expect(answerIssueText({ code: "charset", character: null })).not.toContain("null");
    expect(answerIssueText({ code: "out_of_range", min: 1, max: 255 })).toContain("1 et 255");
    expect(answerIssueText({ code: "out_of_range", min: 2, max: null })).toContain("au moins 2");
  });

  it("ne pose que les questions de la variante choisie, et n'envoie que les réponses non vides", () => {
    const fiche = rich();
    expect(visibleQuestions(fiche, "vanilla").map((item) => item.id)).not.toContain("mod_pack");
    expect(visibleQuestions(fiche, "modded").map((item) => item.id)).toContain("mod_pack");
    expect(visibleQuestions(fiche, null).map((item) => item.id)).not.toContain("mod_pack");
    const sent = answersToSend(visibleQuestions(fiche, "vanilla"), { server_token: "abc" });
    expect(sent.find((item) => item.id === "server_token")).toEqual({
      id: "server_token",
      value: "abc",
    });
    expect(sent.find((item) => item.id === "mod_pack")).toBeUndefined();
    // Une valeur proposée part, un secret vide ne part pas.
    expect(sent.find((item) => item.id === "slots")?.value).toBe("10");
    expect(
      answersToSend(visibleQuestions(terraria(), null), {}).map((item) => item.id),
    ).not.toContain("password");
  });
});

describe("le nom du service proposé (BR-FICHE-015)", () => {
  it("réduit le nom du jeu à ce qu'un nom de service accepte", () => {
    expect(slugOf("Terraria")).toBe("terraria");
    expect(slugOf("Jeu d'Essai Évolué")).toBe("jeu-d-essai-evolue");
    expect(slugOf("§§§")).toBe("jeu");
  });

  it("ajoute -2, -3… jusqu'au premier nom libre, sans tenir compte de la casse", () => {
    expect(proposedName("Terraria", [])).toBe("terraria");
    expect(proposedName("Terraria", ["Terraria"])).toBe("terraria-2");
    expect(proposedName("Terraria", ["terraria", "terraria-2"])).toBe("terraria-3");
  });
});

describe("les ports d'une fiche (BR-FICHE-019)", () => {
  const ports = rich().ports;

  it("publie les ports « toujours » et les facultatifs cochés, jamais ceux qui ne le sont pas", () => {
    expect(publishedPorts(ports, []).map((port) => port.id)).toEqual(["game"]);
    expect(publishedPorts(ports, ["query"]).map((port) => port.id)).toEqual(["game", "query"]);
  });

  it("juge le port du serveur : un nombre entre 1024 et 65535", () => {
    expect(parseHostPort("25565")).toEqual({ port: 25565 });
    expect(parseHostPort("")).toEqual({ issue: "required" });
    expect(parseHostPort("abc")).toEqual({ issue: "not_a_number" });
    expect(parseHostPort("80")).toEqual({ issue: "range" });
    expect(parseHostPort("70000")).toEqual({ issue: "range" });
  });

  it("dit les ports lisibles et repère deux ports de même numéro et de même protocole", () => {
    const both = publishedPorts(ports, ["query"]);
    expect(readableHostPorts(both, { game: "25565" }).map((item) => item.deploy.hostPort)).toEqual([
      25565, 25566,
    ]);
    expect(duplicateHostPorts(both, { game: "30000", query: "30000" }).size).toBe(0);
    const twin = [
      { ...ports[0], id: "a" },
      { ...ports[0], id: "b" },
    ] as typeof ports;
    expect([...duplicateHostPorts(twin, { a: "30000", b: "30000" })].sort()).toEqual(["a", "b"]);
  });
});

describe("la machine et la place (BR-FICHE-020 à 023)", () => {
  it("ne bloque que sur un minimum MESURÉ, avertit sur le chiffre de l'éditeur, dit « non vérifié » sinon", () => {
    const fiche = rich();
    // 1 Gio : sous le minimum mesuré au repos de la mémoire (2 Gio).
    const small = minimumVerdicts(fiche, machine(8, 1));
    expect(tooSmall(small)).toMatchObject({
      kind: "too_small",
      lack: "memory",
      proof: "measured_idle",
    });
    // 8 processeurs, 16 Gio : tout est couvert.
    expect(minimumVerdicts(fiche, machine(8, 16))).toEqual([]);
    // Le chiffre de l'éditeur au-dessus de la machine avertit sans bloquer.
    const publisher: FicheDetail = structuredClone(fiche);
    publisher.resources.memory = {
      minBytes: 32 * GIB,
      recommendedBytes: null,
      evidence: { kind: "publisher", url: "https://example.org/x" },
    };
    const warned = minimumVerdicts(publisher, machine(8, 16));
    expect(warned).toEqual([
      { kind: "publisher_above", lack: "memory", needed: 32 * GIB, has: 16 * GIB },
    ]);
    expect(tooSmall(warned)).toBeNull();
    // Terraria n'a aucune preuve : « non vérifié », jamais un blocage.
    const unproven = minimumVerdicts(terraria(), machine(2, 1));
    expect(unproven).toEqual([
      { kind: "unproven", lack: "memory" },
      { kind: "unproven", lack: "cpu" },
    ]);
    expect(tooSmall(unproven)).toBeNull();
  });

  it("n'écrit la phrase « trop petit » qu'avec ses chiffres et sa preuve", () => {
    const verdict = tooSmall(minimumVerdicts(rich(), machine(8, 1)));
    expect(verdict).not.toBeNull();
    const text = minimumText(verdict as NonNullable<typeof verdict>);
    expect(text).toContain("Votre serveur est trop petit pour ce jeu");
    expect(text).toContain("2 Go");
    expect(text).toContain("au repos");
  });

  it("ne dit si la machine couvre le recommandé que lorsque ce chiffre est prouvé", () => {
    expect(coverage(terraria(), machine(8, 16))).toEqual([]);
    expect(coverage(rich(), machine(8, 16)).map((item) => item.state)).toEqual([
      "covers",
      "covers",
    ]);
    expect(coverage(rich(), machine(1, 3)).map((item) => item.state)).toEqual([
      "below_recommended",
      "below_recommended",
    ]);
  });

  it("refuse sans case un besoin d'espace mesuré, avertit sur celui de l'éditeur", () => {
    const fiche = rich();
    expect(diskVerdict(fiche, 100 * GIB)).toEqual({ kind: "ok" });
    expect(diskVerdict(fiche, null)).toEqual({ kind: "unknown" });
    expect(diskVerdict(fiche, 2 * GIB)).toEqual({
      kind: "short_publisher",
      needed: 8 * GIB,
      free: 2 * GIB,
    });
    const measured: FicheDetail = structuredClone(fiche);
    measured.resources.disk.evidence = { kind: "measured" };
    expect(diskVerdict(measured, 2 * GIB)).toEqual({
      kind: "short_measured",
      needed: 8 * GIB,
      free: 2 * GIB,
      missing: 6 * GIB,
    });
    expect(diskVerdict(terraria(), 1)).toEqual({ kind: "unknown" });
  });

  it("place les curseurs sur le recommandé prouvé, sinon sur la proposition prudente et tous les processeurs", () => {
    const placed = placement(rich(), machine(8, 16));
    expect(placed.resources).toEqual({ memoryBytes: 4 * GIB, cpuMillis: 2000 });
    expect(placed.memory).toMatchObject({
      recommended: 4 * GIB,
      proof: "measured_idle",
      clamped: false,
    });
    // Recommandé au-dessus de la machine : ramené au maximum, et la page le dit.
    const clamped = placement(rich(), machine(4, 3));
    expect(clamped.memory.clamped).toBe(true);
    expect(clamped.resources.memoryBytes).toBeLessThanOrEqual(3 * GIB);
    // Aucune preuve : proposition prudente, tous les processeurs.
    const bare = placement(terraria(), machine(4, 16));
    expect(bare.memory.recommended).toBeNull();
    expect(bare.resources.cpuMillis).toBe(4000);
    expect(bare.cpu.recommended).toBeNull();
  });

  it("dit en chiffres ce qu'il reste de mémoire et ce qui dépasse", () => {
    const fiche = rich();
    // 16 Gio, 1 Gio de réserve, 14 Gio déjà engagés : il reste 1 Gio.
    const tight = machine(8, 16, 14);
    const notices = memoryNotices(fiche, tight, 4 * GIB);
    expect(notices.map((notice) => notice.kind)).toEqual(["remaining", "over_committed"]);
    expect(memoryNotices(fiche, machine(8, 16), 1 * GIB).map((notice) => notice.kind)).toEqual([
      "below_minimum",
    ]);
    // Sans recommandation prouvée, la phrase de reste est toujours dite, sans chiffre de fiche.
    expect(memoryNotices(terraria(), machine(8, 16), null)).toMatchObject([
      { kind: "remaining", recommended: null },
    ]);
  });
});

describe("le brouillon et la demande (BR-FICHE-013, BR-FICHE-034)", () => {
  it("part de la variante proposée et de ce que la fiche propose, sans rien accepter d'avance", () => {
    const fiche = rich();
    expect(defaultVariant(fiche)).toBe("vanilla");
    expect(defaultVariant(terraria())).toBeNull();
    const draft = newFicheDraft(fiche, "essai");
    expect(draft).toMatchObject({
      ficheId: "essai",
      version: 3,
      variant: "vanilla",
      edited: false,
    });
    expect(shownHostPort(draft, fiche.ports[0] as FicheDetail["ports"][number])).toBe("25565");
  });

  it("construit la demande avec la version et l'empreinte lues, les contrats acceptés et les cases validées", () => {
    const fiche = rich();
    const draft = newFicheDraft(fiche, "essai");
    draft.resources = { memoryBytes: 4 * GIB, cpuMillis: 2000 };
    draft.answers.server_token = "jeton";
    draft.optionalPorts.push("query");
    const request = toInstallRequest(fiche, draft, {
      accept: { reservedPorts: true, lowDisk: false },
      agreements: ["licence"],
    });
    expect(request).toMatchObject({
      name: "essai",
      fiche: "essai",
      version: 3,
      digest: fiche.digest,
      variant: "vanilla",
      agreements: ["licence"],
      optionalPorts: ["query"],
      resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 },
      accept: { reservedPorts: true, lowDisk: false },
    });
    expect(request?.ports).toEqual([
      { id: "game", hostPort: 25565 },
      { id: "query", hostPort: 25566 },
    ]);
    expect(request?.answers.find((item) => item.id === "server_token")?.value).toBe("jeton");
  });

  it("ne part pas sans nom valable, sans port lisible ni sans ressources réglées", () => {
    const fiche = rich();
    const draft = newFicheDraft(fiche, "essai");
    const choices = { accept: { reservedPorts: false, lowDisk: false }, agreements: [] };
    expect(toInstallRequest(fiche, draft, choices)).toBeNull();
    draft.resources = { memoryBytes: 4 * GIB, cpuMillis: 2000 };
    expect(toInstallRequest(fiche, draft, choices)).not.toBeNull();
    draft.hostPorts.game = "abc";
    expect(toInstallRequest(fiche, draft, choices)).toBeNull();
    draft.hostPorts.game = "25565";
    draft.name = "x";
    expect(toInstallRequest(fiche, draft, choices)).toBeNull();
  });

  it("efface la saisie, secrets compris", () => {
    const fiche = terraria();
    const draft = newFicheDraft(fiche, "terraria");
    draft.answers.password = "SENTINEL-secret";
    draft.hostPorts.game = "7778";
    wipeFicheDraft(draft);
    expect(draft.answers).toEqual({});
    expect(draft.hostPorts).toEqual({});
    expect(draft.name).toBe("");
    expect(JSON.stringify(draft)).not.toContain("SENTINEL");
  });
});

describe("les points d'attention (BR-FICHE-026)", () => {
  const reservedCheck = {
    name: "ok" as const,
    invalidFields: [],
    ports: [
      {
        hostPort: 25565,
        protocol: "tcp" as const,
        status: "reserved_by_stopped" as const,
        holder: "vieux",
        suggested: null,
        privileged: false,
      },
    ],
    disk: { freeBytes: 100 * GIB, low: false },
    imagePresent: false,
  };

  it("ne demande une case que pour ce qui s'applique, dans l'ordre", () => {
    const fiche = rich();
    const none = attentionPoints({
      detail: fiche,
      capacity: machine(8, 16),
      check: { ...reservedCheck, ports: [] },
      resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 },
      forceLowDisk: null,
    });
    expect(none).toEqual([]);
    const points = attentionPoints({
      detail: fiche,
      capacity: machine(8, 16, 14),
      check: reservedCheck,
      resources: { memoryBytes: GIB, cpuMillis: 2000 },
      forceLowDisk: null,
    });
    expect(points.map((point) => point.key)).toEqual(
      ["reserved_ports", "over_committed", "below_minimum"].filter(
        (key) => key !== "over_committed",
      ),
    );
  });

  it("n'envoie à l'agent que les cases qu'il comprend, et seulement si elles étaient demandées", () => {
    const fiche = rich();
    const points = attentionPoints({
      detail: fiche,
      capacity: machine(8, 16, 15),
      check: { ...reservedCheck, disk: { freeBytes: 2 * GIB, low: true } },
      resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 },
      forceLowDisk: null,
    });
    const keys = points.map((point) => point.key);
    expect(keys).toEqual(expect.arrayContaining(["reserved_ports", "low_disk", "over_committed"]));
    expect(allValidated(points, { reserved_ports: true })).toBe(false);
    const ticked = Object.fromEntries(keys.map((key) => [key, true]));
    expect(allValidated(points, ticked)).toBe(true);
    expect(acceptOf(points, { over_committed: true })).toEqual({
      reservedPorts: false,
      lowDisk: false,
    });
    expect(acceptOf(points, ticked)).toEqual({ reservedPorts: true, lowDisk: true });
    // Une case cochée d'un point qui ne s'applique pas n'est pas envoyée.
    expect(acceptOf([], { low_disk: true })).toEqual({ reservedPorts: false, lowDisk: false });
  });

  it("exige la case du manque de place quand le serveur vient de le refuser", () => {
    const points = attentionPoints({
      detail: terraria(),
      capacity: null,
      check: null,
      resources: null,
      forceLowDisk: { freeBytes: null },
    });
    expect(points).toEqual([{ key: "low_disk", freeBytes: null, neededBytes: null }]);
  });

  it("demande « Je comprends » pour un minimum de l'éditeur que la machine n'atteint pas", () => {
    const publisher: FicheDetail = structuredClone(rich());
    publisher.resources.memory = {
      minBytes: 32 * GIB,
      recommendedBytes: null,
      evidence: { kind: "publisher", url: "https://example.org/x" },
    };
    const points = attentionPoints({
      detail: publisher,
      capacity: machine(8, 16),
      check: null,
      resources: { memoryBytes: 4 * GIB, cpuMillis: 2000 },
      forceLowDisk: null,
    });
    expect(points[0]).toMatchObject({ key: "publisher_minimum", lack: "memory" });
  });
});

describe("le premier démarrage (BR-FICHE-056)", () => {
  const first = rich().firstStart as NonNullable<FicheDetail["firstStart"]>;

  it("ne dit que des faits : l'étape reconnue, les octets écrits, le total attendu quand il est prouvé", () => {
    const view = firstStartView(first, {
      stage: "Chargement du monde",
      writtenBytes: 2 * GIB,
      elapsedSeconds: 100,
    });
    expect(view.kind).toBe("panel");
    if (view.kind !== "panel") return;
    expect(view.lines[0]).toContain("Ce n'est pas une panne");
    expect(view.lines.some((line) => line.includes("Étape en cours : Chargement du monde"))).toBe(
      true,
    );
    expect(view.lines.at(-1)).toContain("déjà écrits, sur environ 6 Go");
    // Aucun pourcentage, aucune durée restante, aucune barre.
    expect(view.lines.join(" ")).not.toMatch(/%|restant|reste/);
  });

  it("ignore une étape que la fiche ne déclare pas et n'invente aucun total", () => {
    const view = firstStartView(
      { ...first, stages: [], downloads: [] },
      { stage: "Inconnue", writtenBytes: GIB, elapsedSeconds: 1 },
    );
    expect(view.kind).toBe("panel");
    if (view.kind !== "panel") return;
    expect(view.lines.some((line) => line.includes("Étape"))).toBe(false);
    expect(view.lines.at(-1)).toBe("1 Go de fichiers de jeu déjà écrits.");
    expect(
      expectedBytes({
        ...first,
        downloads: [{ what: "x", source: "y", bytes: 5, evidence: { kind: "unmeasured" } }],
      }),
    ).toBeNull();
  });

  it("remplace le panneau quand la durée annoncée est dépassée", () => {
    const view = firstStartView(first, {
      stage: null,
      writtenBytes: null,
      elapsedSeconds: first.readyTimeoutSeconds + 1,
    });
    expect(view).toEqual({
      kind: "unknown",
      text: "Hearth ne peut pas savoir si le jeu est prêt.",
    });
  });
});

describe("l'image et l'arrêt du jeu", () => {
  it("déduit le registre du dépôt, le registre public par défaut sinon", () => {
    expect(registryOf("ryshe/terraria")).toBeNull();
    expect(registryOf("terraria")).toBeNull();
    expect(registryOf("ghcr.io/org/jeu")).toBe("ghcr.io");
    expect(registryOf("localhost/jeu")).toBe("localhost");
    expect(registryOf("registre.local:5000/jeu")).toBe("registre.local:5000");
  });

  it("dit la façon d'arrêter le jeu et ne promet rien d'une suite non essayée", () => {
    const untested = stopTexts({
      graceSeconds: 10,
      worldSaved: "by_sequence",
      sequenceTested: false,
    });
    expect(untested[0]).toContain("Hearth ne peut pas encore lui demander");
    expect(untested[1]).toContain("10 secondes");
    expect(
      stopTexts({ graceSeconds: 30, worldSaved: "by_sequence", sequenceTested: true })[0],
    ).toContain("Hearth lui demande de l'enregistrer");
    expect(
      stopTexts({ graceSeconds: 30, worldSaved: "by_image_on_signal", sequenceTested: true })[0],
    ).toContain("jusqu'à 30 secondes");
  });
});

describe("les refus de l'agent propres aux fiches", () => {
  it("nomme la question refusée par son libellé, jamais par sa valeur", () => {
    const fiche = terraria();
    const refusal = ficheFormRefusal(
      { kind: "answer_refused", id: "world_name", reason: "charset", character: "é" },
      fiche,
    );
    expect(refusal?.question).toBe("world_name");
    expect(refusal?.text).toContain("Nom du monde");
    expect(refusal?.text).toContain("é");
    expect(
      ficheFormRefusal(
        { kind: "answer_refused", id: null, reason: "unknown_id", character: null },
        fiche,
      )?.text,
    ).toBe("Une réponse n'est pas valable. Cette réponse n'est pas valable pour ce jeu.");
  });

  it("dit une machine trop petite et un manque de place mesuré, et laisse le contrat à sa fenêtre", () => {
    const fiche = rich();
    expect(
      ficheFormRefusal(
        { kind: "host_too_small", lack: "memory", needed: 4 * GIB, has: 2 * GIB },
        fiche,
      )?.text,
    ).toContain("4 Go");
    expect(
      ficheFormRefusal({ kind: "host_too_small", lack: "memory", needed: null, has: null }, fiche)
        ?.text,
    ).toBe("Votre serveur est trop petit pour ce jeu.");
    expect(
      ficheFormRefusal(
        { kind: "no_space_to_install", neededBytes: 100, freeBytes: 40, missingBytes: 60 },
        fiche,
      )?.text,
    ).toContain("Libérez");
    expect(ficheFormRefusal({ kind: "agreement_required", ids: ["licence"] }, fiche)).toBeNull();
    expect(ficheFormRefusal({ kind: "name_taken" }, fiche)).toBeNull();
  });
});
