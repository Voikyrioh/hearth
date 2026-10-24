import { beforeEach, describe, expect, it } from "vitest";
import type {
  GameSettingsView,
  SaveOutcome,
  SaveSettingsRequest,
  SimulatedLinkBridge,
} from "@/link";
import { freshBridge } from "@/test/app";
import { IMPOSED_STAND_IN } from "./simulated-game-settings";

// L'agent d'essai des réglages juge COMME LE VRAI (HRT-108, ADR-0051) : même ordre de refus, `file_stamp` obligatoire, un
// écart détecté par empreinte, des lignes inconnues et des commentaires gardés, un secret qui ne revient jamais. Le client
// (HRT-109) se prouve contre lui : s'il était plus indulgent que l'agent, un test passerait ici et échouerait en vrai.

const KEY = "h_SIMTERRARIA";
const SERVER = "forge";
const SENTINEL = "SENTINELLE-9f3a-pas-dans-la-vue";

let bridge: SimulatedLinkBridge;

beforeEach(() => {
  bridge = freshBridge().bridge;
  const detail = bridge.catalog.get("terraria");
  if (!detail) throw new Error("Terraria est la fiche de départ");
  bridge.gameSettings.register(SERVER, KEY, detail, [
    { id: "password", value: SENTINEL },
    { id: "max_players", value: "12" },
  ]);
});

function view(): GameSettingsView {
  const read = bridge.gameSettings.read(SERVER, KEY);
  if (read.kind !== "found") throw new Error("des réglages étaient attendus");
  return read.view;
}

function request(patch: Partial<SaveSettingsRequest> = {}): SaveSettingsRequest {
  const current = view();
  return {
    baseRev: current.rev,
    fileStamp: current.fileStamp ?? "",
    origin: "form",
    effect: "restart",
    changes: [],
    others: [],
    driftChoices: [],
    importText: null,
    ...patch,
  };
}

const save = (patch: Partial<SaveSettingsRequest>): SaveOutcome =>
  bridge.gameSettings.save(SERVER, KEY, request(patch), "marie");

const refusal = (outcome: SaveOutcome) => (outcome.kind === "refused" ? outcome.refusal : null);

describe("lire", () => {
  it("rend une valeur par réglage de la fiche, jamais un secret", () => {
    const current = view();
    expect(current.settings).toHaveLength(32);
    const password = current.settings.find((setting) => setting.id === "password");
    expect(password).toMatchObject({ state: "secret_set", value: null });
    expect(JSON.stringify(current)).not.toContain(SENTINEL);
    expect(current.settings.find((s) => s.id === "max_players")).toMatchObject({
      state: "written",
      value: "12",
    });
    expect(current.settings.find((s) => s.id === "port")?.imposed).toBe(true);
    expect(current.settings.find((s) => s.id === "announcement_box_off")?.effect).toBe("recreate");
    expect(current.settings.find((s) => s.id === "motd")?.effect).toBe("restart");
  });

  it("dit « pas de réglages » d'un service qui n'est pas né d'une fiche", () => {
    expect(bridge.gameSettings.read(SERVER, "c_ordinaire")).toEqual({ kind: "absent" });
  });
});

describe("enregistrer, dans l'ordre de l'agent", () => {
  it("exige l'empreinte du fichier : sans elle, rien n'est écrit", () => {
    expect(refusal(save({ fileStamp: "" }))).toEqual({ kind: "invalid" });
  });

  it("refuse une révision périmée, avec qui a changé", () => {
    bridge.gameSettings.bumpRev(SERVER, KEY, "paul");
    const outcome = bridge.gameSettings.save(SERVER, KEY, { ...request(), baseRev: 0 }, "marie");
    expect(refusal(outcome)).toEqual({ kind: "conflict", by: "paul", effectMismatch: false });
  });

  it("refuse un fichier qui a bougé depuis la lecture, sans rien remettre par-dessus", () => {
    const before = request({ changes: [{ id: "motd", action: "set", value: "Salut" }] });
    bridge.gameSettings.rewriteFile(SERVER, KEY, "# main\nmotd=Autre\nmaxplayers=12");
    expect(refusal(bridge.gameSettings.save(SERVER, KEY, before, "marie"))).toMatchObject({
      kind: "file_changed",
    });
    expect(bridge.gameSettings.fileText(SERVER, KEY)).toBe("# main\nmotd=Autre\nmaxplayers=12");
  });

  it("juge tous les changements, tous ou aucun, et ne nomme jamais la valeur", () => {
    const outcome = save({
      changes: [
        { id: "motd", action: "set", value: "Salut" },
        { id: "max_players", action: "set", value: "999" },
      ],
    });
    expect(refusal(outcome)).toEqual({
      kind: "setting_refused",
      id: "max_players",
      reason: "bounds",
      character: null,
    });
    // Rien du premier changement n'est parti.
    expect(view().settings.find((s) => s.id === "motd")?.state).toBe("unwritten");
    expect(JSON.stringify(outcome)).not.toContain("999");
  });

  it("refuse un réglage imposé, un réglage figé, un réglage inconnu et la remise d'un secret", () => {
    const reasonOf = (id: string, action: "set" | "reset" = "set") => {
      const found = refusal(
        save({
          // La remise d'un secret n'est refusée que dans une remise (origine `reset`), comme l'agent.
          origin: action === "reset" ? "reset" : "form",
          changes: [{ id, action, value: action === "set" ? "1" : null }],
        }),
      );
      return found?.kind === "setting_refused" ? found.reason : found?.kind;
    };
    expect(reasonOf("port")).toBe("imposed");
    expect(reasonOf("world_name")).toBe("frozen");
    expect(reasonOf("nope")).toBe("unknown_id");
    expect(reasonOf("password", "reset")).toBe("secret");
  });

  it("cite au plus UN caractère refusé d'un texte, jamais un caractère d'un secret", () => {
    const text = refusal(save({ changes: [{ id: "motd", action: "set", value: "a\u0007b" }] }));
    expect(text).toMatchObject({ reason: "charset", character: "\u0007" });
    const secret = refusal(
      save({ changes: [{ id: "password", action: "set", value: "a\u0007" }] }),
    );
    expect(secret).toMatchObject({ reason: "charset", character: null });
  });

  it("n'applique pas un réglage reçu par argument : il demande de recréer le service", () => {
    const outcome = save({
      effect: "recreate",
      changes: [{ id: "announcement_box_off", action: "set", value: "1" }],
    });
    expect(refusal(outcome)).toEqual({
      kind: "recreate_not_available",
      ids: ["announcement_box_off"],
    });
  });

  it("recalcule l'effet signé et refuse un effet qui diffère", () => {
    const outcome = save({
      effect: "none",
      changes: [{ id: "motd", action: "set", value: "Salut" }],
    });
    expect(refusal(outcome)).toEqual({ kind: "conflict", by: null, effectMismatch: true });
  });

  it("retouche le fichier sans le réécrire : commentaires et lignes inconnues gardent leurs octets", () => {
    bridge.gameSettings.rewriteFile(
      SERVER,
      KEY,
      "# écrit à la main\nmaxplayers=12\nmod_inconnu=2\n; autre commentaire\n#motd=ancien",
    );
    // Hearth est d'accord avec le fichier tel qu'il est : on garde sa valeur pour l'écart de maxplayers... (aucun écart ici
    // car la valeur voulue est 12).
    const current = view();
    const outcome = bridge.gameSettings.save(
      SERVER,
      KEY,
      {
        ...request(),
        baseRev: current.rev,
        fileStamp: current.fileStamp ?? "",
        driftChoices: current.drifts.map((drift) => ({
          id: drift.id,
          choice: "keep_file" as const,
        })),
        changes: [{ id: "motd", action: "set", value: "Salut" }],
      },
      "marie",
    );
    expect(outcome.kind).toBe("saved");
    const text = bridge.gameSettings.fileText(SERVER, KEY);
    expect(text).toContain("# écrit à la main");
    expect(text).toContain("mod_inconnu=2");
    expect(text).toContain("; autre commentaire");
    // Une ligne commentée reste un commentaire : régler le réglage ajoute une vraie ligne.
    expect(text).toContain("#motd=ancien");
    expect(text).toContain("motd=Salut");
  });

  it("ne compte pas un changement qui ne change rien, et contrôle l'effet avant de rendre « rien »", () => {
    const same = { changes: [{ id: "max_players", action: "set" as const, value: "12" }] };
    // Aucun effet : l'agent recalcule « none » et refuse « restart » AVANT le retour à vide.
    expect(refusal(save({ ...same, effect: "restart" }))).toEqual({
      kind: "conflict",
      by: null,
      effectMismatch: true,
    });
    const outcome = save({ ...same, effect: "none" });
    expect(outcome).toMatchObject({
      kind: "saved",
      fileWritten: false,
      changed: 0,
      effect: "none",
    });
    // Rien n'a changé : la révision n'avance pas.
    expect(view().rev).toBe(0);
  });

  it("juge d'abord les changements, puis l'empreinte : une valeur fausse sur un fichier qui a bougé est refusée comme valeur", () => {
    const stale = request({ changes: [{ id: "max_players", action: "set", value: "999" }] });
    // Le jeu ajoute un commentaire : le fichier a bougé, aucun réglage connu n'est en écart.
    bridge.gameSettings.rewriteFile(
      SERVER,
      KEY,
      `${bridge.gameSettings.fileText(SERVER, KEY)}\n# main`,
    );
    expect(refusal(bridge.gameSettings.save(SERVER, KEY, stale, "marie"))).toMatchObject({
      kind: "setting_refused",
      id: "max_players",
      reason: "bounds",
    });
  });

  it("garde un secret sans jamais le rendre, même à l'enregistrement qui le change", () => {
    const outcome = save({ changes: [{ id: "password", action: "set", value: "NEUF-secret" }] });
    expect(outcome.kind).toBe("saved");
    expect(JSON.stringify(outcome)).not.toContain("NEUF-secret");
    expect(JSON.stringify(view())).not.toContain("NEUF-secret");
  });
});

describe("l'écart quand le jeu a réécrit son fichier (BR-FICHE-048)", () => {
  beforeEach(() => {
    bridge.gameSettings.rewriteFile(
      SERVER,
      KEY,
      "# réécrit par le jeu\nmaxplayers=20\nmotd=Bienvenue\nmod_inconnu=2",
    );
  });

  it("le dit avec la valeur de Hearth et celle du fichier, jamais un secret", () => {
    const current = view();
    expect(current.fileChanged).toBe(true);
    const drift = current.drifts.find((item) => item.id === "max_players");
    expect(drift).toEqual({ id: "max_players", hearth: "12", file: "20", fileValid: true });
    expect(current.drifts.find((item) => item.id === "password")).toBeUndefined();
    expect(current.others).toEqual([{ key: "mod_inconnu", value: "2", masked: false }]);
  });

  it("exige un choix par réglage en écart avant d'écrire quoi que ce soit", () => {
    const outcome = save({ changes: [] });
    expect(refusal(outcome)).toMatchObject({ kind: "file_changed" });
    expect(bridge.gameSettings.fileText(SERVER, KEY)).toContain("maxplayers=20");
  });

  it("juge l'écart non tranché avant une clé connue donnée comme autre réglage", () => {
    const outcome = save({ others: [{ key: "motd", value: "x", remove: false }] });
    expect(refusal(outcome)).toMatchObject({ kind: "file_changed" });
  });

  it("garde la valeur du fichier, ou remet celle de Hearth, réglage par réglage", () => {
    const current = view();
    const ids = current.drifts.map((drift) => drift.id);
    expect(ids).toContain("max_players");
    const keep = save({
      effect: "none",
      driftChoices: ids.map((id) => ({
        id,
        choice: id === "max_players" ? ("keep_file" as const) : ("keep_file" as const),
      })),
    });
    expect(keep).toMatchObject({ kind: "saved", changed: 0 });
    expect(view().settings.find((s) => s.id === "max_players")?.value).toBe("20");
  });

  it("remet la valeur de Hearth par-dessus celle du fichier quand on le demande", () => {
    const ids = view().drifts.map((drift) => drift.id);
    const outcome = save({
      effect: "restart",
      driftChoices: ids.map((id) => ({ id, choice: "keep_mine" as const })),
    });
    expect(outcome.kind).toBe("saved");
    expect(bridge.gameSettings.fileText(SERVER, KEY)).toContain("maxplayers=12");
    expect(view().drifts).toEqual([]);
  });

  it("refuse « Garder la valeur du fichier » quand le réglage n'accepte pas cette valeur", () => {
    bridge.gameSettings.rewriteFile(SERVER, KEY, "maxplayers=9999\nmotd=Bienvenue");
    const current = view();
    const drift = current.drifts.find((item) => item.id === "max_players");
    expect(drift?.fileValid).toBe(false);
    const outcome = save({
      driftChoices: current.drifts.map((item) => ({ id: item.id, choice: "keep_file" as const })),
    });
    expect(refusal(outcome)).toMatchObject({ kind: "setting_refused", id: "max_players" });
  });
});

describe("un écart et un changement du même réglage", () => {
  it("ignore « Garder la valeur du fichier » pour un réglage que la demande change : sa valeur gagne", () => {
    bridge.gameSettings.rewriteFile(SERVER, KEY, "maxplayers=20");
    const current = view();
    const outcome = save({
      changes: [{ id: "max_players", action: "set", value: "30" }],
      driftChoices: current.drifts.map((drift) => ({ id: drift.id, choice: "keep_file" as const })),
    });
    expect(outcome.kind).toBe("saved");
    expect(outcome.kind === "saved" && outcome.adopted).not.toContain("max_players");
    expect(view().settings.find((s) => s.id === "max_players")?.value).toBe("30");
    expect(bridge.gameSettings.fileText(SERVER, KEY)).toContain("maxplayers=30");
  });
});

describe("lire un texte collé", () => {
  const analyze = (text: string) => bridge.gameSettings.analyze(SERVER, KEY, text);

  it("un texte collé sans le réglage imposé le défait, avec la valeur posée à l'installation il ne le défait pas", () => {
    const outcome = analyze("motd=Salut");
    if (outcome.kind !== "read") throw new Error("une analyse était attendue");
    expect(outcome.analysis.imposedUndone).toContain("world_path");
    const kept = analyze(`motd=Salut
worldpath=${IMPOSED_STAND_IN}`);
    if (kept.kind !== "read") throw new Error("une analyse était attendue");
    expect(kept.analysis.imposedUndone).not.toContain("world_path");
  });

  it("dit réglage par réglage ce que le texte apporterait, sans rien écrire", () => {
    const outcome = analyze(
      "motd=Salut\nmaxplayers=12\nlanguage=en/US\nport=1234\nworldname=Autre\npassword=secret-collé\nzzz=1",
    );
    if (outcome.kind !== "read") throw new Error("lu attendu");
    const state = (id: string) => outcome.analysis.lines.find((line) => line.id === id);
    expect(state("motd")).toMatchObject({ state: "take", pasted: "Salut" });
    expect(state("max_players")?.state).toBe("same");
    expect(state("language")).toMatchObject({ state: "rejected", reason: "choice" });
    expect(state("port")?.state).toBe("imposed");
    expect(state("world_name")?.state).toBe("frozen");
    expect(state("password")).toMatchObject({ state: "take", secret: true, pasted: null });
    expect(outcome.analysis.unknown).toBe(1);
    expect(outcome.analysis.imposedUndone).toContain("port");
    expect(JSON.stringify(outcome)).not.toContain("secret-collé");
    expect(view().settings.find((s) => s.id === "motd")?.state).toBe("unwritten");
  });

  it("rend UNE ligne par réglage et compte des clés distinctes (une clé répétée, une inconnue répétée)", () => {
    const outcome = analyze("motd=a\nmotd=b\nzzz=1\nzzz=2\nyyy=3");
    if (outcome.kind !== "read") throw new Error("lu attendu");
    expect(outcome.analysis.lines.filter((line) => line.id === "motd")).toHaveLength(1);
    expect(outcome.analysis.lines.find((line) => line.id === "motd")?.pasted).toBe("b");
    expect(outcome.analysis.unknown).toBe(2);
    expect(outcome.analysis.duplicates).toBe(2);
  });

  it("refuse un texte que le format du jeu ne sait pas lire, en nommant la ligne", () => {
    expect(analyze("motd=Salut\nceci n'est pas une ligne")).toEqual({
      kind: "unreadable",
      reason: "no_separator",
      line: 2,
    });
  });

  it("applique la valeur reprise du texte à l'enregistrement, mot de passe collé compris", () => {
    const outcome = save({
      origin: "import",
      importText: "motd=Salut\npassword=venu-du-texte",
      changes: [
        { id: "motd", action: "from_import", value: null },
        { id: "password", action: "from_import", value: null },
      ],
    });
    expect(outcome.kind).toBe("saved");
    expect(view().settings.find((s) => s.id === "motd")?.value).toBe("Salut");
    expect(JSON.stringify(view())).not.toContain("venu-du-texte");
  });
});
