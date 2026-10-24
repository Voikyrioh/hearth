import { describe, expect, it } from "vitest";
import { fr } from "@/i18n/fr";
import type { GameSettingsView, ImportAnalysis, SettingValue } from "@/link";
import { rich, terraria } from "@/link/simulated-fiches";
import {
  type Edits,
  editIssues,
  effectiveOf,
  heaviestEffect,
  isModified,
  listIssue,
  listText,
  listValue,
  pendingEdits,
  resettable,
  toSaveRequest,
} from "./edits";
import { groupByTheme, isCommon, matches, opensByDefault, plain, shown } from "./filter";
import { importRows, toImportRequest, undoneLabels } from "./importing";
import { installModelsOf, modelsOf, type SettingModel } from "./model";

// Les règles pures de l'onglet « Réglages » (HRT-109, BR-FICHE-037, 041, 043, 044, 045, 048, 052), sur la fiche réelle de
// Terraria (32 réglages, sept thèmes) et sur la fiche d'essai qui porte tout (liste d'identifiants, texte opaque,
// réglage d'une variante, défaut non vérifié).

function settingValue(id: string, patch: Partial<SettingValue> = {}): SettingValue {
  return {
    id,
    state: "unwritten",
    value: null,
    frozen: false,
    imposed: false,
    gone: false,
    effect: "restart",
    ...patch,
  };
}

/** La vue d'un service de Terraria installé avec ses valeurs proposées. */
function terrariaView(patch: Partial<GameSettingsView> = {}): GameSettingsView {
  const detail = terraria();
  return {
    rev: 0,
    fiche: { id: "terraria", version: 2, digest: detail.digest },
    fileState: "ok",
    fileStamp: "stamp",
    fileChanged: false,
    rawEdit: true,
    settings: detail.settings.map((setting) => {
      const question = setting.question;
      return settingValue(question.id, {
        state: question.secret ? "unwritten" : question.proposed !== null ? "written" : "unwritten",
        value: question.secret ? null : question.proposed,
        frozen: question.frozenAfterCreation,
        imposed: setting.imposedWhy !== null,
        effect: setting.key.startsWith("-") ? "recreate" : "restart",
      });
    }),
    drifts: [],
    others: [],
    imposedBroken: [],
    ...patch,
  };
}

function terrariaModels(patch: Partial<GameSettingsView> = {}): SettingModel[] {
  return modelsOf(terraria().settings, terrariaView(patch));
}

const byId = (models: readonly SettingModel[], id: string): SettingModel => {
  const found = models.find((model) => model.id === id);
  if (!found) throw new Error(`réglage ${id} absent`);
  return found;
};

describe("le catalogue de Terraria rendu lisible (objectif de Voiky)", () => {
  it("a 32 réglages rangés dans 7 thèmes, tous nommés par la fiche", () => {
    const detail = terraria();
    expect(detail.settings).toHaveLength(32);
    expect(detail.themes.map((theme) => theme.label)).toEqual([
      "Ma partie",
      "Joueurs",
      "Monde",
      "Réseau",
      "Triche et sécurité",
      "Mode Voyage",
      "Performances",
    ]);
    const themes = new Set(detail.themes.map((theme) => theme.id));
    expect(detail.settings.every((setting) => themes.has(setting.theme))).toBe(true);
  });

  it("verrouille ce que la fiche impose, ce qui est figé et ce qui demande de recréer le service", () => {
    const models = terrariaModels();
    expect(byId(models, "port").lock).toBe("imposed");
    expect(byId(models, "world_file").lock).toBe("imposed");
    expect(byId(models, "world_name").lock).toBe("frozen");
    expect(byId(models, "announcement_box_off").lock).toBe("recreate");
    expect(byId(models, "motd").lock).toBeNull();
    // Dix-sept réglages se règlent encore (les 15 du mode Voyage, et le reste de la partie).
    expect(models.filter((model) => model.lock === null).length).toBeGreaterThan(20);
  });

  it("ne lit jamais la valeur d'un secret et le dit seulement défini ou non", () => {
    const view = terrariaView({
      settings: terrariaView().settings.map((value) =>
        value.id === "password" ? { ...value, state: "secret_set", value: "FUITE" } : value,
      ),
    });
    const models = modelsOf(terraria().settings, view);
    const password = byId(models, "password");
    expect(password.state).toBe("secret_set");
    expect(password.value).toBeNull();
    expect(effectiveOf(password, undefined)).toEqual({
      written: true,
      value: null,
      secretKept: true,
    });
  });
});

describe("modifié, remis, remise en bloc (BR-FICHE-041, 044)", () => {
  it("est modifié quand la valeur diffère de la valeur proposée, et pas avant", () => {
    const models = terrariaModels();
    const players = byId(models, "max_players");
    expect(isModified(players, undefined)).toBe(false);
    expect(isModified(players, { action: "set", value: "12" })).toBe(true);
    expect(isModified(players, { action: "set", value: "8" })).toBe(false);
    // Sans valeur proposée, « non écrit » est la valeur proposée : écrire, c'est modifier.
    const motd = byId(models, "motd");
    expect(isModified(motd, undefined)).toBe(false);
    expect(isModified(motd, { action: "set", value: "Bonjour" })).toBe(true);
    expect(isModified(motd, { action: "reset" })).toBe(false);
  });

  it("ne remet en bloc ni un réglage figé, ni un imposé, ni un secret, ni celui qui attend la recréation", () => {
    const models = terrariaModels();
    const touch: Edits[string] = { action: "set", value: "x" };
    for (const id of ["world_name", "port", "password", "announcement_box_off"]) {
      expect(resettable(byId(models, id), touch)).toBe(false);
    }
    expect(resettable(byId(models, "max_players"), { action: "set", value: "12" })).toBe(true);
    expect(resettable(byId(models, "max_players"), undefined)).toBe(false);
  });
});

describe("chercher, filtrer, ranger (BR-FICHE-043, 045)", () => {
  const models = terrariaModels();
  const all = { query: "", modifiedOnly: false, scope: "all" as const };

  it("cherche sans tenir compte des majuscules ni des accents, par libellé, nom technique et phrase", () => {
    expect(plain("Fluidité DES ennemis")).toBe("fluidite des ennemis");
    expect(matches(byId(models, "npc_stream"), "FLUIDITE")).toBe(true);
    expect(matches(byId(models, "npc_stream"), "npcstream")).toBe(true);
    expect(matches(byId(models, "npc_stream"), "saccades")).toBe(true);
    expect(matches(byId(models, "npc_stream"), "difficulte")).toBe(false);
    expect(shown(models, { ...all, query: "difficulte" }, {}).map((m) => m.id)).toContain(
      "difficulty",
    );
  });

  it("montre les courants par défaut, tout avec la bascule, et cherche toujours dans tous les niveaux", () => {
    const common = shown(models, { ...all, scope: "common" }, {});
    expect(common.every(isCommon)).toBe(true);
    expect(common.length).toBeLessThan(models.length);
    expect(shown(models, all, {})).toHaveLength(models.length);
    // « Fluidité des ennemis » est avancé : la recherche le trouve même en vue « Courants ».
    expect(common.map((model) => model.id)).not.toContain("npc_stream");
    expect(
      shown(models, { query: "fluidité", modifiedOnly: false, scope: "common" }, {}).map(
        (model) => model.id,
      ),
    ).toEqual(["npc_stream"]);
  });

  it("garde les modifiés, et range par thème dans l'ordre de la fiche avec le compte", () => {
    const edits: Edits = { motd: { action: "set", value: "Bonjour" } };
    expect(shown(models, { ...all, modifiedOnly: true }, edits).map((m) => m.id)).toEqual(["motd"]);
    const detail = terraria();
    const groups = groupByTheme(detail.themes, models, shown(models, all, edits), edits);
    expect(groups.map((group) => group.theme.id)).toEqual([
      "game",
      "players",
      "world",
      "network",
      "anticheat",
      "journey",
      "performance",
    ]);
    const game = groups[0];
    expect(game?.modified).toBe(1);
    expect(game && opensByDefault(game)).toBe(true);
    // Le mode Voyage n'a que des réglages courants ou avancés, aucun essentiel, aucun modifié : replié.
    const journey = groups.find((group) => group.theme.id === "journey");
    expect(journey?.total).toBe(15);
    expect(journey && opensByDefault(journey)).toBe(false);
  });

  it("n'oublie jamais un réglage dont le thème est inconnu : il est rangé à la fin", () => {
    const detail = terraria();
    const orphan: SettingModel[] = [
      ...models,
      {
        ...byId(models, "motd"),
        id: "stray",
        setting: { ...byId(models, "motd").setting, theme: "ghost" },
      },
    ];
    const groups = groupByTheme(detail.themes, orphan, orphan, {});
    expect(groups.at(-1)?.models.map((model) => model.id)).toEqual(["stray"]);
  });
});

describe("l'enregistrement (BR-FICHE-041, 042, 048)", () => {
  const models = terrariaModels();
  const base = { baseRev: 3, fileStamp: "abc", drifts: [], choices: {}, others: [] };

  it("n'envoie que ce qui change, avec l'effet le plus lourd, la révision et l'empreinte lues", () => {
    const edits: Edits = {
      motd: { action: "set", value: "Bonjour" },
      max_players: { action: "set", value: "8" },
      seed: { action: "unset" },
    };
    const built = toSaveRequest({ ...base, models, edits });
    expect(built.request.changes).toEqual([{ id: "motd", action: "set", value: "Bonjour" }]);
    expect(built.request).toMatchObject({
      baseRev: 3,
      fileStamp: "abc",
      origin: "form",
      effect: "restart",
      importText: null,
    });
  });

  it("dit « remise » quand tout est remis aux valeurs proposées, et « écart » quand seul un écart est tranché", () => {
    const edits: Edits = { max_players: { action: "reset" } };
    const changed: SettingModel[] = models.map((model) =>
      model.id === "max_players" ? { ...model, value: "20", state: "written" } : model,
    );
    expect(toSaveRequest({ ...base, models: changed, edits }).request.origin).toBe("reset");
    const drift = toSaveRequest({
      ...base,
      models,
      edits: {},
      drifts: [{ id: "motd" }],
      choices: {},
    });
    expect(drift.request.origin).toBe("drift");
    // « Garder la valeur du fichier » est le choix par défaut, réglage par réglage.
    expect(drift.request.driftChoices).toEqual([{ id: "motd", choice: "keep_file" }]);
    expect(drift.effect).toBe("none");
    const mine = toSaveRequest({
      ...base,
      models,
      edits: {},
      drifts: [{ id: "motd" }],
      choices: { motd: "keep_mine" },
    });
    expect(mine.effect).toBe("restart");
  });

  it("pose un secret sans jamais le comparer, et ne retire que celui qui est défini", () => {
    const edits: Edits = { password: { action: "set", value: "nouveau" } };
    expect(pendingEdits(models, edits).map((model) => model.id)).toEqual(["password"]);
    expect(pendingEdits(models, { password: { action: "unset" } })).toEqual([]);
    const set = modelsOf(
      terraria().settings,
      terrariaView({
        settings: terrariaView().settings.map((value) =>
          value.id === "password" ? { ...value, state: "secret_set" } : value,
        ),
      }),
    );
    expect(pendingEdits(set, { password: { action: "unset" } }).map((model) => model.id)).toEqual([
      "password",
    ]);
  });

  it("ne propose jamais de changer un réglage verrouillé, et compte les réglages inconnus comme un redémarrage", () => {
    const edits: Edits = {
      port: { action: "set", value: "9" },
      world_name: { action: "set", value: "Autre" },
    };
    expect(pendingEdits(models, edits)).toEqual([]);
    const others = toSaveRequest({
      ...base,
      models,
      edits: {},
      others: [{ key: "mod_x", remove: true, value: null }],
    });
    expect(others.effect).toBe("restart");
    expect(others.request.others).toEqual([{ key: "mod_x", remove: true, value: null }]);
    expect(heaviestEffect([])).toBe("none");
  });

  it("juge la forme de chaque valeur posée comme la fiche le dit", () => {
    const edits: Edits = {
      max_players: { action: "set", value: "999" },
      motd: { action: "set", value: "x".repeat(101) },
      language: { action: "set", value: "klingon" },
    };
    const found = editIssues(models, edits);
    expect(found.get("max_players")?.code).toBe("out_of_range");
    expect(found.get("motd")?.code).toBe("too_long");
    expect(found.get("language")?.code).toBe("bad_choice");
  });
});

describe("une liste d'identifiants (BR-FICHE-037)", () => {
  const list = rich().settings.find((setting) => setting.question.id === "mod_ids");
  if (!list) throw new Error("la fiche d'essai a une liste");

  it("dit le séparateur, le jeu de caractères et le maximum de la fiche, jamais les siens", () => {
    expect(listValue(list, "12\n34\n")).toBe("12,34");
    expect(listText(list, "12,34")).toBe("12\n34");
    expect(listIssue(list, "12\n34")).toBeNull();
    expect(listIssue(list, "12\nabc")).toEqual({ code: "bad_item", item: "abc" });
    expect(listIssue(list, "1\n2\n3\n4\n5\n6")).toEqual({ code: "too_many", max: 5 });
  });
});

describe("l'installation : tous les réglages sur leur valeur proposée (BR-FICHE-040)", () => {
  it("ne répète pas les essentiels, garde l'imposé verrouillé et le figé réglable", () => {
    const models = installModelsOf(terraria().settings, null);
    expect(models).toHaveLength(32);
    expect(byId(models, "world_name").lock).toBeNull();
    expect(byId(models, "port").lock).toBe("imposed");
    expect(byId(models, "max_players").value).toBe("8");
    expect(byId(models, "motd").state).toBe("unwritten");
  });

  it("ne montre que les réglages de la variante choisie", () => {
    const detail = rich();
    expect(installModelsOf(detail.settings, "vanilla").map((m) => m.id)).not.toContain("mod_ids");
    expect(installModelsOf(detail.settings, "modded").map((m) => m.id)).toContain("mod_ids");
  });
});

describe("l'import d'un texte collé (BR-FICHE-052)", () => {
  const models = terrariaModels();
  const analysis: ImportAnalysis = {
    lines: [
      { id: "motd", state: "take", reason: null, pasted: "Salut", secret: false },
      { id: "max_players", state: "same", reason: null, pasted: "8", secret: false },
      { id: "language", state: "rejected", reason: "choice", pasted: "en/US", secret: false },
      { id: "password", state: "take", reason: null, pasted: null, secret: true },
      { id: "port", state: "imposed", reason: null, pasted: "1", secret: false },
      { id: "world_name", state: "frozen", reason: null, pasted: "Autre", secret: false },
    ],
    unknown: 2,
    duplicates: 0,
    imposedUndone: ["port"],
  };

  it("reprend les lignes reconnues dans le texte (le mot de passe collé inclus), jamais les imposées ni les figées", () => {
    const rows = importRows(analysis, models);
    expect(rows.find((row) => row.line.id === "language")?.alternatives.length).toBe(9);
    const built = toImportRequest({
      rows,
      picks: {},
      text: "motd=Salut",
      baseRev: 2,
      fileStamp: "s",
    });
    expect(built?.request.origin).toBe("import");
    expect(built?.request.importText).toBe("motd=Salut");
    expect(built?.request.changes).toEqual([
      { id: "motd", action: "from_import", value: null },
      { id: "password", action: "from_import", value: null },
    ]);
    expect(built?.count).toBe(2);
  });

  it("applique la valeur de la liste choisie pour une ligne refusée, et garde l'ancienne sinon", () => {
    const rows = importRows(analysis, models);
    const built = toImportRequest({
      rows,
      picks: { language: "fr-FR" },
      text: "t",
      baseRev: 0,
      fileStamp: "s",
    });
    expect(built?.request.changes).toContainEqual({
      id: "language",
      action: "set",
      value: "fr-FR",
    });
    expect(
      toImportRequest({
        rows: importRows({ ...analysis, lines: [analysis.lines[1] as never] }, models),
        picks: {},
        text: "",
        baseRev: 0,
        fileStamp: "s",
      }),
    ).toBeNull();
  });

  it("nomme par leur libellé les réglages imposés que le texte défait", () => {
    expect(undoneLabels(analysis, models)).toEqual(["Port d'écoute dans le service"]);
  });
});

describe("les textes de l'onglet (vouvoiement, mots de la spec)", () => {
  const strings: [string, string][] = [];
  const walk = (value: unknown, path: string) => {
    if (typeof value === "string") strings.push([path, value]);
    else if (value && typeof value === "object") {
      for (const [key, inner] of Object.entries(value)) walk(inner, `${path}.${key}`);
    }
  };
  walk(fr.gameSettings, "gameSettings");

  it("n'a ni tiret cadratin, ni tutoiement, ni mot interne, ni nom de jeu", () => {
    const bad = [
      /—|–/,
      /(?<![\p{L}'])(tu|ton|ta|tes|toi|te|t)(?![\p{L}])/iu,
      /(?<![\p{L}])(choisis|attends|réessaie|corrige|renseigne|utilise|laisse|ouvre|mets|libère|colle|enregistre|redémarre|prévenez-toi)(?![\p{L}])/iu,
      /docker|conteneur/i,
      /terraria|minecraft|valheim|7 days|\bhttps?:\/\//i,
      /(?<![\p{L}])(isolé|isolée|sûr|sûre|sécurisé|protégé)(?![\p{L}])/iu,
    ];
    for (const pattern of bad) {
      expect(strings.filter(([, text]) => pattern.test(text)).map(([path]) => path)).toEqual([]);
    }
  });

  it("dit les quatre mentions d'effet de la spec, mot pour mot", () => {
    expect(fr.gameSettings.effect.restart).toBe("Prend effet au prochain démarrage");
    expect(fr.gameSettings.effect.recreate).toBe(
      "Prend effet quand Hearth recrée le service (bouton Redémarrer maintenant, ou prochain démarrage lancé depuis Hearth).",
    );
    expect(fr.gameSettings.effect.frozen).toBe(
      "Figé : ne peut plus changer après la création du monde. Il faudrait créer un autre monde.",
    );
    expect(fr.gameSettings.effect.unknown).toBe(
      "Hearth ne sait pas si ce réglage existe dans cette version du jeu.",
    );
  });
});
