import type {
  LogClosedCause,
  LogEnd,
  LogGapReason,
  LogLine,
  LogMarkerKind,
  LogMessage,
  LogReading,
  LogState,
} from "./logs";
import { LinkCommandError } from "./types";

/**
 * Les journaux d'un service du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent que
 * l'appelant PILOTE. Il joue tous les états de l'onglet Journaux : première page, direct, pause (côté interface),
 * service arrêté, redémarrage, trou, coupure et reprise, refus (droit, plafond du compte, frein avec son délai,
 * lecture impossible, moteur non prêt, service disparu), et un DÉBIT ÉLEVÉ (`flood`, par lots de 100 ms comme
 * l'agent). Comme l'agent : au plus trois lectures par compte (`BR-SLOG-026`), un frein aux ouvertures
 * (`BR-SLOG-027`, douze en rafale, une rendue toutes les cinq secondes), une reprise qui ne rejoue rien. Les
 * messages sont ceux du type `LogMessage` (le même que la coquille produit) : `tsc` refuse tout champ en trop ou
 * manquant. Jamais livré (derrière `import.meta.env.DEV`).
 */

/** Plafond d'un compte (`hearth_proto::api::service_logs::MAX_READS_PER_ACCOUNT`). */
export const SIM_ACCOUNT_LIMIT = 3;
/** Rafale d'ouvertures et rythme de remise (`OPEN_BURST`, `OPEN_REFILL_S`). */
export const SIM_OPEN_BURST = 12;
export const SIM_OPEN_REFILL_S = 5;
/** Lectures que la vraie coquille tient au plus pour un serveur (`MAX_SESSIONS_PER_SERVER`) ; à poser à `shellLimit`. */
export const SIM_SHELL_LIMIT = 3;
/**
 * Où la coquille simulée garde le nombre de ses sessions : `window.name`, qui survit au rechargement de la page comme
 * la vraie coquille survit à la page, sans toucher au stockage (aucune ligne ni rien d'autre n'y est écrit).
 */
const SHELL_PREFIX = "hearth-sim-logs-shell=";
/** Lots du direct : toutes les 100 ms (`BATCH_WINDOW_MS`), 500 lignes au plus (`BATCH_LINES`). */
const BATCH_MS = 100;
const BATCH_LINES = 500;

export interface SimLineInput {
  text: string;
  source?: "out" | "err" | "unknown";
  /** Millisecondes depuis l'époque ; défaut : l'instant de l'écriture, croissant. */
  t?: number;
}

/** Des lignes hostiles : tout ce qu'un programme peut écrire (séquences de terminal, bidirectionnel, faux horodatages). */
export const HOSTILE_LINES: readonly string[] = [
  "\u001b[31mrouge\u001b[0m puis \u001b[2Jefface l'ecran\u001b[H",
  "\u001b]0;titre de fenetre\u0007apres le titre",
  "lien \u001b]8;;https://exemple.invalid\u0007clique\u001b]8;;\u0007 fabrique",
  "bidi ‮errof‬ ⁦isole⁩ fin",
  "nul\u0000 et bip\u0007 et retour\u0008 arriere",
  "2026-01-01T00:00:00Z [INFO] faux horodatage au milieu du texte",
  "<script>alert('v-html')</script> <img src=x onerror=alert(1)>",
  "https://exemple.invalid/lien-non-cliquable et www.exemple.invalid",
  `${"a".repeat(64 * 1024)}`,
];

interface Reader {
  serverId: string;
  key: string;
  listener: (message: LogMessage) => void;
  closed: boolean;
  /** Le lien est coupé : rien n'est envoyé à cette lecture jusqu'à la reprise. */
  suspended: boolean;
  acked: number;
  /** Combien de lignes ce service avait écrites quand cette lecture a reçu la dernière. */
  seen: number;
}

interface Feed {
  /** Ce qui a été écrit jusque-là (l'anneau de l'agent : 5 000 lignes). */
  lines: LogLine[];
  /** Lignes écrites depuis toujours (l'anneau en garde 5 000). */
  total: number;
  status: Extract<LogMessage, { kind: "status" }>;
}

const key = (serverId: string, serviceKey: string) => `${serverId}\u0000${serviceKey}`;

const IDLE_STATUS: Feed["status"] = {
  kind: "status",
  state: "live",
  container: "0123456789ab",
  mixed: false,
  exitCode: null,
  endedAt: null,
  notKept: false,
};

export class SimulatedLogs {
  private readonly feeds = new Map<string, Feed>();
  private readonly readers = new Set<Reader>();
  private readonly floods = new Map<string, ReturnType<typeof setInterval>>();
  private clock = Date.now();
  private opens: number[] = [];

  /** Les lectures ouvertes puis fermées, dans l'ordre (de quoi vérifier « une seule lecture par onglet »). */
  readonly opened: { serverId: string; key: string }[] = [];
  closedCount = 0;
  /** Le prochain `open` est refusé avec ce motif (une fois). */
  private refusal: Extract<LogEnd, { kind: "refused" }> | null = null;
  /** Délai de la réponse d'ouverture (ms) : de quoi voir l'état « chargement ». */
  openDelayMs = 0;
  /** Ouvrir échoue à la manière d'un serveur qui ne répond pas. */
  failOpen = false;
  /** Plafond du compte, réglable (`SIM_ACCOUNT_LIMIT` par défaut). */
  accountLimit = SIM_ACCOUNT_LIMIT;
  /** Frein aux ouvertures en rafale, actif par défaut ; faux : jamais de frein. */
  brake = true;
  /** Lots envoyés à ce jour, toutes lectures confondues (de quoi mesurer le regroupement). */
  batches = 0;
  /**
   * Plafond de la coquille simulée. Sans limite par défaut, pour que le plafond de l'AGENT (`accountLimit`, les
   * lectures des autres postes du compte) reste éprouvable ; un test de la coquille le pose à `SIM_SHELL_LIMIT`.
   */
  shellLimit = Number.POSITIVE_INFINITY;
  /**
   * Sessions que la page PRÉCÉDENTE avait laissées ouvertes dans la coquille simulée et que l'annonce de cette page a
   * fermées (rechargement). La vraie coquille fait de même (`LogSessions::announce_page`).
   */
  readonly orphansClosedAtStart: number;

  constructor() {
    this.orphansClosedAtStart = this.shellCount();
    this.setShellCount(0);
  }

  /** Sessions que la coquille simulée tient (elle survit au rechargement de la page). */
  shellCount(): number {
    return window.name.startsWith(SHELL_PREFIX)
      ? Number(window.name.slice(SHELL_PREFIX.length)) || 0
      : 0;
  }

  private setShellCount(n: number): void {
    window.name = `${SHELL_PREFIX}${Math.max(0, n)}`;
  }

  /** Les commandes de fermeture sont perdues (la page meurt pendant l'appel) : ce qu'elle laisse est un orphelin. */
  loseCloses = false;

  /** La fenêtre est cachée : la coquille ferme les lectures et le dit à la page (comme `close_window`). */
  hideWindow(cause: LogClosedCause = "window_hidden"): void {
    for (const reader of [...this.readers]) {
      if (reader.closed) continue;
      reader.closed = true;
      this.readers.delete(reader);
      this.setShellCount(this.shellCount() - 1);
      reader.listener({ kind: "ended", end: { kind: "closed", cause } });
    }
  }

  /** Lectures ouvertes en ce moment. */
  openCount(): number {
    return [...this.readers].filter((reader) => !reader.closed).length;
  }

  /** Les lectures ouvertes de ce service. */
  openFor(serverId: string, serviceKey: string): number {
    return [...this.readers].filter(
      (reader) => !reader.closed && reader.serverId === serverId && reader.key === serviceKey,
    ).length;
  }

  private feed(serverId: string, serviceKey: string): Feed {
    const id = key(serverId, serviceKey);
    let found = this.feeds.get(id);
    if (!found) {
      found = { lines: [], total: 0, status: { ...IDLE_STATUS } };
      this.feeds.set(id, found);
    }
    return found;
  }

  private nextTime(): number {
    this.clock = Math.max(this.clock + 1, Date.now());
    return this.clock;
  }

  private line(input: SimLineInput): LogLine {
    return {
      t: input.t ?? this.nextTime(),
      source: input.source ?? "out",
      text: input.text,
      cut: null,
    };
  }

  /** Écrit l'historique de ce service (ce que le moteur a gardé), sans rien envoyer aux lectures ouvertes. */
  seed(serverId: string, serviceKey: string, lines: SimLineInput[]): void {
    const feed = this.feed(serverId, serviceKey);
    feed.lines = lines.map((input) => this.line(input)).slice(-5_000);
    feed.total = lines.length;
  }

  /** Le service écrit ces lignes : elles partent en UN lot à toutes les lectures de ce service. */
  push(serverId: string, serviceKey: string, lines: SimLineInput[]): void {
    const feed = this.feed(serverId, serviceKey);
    const made = lines.map((input) => this.line(input));
    this.keep(feed, made);
    for (const reader of this.matching(serverId, serviceKey)) {
      if (reader.suspended) continue;
      reader.seen = feed.total;
      reader.listener({ kind: "lines", seq: ++this.batches, lines: made });
    }
  }

  private keep(feed: Feed, made: LogLine[]): void {
    feed.lines.push(...made);
    feed.total += made.length;
    if (feed.lines.length > 5_000) feed.lines.splice(0, feed.lines.length - 5_000);
  }

  /** Un débit élevé : `perSecond` lignes par seconde, par lots de 100 ms (500 lignes au plus par lot). Rend l'arrêt. */
  flood(
    serverId: string,
    serviceKey: string,
    perSecond: number,
    options: { errorEvery?: number; text?: (n: number) => string } = {},
  ): () => void {
    const id = key(serverId, serviceKey);
    this.stopFlood(serverId, serviceKey);
    let n = 0;
    let owed = 0;
    const timer = setInterval(() => {
      owed += (perSecond * BATCH_MS) / 1000;
      const count = Math.min(Math.floor(owed), BATCH_LINES);
      owed -= count;
      if (count <= 0) return;
      const lines: SimLineInput[] = [];
      for (let i = 0; i < count; i += 1) {
        n += 1;
        lines.push({
          text: options.text?.(n) ?? `ligne ${n} du service, un texte assez long pour compter`,
          source: options.errorEvery && n % options.errorEvery === 0 ? "err" : "out",
        });
      }
      this.push(serverId, serviceKey, lines);
    }, BATCH_MS);
    this.floods.set(id, timer);
    return () => this.stopFlood(serverId, serviceKey);
  }

  stopFlood(serverId: string, serviceKey: string): void {
    const id = key(serverId, serviceKey);
    const timer = this.floods.get(id);
    if (timer !== undefined) clearInterval(timer);
    this.floods.delete(id);
  }

  /** Arrête tout débit et ferme toutes les lectures (fin d'un test). */
  reset(): void {
    for (const timer of this.floods.values()) clearInterval(timer);
    this.floods.clear();
    for (const reader of this.readers) reader.closed = true;
    this.readers.clear();
    this.setShellCount(0);
    this.shellLimit = Number.POSITIVE_INFINITY;
    this.feeds.clear();
    this.opens = [];
    this.refusal = null;
    this.opened.length = 0;
    this.closedCount = 0;
    this.batches = 0;
    this.brake = true;
    this.failOpen = false;
    this.openDelayMs = 0;
    this.accountLimit = SIM_ACCOUNT_LIMIT;
  }

  /** Le service change d'état de lecture (arrêté, en attente...) : un `status` à toutes ses lectures. */
  setStatus(
    serverId: string,
    serviceKey: string,
    patch: Partial<Omit<Extract<LogMessage, { kind: "status" }>, "kind">>,
  ): void {
    const feed = this.feed(serverId, serviceKey);
    feed.status = { ...feed.status, ...patch, kind: "status" };
    this.broadcast(serverId, serviceKey, feed.status);
  }

  /** Le service redémarre : un repère, puis son nouvel état « en marche ». */
  restart(serverId: string, serviceKey: string): void {
    this.broadcast(serverId, serviceKey, {
      kind: "marker",
      marker: "restarted",
      at: this.nextTime(),
    });
    this.setStatus(serverId, serviceKey, { state: "live", exitCode: null, endedAt: null });
  }

  /** Un repère d'un genre donné (`unknown` : un repère d'un agent plus récent). */
  mark(serverId: string, serviceKey: string, marker: LogMarkerKind): void {
    this.broadcast(serverId, serviceKey, { kind: "marker", marker, at: this.nextTime() });
  }

  /** Des lignes manquent ici. */
  gap(
    serverId: string,
    serviceKey: string,
    reason: LogGapReason,
    count: number | null = null,
  ): void {
    this.broadcast(serverId, serviceKey, { kind: "gap", count, reason });
  }

  /** Le socket tombe : `interrupted`, puis (après `afterMs`) la reprise : un `status`, les lignes manquées, sans doublon. */
  interrupt(
    serverId: string,
    serviceKey: string,
    options: { afterMs?: number; whileCut?: SimLineInput[]; resumeLost?: boolean } = {},
  ): void {
    const readers = this.matching(serverId, serviceKey);
    this.suspend(readers);
    this.keep(
      this.feed(serverId, serviceKey),
      (options.whileCut ?? []).map((input) => this.line(input)),
    );
    setTimeout(() => this.resume(readers, options.resumeLost === true), options.afterMs ?? 20);
  }

  /** Le lien avec ce serveur est coupé : toutes ses lectures sont interrompues (et reprendront au retour du lien). */
  linkDown(serverId: string): void {
    this.suspend([...this.readers].filter((r) => !r.closed && r.serverId === serverId));
  }

  /** Le lien est revenu : les lectures reprennent par leur curseur, sans doublon. */
  linkUp(serverId: string): void {
    this.resume(
      [...this.readers].filter((r) => !r.closed && r.serverId === serverId),
      false,
    );
  }

  private suspend(readers: Reader[]): void {
    for (const reader of readers) {
      if (reader.suspended) continue;
      reader.suspended = true;
      reader.listener({ kind: "interrupted" });
    }
  }

  private resume(readers: Reader[], lost: boolean): void {
    for (const reader of readers) {
      if (reader.closed || !reader.suspended) continue;
      reader.suspended = false;
      const feed = this.feed(reader.serverId, reader.key);
      reader.listener(feed.status);
      const missed = feed.total - reader.seen;
      reader.seen = feed.total;
      if (lost || missed > feed.lines.length) {
        reader.listener({ kind: "gap", count: null, reason: "resume_lost" });
        reader.listener({ kind: "lines", seq: ++this.batches, lines: feed.lines.slice(-1_000) });
      } else if (missed > 0) {
        reader.listener({ kind: "lines", seq: ++this.batches, lines: feed.lines.slice(-missed) });
      }
    }
  }

  /** La lecture finit (refus de l'agent, service disparu, droit retiré...) : un `ended` à toutes les lectures. */
  end(serverId: string, serviceKey: string, end: LogEnd): void {
    for (const reader of this.matching(serverId, serviceKey)) {
      reader.listener({ kind: "ended", end });
      if (!reader.closed) this.setShellCount(this.shellCount() - 1);
      reader.closed = true;
    }
  }

  /** La prochaine ouverture est refusée (une fois) avec ce motif. */
  refuseNext(
    refusal: Extract<LogEnd, { kind: "refused" }>["refusal"],
    retryAfterS: number | null = null,
  ): void {
    this.refusal = { kind: "refused", refusal, retryAfterS };
  }

  /** Les accusés reçus (lots que l'interface a traités), par lecture ouverte la plus récente. */
  ackedBatches(): number {
    return [...this.readers].reduce((sum, reader) => sum + reader.acked, 0);
  }

  private matching(serverId: string, serviceKey: string): Reader[] {
    return [...this.readers].filter(
      (reader) => !reader.closed && reader.serverId === serverId && reader.key === serviceKey,
    );
  }

  private broadcast(serverId: string, serviceKey: string, message: LogMessage): void {
    for (const reader of this.matching(serverId, serviceKey)) {
      if (!reader.suspended) reader.listener(message);
    }
  }

  /** Le frein aux ouvertures : douze en rafale, une rendue toutes les cinq secondes. `null` : l'ouverture passe. */
  private braked(): number | null {
    if (!this.brake) return null;
    const now = Date.now();
    this.opens = this.opens.filter((at) => now - at < SIM_OPEN_BURST * SIM_OPEN_REFILL_S * 1000);
    if (this.opens.length >= SIM_OPEN_BURST) {
      const oldest = this.opens[this.opens.length - SIM_OPEN_BURST] ?? now;
      const wait = Math.ceil((oldest + SIM_OPEN_REFILL_S * 1000 - now) / 1000);
      return Math.max(1, wait);
    }
    this.opens.push(now);
    return null;
  }

  /**
   * Ouvre une lecture. Les refus arrivent comme `ended`, après la fin de l'appel (comme l'agent : la socket
   * s'ouvre, puis l'erreur arrive).
   */
  async open(
    serverId: string,
    serviceKey: string,
    listener: (message: LogMessage) => void,
    options: { refuse?: Extract<LogEnd, { kind: "refused" }> | null } = {},
  ): Promise<LogReading> {
    if (this.failOpen) throw new Error("ouverture impossible");
    if (this.shellCount() >= this.shellLimit)
      throw new LinkCommandError({ kind: "log_reads_full" });
    this.setShellCount(this.shellCount() + 1);
    if (this.openDelayMs > 0) await new Promise((r) => setTimeout(r, this.openDelayMs));
    const reader: Reader = {
      serverId,
      key: serviceKey,
      listener,
      closed: false,
      suspended: false,
      acked: 0,
      seen: 0,
    };
    this.opened.push({ serverId, key: serviceKey });
    const refused = options.refuse ?? this.refusal;
    this.refusal = null;
    const wait = this.braked();
    const tooMany = this.openCount() >= this.accountLimit;
    queueMicrotask(() => {
      if (reader.closed) return;
      const end = refused
        ? refused
        : wait !== null
          ? ({ kind: "refused", refusal: "logs_rate_limited", retryAfterS: wait } as const)
          : tooMany
            ? ({ kind: "refused", refusal: "logs_account_limit", retryAfterS: null } as const)
            : null;
      if (end) {
        listener({ kind: "ended", end });
        reader.closed = true;
        this.setShellCount(this.shellCount() - 1);
        return;
      }
      this.readers.add(reader);
      const feed = this.feed(serverId, serviceKey);
      reader.seen = feed.total;
      listener(feed.status);
      if (feed.lines.length > 0) {
        listener({
          kind: "lines",
          seq: ++this.batches,
          lines: feed.lines.slice(-1_000),
        });
      }
    });
    return {
      close: () => {
        if (reader.closed) return;
        reader.closed = true;
        this.readers.delete(reader);
        if (!this.loseCloses) this.setShellCount(this.shellCount() - 1);
        this.closedCount += 1;
      },
      ack: (batches) => {
        reader.acked += batches;
      },
    };
  }

  /** L'état de lecture à l'ouverture d'un service : de quoi seeder un service arrêté. */
  static stopped(code: number | null = 0): Partial<Extract<LogMessage, { kind: "status" }>> {
    return { state: "ended" as LogState, exitCode: code, endedAt: Date.now() - 60_000 };
  }
}
