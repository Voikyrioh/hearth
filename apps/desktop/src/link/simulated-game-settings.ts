import type { FicheDetail, FicheSetting } from "./catalog";
import type {
  AnalyzeOutcome,
  Drift,
  GameSettingsRead,
  GameSettingsView,
  ImportLine,
  OtherSetting,
  SaveOutcome,
  SaveRefusal,
  SaveSettingsRequest,
  SettingEffect,
  SettingValue,
} from "./gameSettings";
import type { SimulatedCatalog } from "./simulated-catalog";
import { judgeValue } from "./simulated-judge";
import { TERRARIA_CHANNEL } from "./simulated-terraria-settings";

/**
 * Les réglages d'un serveur de jeu du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un agent d'essai
 * qui juge une lecture, une analyse et un enregistrement COMME LE VRAI (`crates/hearth-agent/src/application/
 * game_settings.rs`, ADR-0051) : même ordre de refus (révision, empreinte du fichier, choix d'écart, fichier lisible,
 * réglage connu, imposé, figé, valeur, recréation, effet signé), mêmes codes, `file_stamp` OBLIGATOIRE, un fichier qui
 * garde ses commentaires et ses lignes inconnues, jamais réécrit quand aucun octet ne change, un secret qui ne revient
 * jamais. Un test qui passe ici passerait contre l'agent ; le simulateur n'est pas plus indulgent que lui.
 *
 * Le fichier du jeu est un fichier de lignes `nom=valeur` (le format de Terraria) tenu en texte, de quoi simuler le jeu
 * qui le réécrit (`rewriteFile`) ou une main qui l'édite. Jamais livré (derrière `import.meta.env.DEV`).
 */

/** Ce que l'agent d'essai garde d'un service de jeu. */
interface Held {
  ficheId: string;
  /** L'empreinte de la fiche de l'installation. */
  digest: string;
  version: number;
  rev: number;
  /** Les valeurs voulues (jamais un secret), par identifiant. */
  wanted: Map<string, string>;
  /** Les secrets définis (jamais leur valeur au-delà de ce test). */
  secrets: Set<string>;
  /** Le fichier du jeu, ligne par ligne. */
  lines: string[];
  /** L'empreinte du fichier tel que Hearth l'a écrit en dernier. */
  writtenStamp: string;
  fileMissing: boolean;
  brokenImposed: string[];
  lastBy: string | null;
}

/** Une sauvegarde reçue, telle que le serveur la garde pour un test : jamais lue par l'interface. */
export interface ReceivedSave {
  serverId: string;
  key: string;
  request: SaveSettingsRequest;
}

const SECRET_NAME = /PASSWORD|PASS|SECRET|TOKEN|KEY/i;

/** Une empreinte déterministe du texte (le vrai est un SHA-256 hexadécimal). */
export function stampOf(text: string): string {
  let a = 0x811c9dc5;
  let b = 0x01000193;
  for (let i = 0; i < text.length; i += 1) {
    a = Math.imul(a ^ text.charCodeAt(i), 0x01000193) >>> 0;
    b = (Math.imul(b, 31) + text.charCodeAt(i)) >>> 0;
  }
  return `${a.toString(16).padStart(8, "0")}${b.toString(16).padStart(8, "0")}`.repeat(4);
}

function parseLine(line: string): { key: string; value: string } | null {
  const trimmed = line.trim();
  if (trimmed === "" || trimmed.startsWith("#") || trimmed.startsWith(";")) return null;
  const at = line.indexOf("=");
  if (at <= 0) return null;
  return { key: line.slice(0, at).trim(), value: line.slice(at + 1) };
}

export class SimulatedGameSettings {
  /** Les enregistrements ACCEPTÉS ou jugés, dans l'ordre (de quoi vérifier ce qui part). */
  readonly received: ReceivedSave[] = [];
  /** Les textes collés reçus pour lecture. */
  readonly analyzed: string[] = [];
  /** `true` : la lecture des réglages échoue (agent qui ne répond pas). */
  failRead = false;
  private readonly held = new Map<string, Held>();

  constructor(private readonly catalog: SimulatedCatalog) {}

  private id(serverId: string, key: string): string {
    return `${serverId}/${key}`;
  }

  private ficheOf(entry: Held): FicheDetail | undefined {
    return this.catalog.get(entry.ficheId);
  }

  // ── Ce que l'agent garde (pilotable) ─────────────────────────────────────────────────────────

  /**
   * Un service de jeu né d'une fiche : les valeurs voulues sont les réponses de l'installation, sinon la valeur
   * proposée par la fiche ; le fichier est écrit d'après elles (ce que l'installation fait), les secrets sont définis.
   */
  register(
    serverId: string,
    key: string,
    detail: FicheDetail,
    answers: readonly { id: string; value: string }[] = [],
  ): void {
    const wanted = new Map<string, string>();
    const secrets = new Set<string>();
    const lines: string[] = ["# Fichier du jeu, écrit par Hearth à l'installation."];
    const given = new Map(answers.map((answer) => [answer.id, answer.value]));
    for (const setting of detail.settings) {
      const question = setting.question;
      if (setting.imposedWhy !== null) continue;
      const value = given.get(question.id) ?? (question.secret ? null : question.proposed);
      if (value === null || value === undefined) continue;
      if (question.secret) {
        if (value !== "") secrets.add(question.id);
      } else {
        wanted.set(question.id, value);
      }
      if (this.catalogChannel(setting) === "config" && !(question.secret && value === "")) {
        lines.push(`${setting.key}=${value}`);
      }
    }
    const held: Held = {
      ficheId: detail.card.id,
      digest: detail.digest,
      version: detail.card.version,
      rev: 0,
      wanted,
      secrets,
      lines,
      writtenStamp: "",
      fileMissing: false,
      brokenImposed: [],
      lastBy: null,
    };
    held.writtenStamp = stampOf(held.lines.join("\n"));
    this.held.set(this.id(serverId, key), held);
  }

  /** Ce service a-t-il des réglages de jeu ? */
  has(serverId: string, key: string): boolean {
    return this.held.has(this.id(serverId, key));
  }

  forget(serverId: string, key: string): void {
    this.held.delete(this.id(serverId, key));
  }

  /** Le texte du fichier tel que le jeu le lit (de quoi vérifier les octets gardés). */
  fileText(serverId: string, key: string): string {
    return (this.held.get(this.id(serverId, key))?.lines ?? []).join("\n");
  }

  /** Le jeu (ou une main) réécrit le fichier : l'empreinte change sans que Hearth l'ait écrit. */
  rewriteFile(serverId: string, key: string, text: string): void {
    const held = this.held.get(this.id(serverId, key));
    if (!held) return;
    held.lines = text.split("\n");
    held.fileMissing = false;
  }

  /** Le jeu a effacé son fichier. */
  deleteFile(serverId: string, key: string): void {
    const held = this.held.get(this.id(serverId, key));
    if (held) held.fileMissing = true;
  }

  /** Le fichier a défait ces réglages imposés (signalés, jamais réécrits). */
  breakImposed(serverId: string, key: string, ids: string[]): void {
    const held = this.held.get(this.id(serverId, key));
    if (held) held.brokenImposed = [...ids];
  }

  /** Un autre compte enregistre pendant que celui-ci édite : la révision avance. */
  bumpRev(serverId: string, key: string, by = "marie"): void {
    const held = this.held.get(this.id(serverId, key));
    if (!held) return;
    held.rev += 1;
    held.lastBy = by;
  }

  private catalogChannel(setting: FicheSetting): "config" | "args" {
    // Le canal n'est pas dans la fiche servie : l'agent le sait. Terraria le dit, les autres fiches passent par le fichier.
    return TERRARIA_CHANNEL[setting.question.id] ?? "config";
  }

  // ── Les lectures ─────────────────────────────────────────────────────────────────────────────

  private fileMap(held: Held): Map<string, string> {
    const map = new Map<string, string>();
    for (const line of held.lines) {
      const parsed = parseLine(line);
      if (parsed) map.set(parsed.key, parsed.value);
    }
    return map;
  }

  private knownKeys(detail: FicheDetail): Set<string> {
    return new Set(detail.settings.map((setting) => setting.key));
  }

  private effectOf(setting: FicheSetting): SettingEffect {
    return this.catalogChannel(setting) === "args" ? "recreate" : "restart";
  }

  read(serverId: string, key: string): GameSettingsRead {
    if (this.failRead) throw new Error("réglages illisibles");
    const held = this.held.get(this.id(serverId, key));
    const detail = held ? this.ficheOf(held) : undefined;
    if (!held || !detail) return { kind: "absent" };
    const file = this.fileMap(held);
    const settings: SettingValue[] = detail.settings.map((setting) => {
      const question = setting.question;
      const wanted = held.wanted.get(question.id);
      const state = question.secret
        ? held.secrets.has(question.id)
          ? ("secret_set" as const)
          : ("unwritten" as const)
        : wanted !== undefined
          ? ("written" as const)
          : ("unwritten" as const);
      return {
        id: question.id,
        state,
        value: question.secret ? null : (wanted ?? null),
        frozen: question.frozenAfterCreation,
        imposed: setting.imposedWhy !== null,
        gone: false,
        effect: this.effectOf(setting),
      };
    });
    const drifts: Drift[] = [];
    if (!held.fileMissing) {
      for (const setting of detail.settings) {
        const question = setting.question;
        if (question.secret || setting.imposedWhy !== null) continue;
        if (this.catalogChannel(setting) !== "config") continue;
        const wanted = held.wanted.get(question.id) ?? null;
        const found = file.get(setting.key) ?? null;
        if (wanted !== found) {
          drifts.push({
            id: question.id,
            hearth: wanted,
            file: found,
            fileValid: found === null || judgeValue(question, found, setting.list) === null,
          });
        }
      }
    }
    const known = this.knownKeys(detail);
    const others: OtherSetting[] = [...file.entries()]
      .filter(([name]) => !known.has(name))
      .map(([name, value]) =>
        SECRET_NAME.test(name)
          ? { key: name, value: null, masked: true }
          : { key: name, value, masked: false },
      );
    const text = held.lines.join("\n");
    const view: GameSettingsView = {
      rev: held.rev,
      fiche: { id: held.ficheId, version: held.version, digest: held.digest },
      fileState: held.fileMissing ? "missing" : "ok",
      fileStamp: stampOf(held.fileMissing ? "" : text),
      fileChanged: !held.fileMissing && stampOf(text) !== held.writtenStamp,
      rawEdit: true,
      settings,
      drifts,
      others,
      imposedBroken: [...held.brokenImposed],
    };
    return { kind: "found", view };
  }

  /**
   * Lit un texte collé comme l'agent (`analyze_import`) : un fichier de lignes `nom=valeur`, rien n'est écrit. UNE ligne
   * par réglage de la fiche, dans l'ordre de la fiche, la dernière valeur d'une clé répétée l'emporte ; un réglage imposé a
   * toujours sa ligne ; `unknown` compte des clés DISTINCTES que la fiche ne connaît pas, `duplicates` des clés répétées.
   */
  analyze(serverId: string, key: string, text: string): AnalyzeOutcome {
    this.analyzed.push(text);
    const held = this.held.get(this.id(serverId, key));
    const detail = held ? this.ficheOf(held) : undefined;
    if (!held || !detail) throw new Error("service sans réglages");
    const parsed = this.parseText(text);
    if (parsed === null)
      return { kind: "unreadable", reason: "no_separator", line: this.badLine(text) };
    const last = new Map<string, string>();
    const count = new Map<string, number>();
    for (const [name, value] of parsed.entries) {
      last.set(name, value);
      count.set(name, (count.get(name) ?? 0) + 1);
    }
    const file = this.fileMap(held);
    const lines: ImportLine[] = [];
    const imposedUndone: string[] = [];
    for (const setting of detail.settings) {
      if (this.catalogChannel(setting) !== "config") continue;
      const question = setting.question;
      const value = last.get(setting.key);
      const secret = question.secret;
      const base = { id: question.id, secret, pasted: secret ? null : (value ?? null) };
      if (setting.imposedWhy !== null) {
        if (value !== file.get(setting.key)) imposedUndone.push(question.id);
        lines.push({ ...base, state: "imposed", reason: null });
        continue;
      }
      if (value === undefined) continue;
      if (question.frozenAfterCreation) {
        lines.push({ ...base, state: "frozen", reason: null });
        continue;
      }
      const refusal = judgeValue(question, value, setting.list);
      if (refusal) lines.push({ ...base, state: "rejected", reason: refusal.reason });
      else if (!secret && held.wanted.get(question.id) === value) {
        lines.push({ ...base, state: "same", reason: null });
      } else lines.push({ ...base, state: "take", reason: null });
    }
    const known = this.knownKeys(detail);
    const unknown = [...count.keys()].filter((name) => !known.has(name)).length;
    const duplicates = [...count.values()].filter((n) => n > 1).length;
    return { kind: "read", analysis: { lines, unknown, duplicates, imposedUndone } };
  }

  private badLine(text: string): number | null {
    const rows = text.split(/\r?\n/);
    const at = rows.findIndex(
      (row) => row.trim() !== "" && !/^\s*[#;]/.test(row) && row.indexOf("=") <= 0,
    );
    return at < 0 ? null : at + 1;
  }

  private parseText(text: string): { entries: [string, string][] } | null {
    if (this.badLine(text) !== null) return null;
    const entries: [string, string][] = [];
    for (const row of text.split(/\r?\n/)) {
      const parsed = parseLine(row);
      if (parsed) entries.push([parsed.key, parsed.value]);
    }
    return { entries };
  }

  // ── L'enregistrement (`PUT /services/{key}/settings`) ────────────────────────────────────────

  /**
   * Juge un enregistrement DANS L'ORDRE de `GameSettings::save` (agent), tous les changements ou aucun, puis l'applique en
   * retouchant le fichier sans le réécrire : révision ; chaque changement (inconnu, remise d'un secret, réglage reçu par
   * argument, forme de l'action) ; recréation ; puis, par réglage en écart, le choix (« Garder la valeur du fichier » est
   * IGNORÉ pour un réglage que la même demande change : sa valeur gagne) ; les écarts sans choix donnent
   * `SETTINGS_FILE_CHANGED` avant de juger les valeurs ; sinon imposé, figé, valeur ; ensuite l'empreinte du fichier,
   * l'effet signé (contrôlé AVANT le retour « rien ne change ») et enfin l'écriture. Un changement qui ne change rien ne
   * compte pas. Le droit, la confirmation et le moteur ont déjà été jugés par l'appelant.
   */
  save(serverId: string, key: string, request: SaveSettingsRequest, by: string): SaveOutcome {
    this.received.push({ serverId, key, request: structuredClone(request) });
    const refused = (refusal: SaveRefusal): SaveOutcome => ({ kind: "refused", refusal });
    const held = this.held.get(this.id(serverId, key));
    const detail = held ? this.ficheOf(held) : undefined;
    if (!held || !detail) return refused({ kind: "service_not_found" });
    if (request.fileStamp === "") return refused({ kind: "invalid" });
    if (request.baseRev !== held.rev) {
      return refused({ kind: "conflict", by: held.lastBy, effectMismatch: false });
    }
    const text = held.lines.join("\n");
    const stamp = stampOf(held.fileMissing ? "" : text);
    const parsedText = request.importText === null ? null : this.parseText(request.importText);

    // 1. Chaque changement vers son réglage, dans l'ordre de la demande.
    const asked: { setting: FicheSetting; wanted: Wanted }[] = [];
    const recreate: string[] = [];
    for (const change of request.changes) {
      const setting = detail.settings.find((item) => item.question.id === change.id);
      if (!setting) {
        return refused({
          kind: "setting_refused",
          id: null,
          reason: "unknown_id",
          character: null,
        });
      }
      const question = setting.question;
      if (request.origin === "reset" && question.secret) {
        return refused({
          kind: "setting_refused",
          id: question.id,
          reason: "secret",
          character: null,
        });
      }
      if (this.effectOf(setting) === "recreate") {
        recreate.push(question.id);
        continue;
      }
      let wanted: Wanted;
      if (change.action === "set") {
        if (change.value === null) return refused({ kind: "invalid" });
        wanted = { action: "set", value: change.value };
      } else if (change.action === "unset") wanted = { action: "unset" };
      else if (change.action === "reset") wanted = { action: "reset" };
      else {
        if (request.importText === null) return refused({ kind: "invalid" });
        if (parsedText === null) {
          return refused({
            kind: "file_unreadable",
            reason: "no_separator",
            line: this.badLine(request.importText),
          });
        }
        const pasted = parsedText.entries.filter(([name]) => name === setting.key).at(-1);
        if (!pasted) return refused({ kind: "invalid" });
        wanted = { action: "set", value: pasted[1] };
      }
      asked.push({ setting, wanted });
    }
    if (recreate.length > 0) return refused({ kind: "recreate_not_available", ids: recreate });
    for (const other of request.others) {
      if (!other.remove && other.value === null) return refused({ kind: "invalid" });
    }

    // 2. Les écarts : un choix par réglage que la demande ne change pas.
    const current = this.read(serverId, key);
    const drifts = (current.kind === "found" ? current.view.drifts : []).filter(
      (drift) => !asked.some((item) => item.setting.question.id === drift.id),
    );
    const next = new Map(held.wanted);
    const adopted: string[] = [];
    const mine: [FicheSetting, string | null][] = [];
    const unresolved: string[] = [];
    for (const drift of drifts) {
      const setting = detail.settings.find((item) => item.question.id === drift.id);
      if (!setting) continue;
      const choice = request.driftChoices.find((item) => item.id === drift.id);
      if (choice?.choice === "keep_file") {
        const refusal =
          drift.file === null ? null : judgeValue(setting.question, drift.file, setting.list);
        if (refusal) {
          return refused({
            kind: "setting_refused",
            id: drift.id,
            reason: refusal.reason,
            character: refusal.character,
          });
        }
        if (drift.file === null) next.delete(drift.id);
        else next.set(drift.id, drift.file);
        adopted.push(drift.id);
      } else if (choice?.choice === "keep_mine") mine.push([setting, drift.hearth]);
      else unresolved.push(drift.id);
    }

    // 3. Les valeurs, seulement si aucun écart n'attend de choix (l'agent rend `SETTINGS_FILE_CHANGED` avant de juger).
    const nextSecrets = new Set(held.secrets);
    const changed: FicheSetting[] = [];
    const edits: [FicheSetting, string | null][] = [];
    if (unresolved.length === 0) {
      for (const { setting, wanted } of asked) {
        const question = setting.question;
        const refuse = (reason: string, character: string | null = null) =>
          refused({ kind: "setting_refused", id: question.id, reason, character });
        if (setting.imposedWhy !== null) return refuse("imposed");
        if (question.frozenAfterCreation) return refuse("frozen");
        let value: string | null;
        if (wanted.action === "set") {
          const refusal = judgeValue(question, wanted.value, setting.list);
          if (refusal) return refuse(refusal.reason, refusal.character);
          value = wanted.value;
        } else if (wanted.action === "reset") {
          value = question.secret ? null : question.proposed;
        } else value = null;
        const before = question.secret
          ? held.secrets.has(question.id)
            ? "set"
            : null
          : (next.get(question.id) ?? null);
        const differs = question.secret ? value !== null || before !== null : before !== value;
        if (question.secret) {
          if (value !== null) nextSecrets.add(question.id);
          else nextSecrets.delete(question.id);
        } else if (value !== null) next.set(question.id, value);
        else next.delete(question.id);
        if (!differs) continue;
        changed.push(setting);
        edits.push([setting, value]);
      }
    }

    // 4. Le fichier a bougé ou un écart attend un choix : rien n'est écrit.
    if (unresolved.length > 0 || request.fileStamp !== stamp) {
      return refused({ kind: "file_changed", ids: unresolved });
    }

    // 5. Ce qu'il faut réécrire dans le fichier : seulement ce qui diffère de ce qu'il porte déjà.
    const observed = this.fileMap(held);
    const all: [string, string | null][] = [];
    for (const [setting, value] of mine) all.push([setting.key, value]);
    for (const [setting, value] of edits) {
      if (this.catalogChannel(setting) === "config") all.push([setting.key, value]);
    }
    const known = this.knownKeys(detail);
    for (const other of request.others) {
      if (known.has(other.key)) return refused({ kind: "invalid" });
      all.push([other.key, other.remove ? null : other.value]);
    }
    const toWrite = all.filter(([name, value]) => (observed.get(name) ?? null) !== value);

    // 6. L'effet signé : le plus lourd des changements, ou un redémarrage quand seul le fichier est retouché.
    const effect: SettingEffect = changed.length > 0 || toWrite.length > 0 ? "restart" : "none";
    if (request.effect !== effect) {
      return refused({ kind: "conflict", by: null, effectMismatch: true });
    }
    if (changed.length === 0 && adopted.length === 0 && toWrite.length === 0) {
      return {
        kind: "saved",
        rev: held.rev,
        effect: "none",
        fileWritten: false,
        changed: 0,
        adopted: [],
      };
    }

    // 7. Écrire : le fichier est retouché sur les seules lignes demandées, commentaires et lignes inconnues intacts.
    const lines = [...held.lines];
    for (const [name, value] of toWrite) {
      const at = lines.findIndex((line) => parseLine(line)?.key === name);
      if (value === null) {
        if (at >= 0) lines.splice(at, 1);
      } else if (at >= 0) lines[at] = `${name}=${value}`;
      else lines.push(`${name}=${value}`);
    }
    held.wanted = next;
    held.secrets = nextSecrets;
    if (toWrite.length > 0) held.lines = lines;
    held.fileMissing = false;
    held.writtenStamp = stampOf(held.lines.join("\n"));
    held.rev += 1;
    held.lastBy = by;
    return {
      kind: "saved",
      rev: held.rev,
      effect,
      fileWritten: toWrite.length > 0,
      changed: changed.length + request.others.length,
      adopted,
    };
  }
}

/** Ce qu'un changement demande, une fois décodé (`set` porte sa valeur, y compris reprise d'un texte collé). */
type Wanted = { action: "set"; value: string } | { action: "unset" } | { action: "reset" };
