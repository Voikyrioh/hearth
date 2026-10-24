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

  /** Lit un texte collé comme l'agent : un fichier de lignes `nom=valeur`, rien n'est écrit. */
  analyze(serverId: string, key: string, text: string): AnalyzeOutcome {
    this.analyzed.push(text);
    const held = this.held.get(this.id(serverId, key));
    const detail = held ? this.ficheOf(held) : undefined;
    if (!held || !detail) throw new Error("service sans réglages");
    const parsed = this.parseText(text);
    if (parsed === null)
      return { kind: "unreadable", reason: "no_separator", line: this.badLine(text) };
    const lines: ImportLine[] = [];
    const seen = new Set<string>();
    let duplicates = 0;
    let unknown = 0;
    const imposedUndone: string[] = [];
    for (const [name, value] of parsed.entries) {
      const setting = detail.settings.find((item) => item.key === name);
      if (!setting) {
        unknown += 1;
        continue;
      }
      const question = setting.question;
      if (seen.has(question.id)) {
        duplicates += 1;
      }
      seen.add(question.id);
      const secret = question.secret;
      const base = { id: question.id, secret, pasted: secret ? null : value };
      if (setting.imposedWhy !== null) {
        imposedUndone.push(question.id);
        lines.push({ ...base, state: "imposed", reason: null });
      } else if (question.frozenAfterCreation) {
        lines.push({ ...base, state: "frozen", reason: null });
      } else {
        const refusal = judgeValue(question, value, setting.list);
        if (refusal) lines.push({ ...base, state: "rejected", reason: refusal.reason });
        else if (!secret && held.wanted.get(question.id) === value) {
          lines.push({ ...base, state: "same", reason: null });
        } else lines.push({ ...base, state: "take", reason: null });
      }
    }
    return {
      kind: "read",
      analysis: { lines, unknown, duplicates, imposedUndone },
    };
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
   * Juge un enregistrement dans l'ordre de l'agent, tous les changements ou aucun, puis l'applique en retouchant le
   * fichier sans le réécrire. Le droit, la confirmation et le moteur ont déjà été jugés par l'appelant.
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
    if (request.fileStamp !== stamp) return refused({ kind: "file_changed", ids: [] });
    // Les écarts attendent un choix, réglage par réglage.
    const current = this.read(serverId, key);
    const drifts = current.kind === "found" ? current.view.drifts : [];
    const missing = drifts
      .filter((drift) => !request.driftChoices.some((choice) => choice.id === drift.id))
      .map((drift) => drift.id);
    if (missing.length > 0) return refused({ kind: "file_changed", ids: missing });
    const textRead = request.importText === null ? null : this.parseText(request.importText);
    if (request.importText !== null && textRead === null) {
      return refused({
        kind: "file_unreadable",
        reason: "no_separator",
        line: this.badLine(request.importText),
      });
    }

    // « Garder la valeur du fichier » est refusé quand le réglage n'accepte pas cette valeur (jugé avec le reste).
    for (const choice of request.driftChoices) {
      const drift = drifts.find((item) => item.id === choice.id);
      if (choice.choice === "keep_file" && drift && !drift.fileValid) {
        return refused({
          kind: "setting_refused",
          id: choice.id,
          reason: "check",
          character: null,
        });
      }
    }
    // Tous les changements sont jugés d'abord.
    const next = new Map(held.wanted);
    const nextSecrets = new Set(held.secrets);
    const touched: FicheSetting[] = [];
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
      const fail = (reason: string, character: string | null = null) =>
        refused({ kind: "setting_refused", id: question.id, reason, character });
      if (setting.imposedWhy !== null) return fail("imposed");
      if (question.frozenAfterCreation) return fail("frozen");
      let value: string | null = null;
      if (change.action === "set") value = change.value ?? "";
      if (change.action === "from_import") {
        const pasted = textRead?.entries.filter(([name]) => name === setting.key).at(-1);
        if (!pasted) return fail("unknown_id");
        value = pasted[1];
      }
      if (change.action === "reset" && question.secret) return fail("secret");
      if (value !== null) {
        const refusal = judgeValue(question, value, setting.list);
        if (refusal) return fail(refusal.reason, refusal.character);
      }
      touched.push(setting);
      if (question.secret) {
        if (value !== null) {
          if (value === "") nextSecrets.delete(question.id);
          else nextSecrets.add(question.id);
        } else nextSecrets.delete(question.id);
      } else if (value !== null) next.set(question.id, value);
      else if (change.action === "reset" && question.proposed !== null) {
        next.set(question.id, question.proposed);
      } else next.delete(question.id);
    }
    // Un réglage reçu par argument demande de recréer le service (HRT-87 pas livré).
    const recreate = touched.filter((setting) => this.effectOf(setting) === "recreate");
    if (recreate.length > 0) {
      return refused({
        kind: "recreate_not_available",
        ids: recreate.map((setting) => setting.question.id),
      });
    }
    // L'effet signé est recalculé : le fichier est retouché (redémarrage) dès qu'un réglage, un réglage inconnu ou un
    // écart rendu à Hearth change quelque chose ; garder la valeur du fichier ne touche à rien.
    const keepMine = request.driftChoices.some((choice) => choice.choice === "keep_mine");
    const effect: SettingEffect =
      touched.length + request.others.length > 0 || keepMine ? "restart" : "none";
    if (request.effect !== effect) {
      return refused({ kind: "conflict", by: null, effectMismatch: true });
    }

    // Les choix d'écart : garder le fichier reprend sa valeur, remettre la mienne la réécrit.
    const adopted: string[] = [];
    for (const choice of request.driftChoices) {
      const drift = drifts.find((item) => item.id === choice.id);
      if (!drift || choice.choice !== "keep_file") continue;
      if (drift.file === null) next.delete(choice.id);
      else next.set(choice.id, drift.file);
      adopted.push(choice.id);
    }

    // Le fichier est retouché sur les seules valeurs demandées : commentaires et lignes inconnues gardent leurs octets.
    const lines = [...held.lines];
    const write = (setting: FicheSetting, value: string | null) => {
      if (this.catalogChannel(setting) !== "config") return;
      const at = lines.findIndex((line) => parseLine(line)?.key === setting.key);
      if (value === null) {
        if (at >= 0) lines.splice(at, 1);
      } else if (at >= 0) lines[at] = `${setting.key}=${value}`;
      else lines.push(`${setting.key}=${value}`);
    };
    for (const setting of touched) {
      const question = setting.question;
      if (question.secret) {
        const change = request.changes.find((item) => item.id === question.id);
        const value =
          change?.action === "from_import"
            ? (textRead?.entries.filter(([name]) => name === setting.key).at(-1)?.[1] ?? null)
            : (change?.value ?? null);
        write(setting, nextSecrets.has(question.id) ? value : null);
      } else {
        write(setting, next.get(question.id) ?? null);
      }
    }
    for (const choice of request.driftChoices) {
      const setting = detail.settings.find((item) => item.question.id === choice.id);
      if (setting && choice.choice === "keep_mine") write(setting, next.get(choice.id) ?? null);
    }
    for (const other of request.others) {
      const at = lines.findIndex((line) => parseLine(line)?.key === other.key);
      if (at < 0) continue;
      if (other.remove) lines.splice(at, 1);
      else lines[at] = `${other.key}=${other.value ?? ""}`;
    }
    const written = lines.join("\n") !== held.lines.join("\n");
    held.wanted = next;
    held.secrets = nextSecrets;
    if (written) held.lines = lines;
    held.fileMissing = false;
    held.writtenStamp = stampOf(held.lines.join("\n"));
    held.rev += 1;
    held.lastBy = by;
    return {
      kind: "saved",
      rev: held.rev,
      effect,
      fileWritten: written,
      changed: touched.length + request.others.length,
      adopted,
    };
  }
}
