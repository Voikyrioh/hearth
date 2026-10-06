import type { AuditEntry, AuditExportResult, AuditFilter, AuditKind, AuditPage } from "./audit";
import { LinkCommandError, type Role, type Unsubscribe } from "./types";

/**
 * Journal d'activité simulé (développement, tests, Playwright) : ce que fait l'agent, en petit.
 * L'interface n'y filtre rien elle-même : c'est ICI, comme chez l'agent, que se font le filtrage
 * (comptes, types d'action, résultats, période, recherche), la pagination par curseur et le
 * refus d'un compte lecture seule. Pilotable : `add`, `addBurst`, `seed`, `exportMode`.
 */

export interface SimulatedAuditOptions {
  now: () => number;
  /** Le lien du serveur est-il « Connecté » ? Sinon, aucune lecture, aucun direct. */
  connected: (serverId: string) => boolean;
  role: (serverId: string) => Role;
}

/** Codes d'action et résultats de chaque type de la spec (miroir de l'agent). */
const KINDS: Record<AuditKind, { codes: string[]; outcomes: string[] }> = {
  login_ok: { codes: ["login"], outcomes: ["ok"] },
  login_denied: { codes: ["login", "login.locked"], outcomes: ["denied"] },
  accounts: {
    codes: [
      "account.create",
      "account.delete",
      "account.role",
      "account.password",
      "account.password.own",
      "sessions.revoke",
    ],
    outcomes: ["ok", "denied", "failed"],
  },
  update: { codes: ["agent.update"], outcomes: ["ok", "denied", "failed"] },
  denied: {
    codes: [
      "logout",
      "account.create",
      "account.delete",
      "account.role",
      "account.password",
      "account.password.own",
      "sessions.revoke",
      "accounts.read",
      "audit.read",
      "agent.update",
    ],
    outcomes: ["denied"],
  },
};

const LABELS: Record<string, string> = {
  login: "Connexion",
  "login.locked": "Blocage temporaire",
  logout: "Déconnexion",
  "account.create": "Création de compte",
  "account.delete": "Suppression de compte",
  "account.role": "Changement de rôle",
  "account.password": "Changement du mot de passe d'un compte",
  "account.password.own": "Changement de son mot de passe",
  "sessions.revoke": "Fermeture des sessions",
  "accounts.read": "Consultation des comptes",
  "audit.read": "Tentative de lecture du journal",
  "agent.update": "Mise à jour de l'agent",
};

const fold = (text: string) => text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

export const SIM_PAGE_SIZE = 100;

export type NewAuditEntry = Partial<Omit<AuditEntry, "id" | "at" | "atMs">> & { atMs?: number };

export class SimulatedAudit {
  private readonly journals = new Map<string, AuditEntry[]>();
  private readonly listeners = new Map<string, Set<(entry: AuditEntry) => void>>();
  private nextId = 1;
  /** Lectures reçues (observable dans les tests) : filtre, curseur. */
  readonly reads: { serverId: string; filter: AuditFilter; before: number | null }[] = [];
  /** Exports reçus (observable dans les tests). */
  readonly exports: { serverId: string; filter: AuditFilter }[] = [];
  /** Ce que fait le prochain export : enregistré, annulé dans la boîte de dialogue, ou en échec. */
  exportMode: "save" | "cancel" | "fail" = "save";
  exportTruncated = false;
  /** Une lecture sur deux échoue tant que c'est vrai (tests de l'erreur de chargement). */
  failReads = false;

  constructor(private readonly options: SimulatedAuditOptions) {}

  private journal(serverId: string): AuditEntry[] {
    let list = this.journals.get(serverId);
    if (!list) {
      list = [];
      this.journals.set(serverId, list);
    }
    return list;
  }

  /** Vide le journal d'un serveur (les identifiants repartent d'où ils en sont). */
  clear(serverId: string): void {
    this.journals.set(serverId, []);
  }

  /** Une entrée écrite maintenant (ou à `atMs`) ; envoyée en direct si le lien est établi. */
  add(serverId: string, partial: NewAuditEntry = {}): AuditEntry {
    const atMs = partial.atMs ?? this.options.now();
    const action = partial.action ?? "login";
    const entry: AuditEntry = {
      id: this.nextId++,
      at: new Date(atMs).toISOString(),
      atMs,
      account: partial.account === undefined ? "marie" : partial.account,
      origin: partial.origin ?? {
        kind: "client",
        name: "poste-de-marie",
        addr: "192.168.1.20",
        text: "192.168.1.20 (poste-de-marie)",
      },
      action,
      actionLabel: partial.actionLabel ?? LABELS[action] ?? action,
      target: partial.target === undefined ? null : partial.target,
      outcome: partial.outcome ?? "ok",
      reason: partial.reason === undefined ? null : partial.reason,
      repeatCount: partial.repeatCount ?? 0,
    };
    this.journal(serverId).push(entry);
    // Un lien coupé ne livre rien : l'entrée sera à rattraper par une lecture.
    if (this.options.connected(serverId)) {
      for (const listener of [...(this.listeners.get(serverId) ?? [])]) listener({ ...entry });
    }
    return entry;
  }

  /** Une rafale de connexions refusées depuis la même adresse, une toutes les `everyMs`. */
  addBurst(serverId: string, count: number, addr = "203.0.113.9", everyMs = 5000): AuditEntry[] {
    const start = this.options.now();
    const out: AuditEntry[] = [];
    for (let i = 0; i < count; i += 1) {
      out.push(
        this.add(serverId, {
          atMs: start + i * everyMs,
          account: null,
          origin: { kind: "client", name: null, addr, text: `${addr} (inconnu)` },
          outcome: "denied",
          reason: "identifiants incorrects",
        }),
      );
    }
    return out;
  }

  /** `count` entrées variées, de plus en plus anciennes (jusqu'à maintenant), sans direct. */
  seed(serverId: string, count: number): void {
    const end = this.options.now();
    const accounts = ["marie", "paul", "léa"];
    const codes = ["login", "logout", "account.create", "account.role", "agent.update"];
    const list = this.journal(serverId);
    for (let i = 0; i < count; i += 1) {
      const atMs = end - (count - i) * 60_000;
      const action = codes[i % codes.length] as string;
      const refused = i % 7 === 3;
      const entry: AuditEntry = {
        id: this.nextId++,
        at: new Date(atMs).toISOString(),
        atMs,
        account: accounts[i % accounts.length] as string,
        origin: {
          kind: "client",
          name: `poste-${(i % 3) + 1}`,
          addr: `192.168.1.${20 + (i % 3)}`,
          text: `192.168.1.${20 + (i % 3)} (poste-${(i % 3) + 1})`,
        },
        action,
        actionLabel: LABELS[action] ?? action,
        target: action.startsWith("account") ? `compte-${i}` : null,
        outcome: refused ? "denied" : "ok",
        reason: refused ? "lecture seule" : null,
        repeatCount: 0,
      };
      list.push(entry);
    }
  }

  private matches(entry: AuditEntry, filter: AuditFilter): boolean {
    if (filter.accounts.length > 0) {
      const accounts = filter.accounts.map(fold);
      if (!entry.account || !accounts.includes(fold(entry.account))) return false;
    }
    if (filter.outcomes.length > 0 && !filter.outcomes.includes(entry.outcome)) return false;
    if (filter.kinds.length > 0) {
      const hit = filter.kinds.some((kind) => {
        const rule = KINDS[kind];
        return rule.codes.includes(entry.action) && rule.outcomes.includes(entry.outcome);
      });
      if (!hit) return false;
    }
    if (filter.fromS !== null && entry.atMs < filter.fromS * 1000) return false;
    if (filter.toS !== null && entry.atMs >= (filter.toS + 1) * 1000) return false;
    if (filter.text !== "") {
      const hay = [
        entry.account,
        entry.origin.addr,
        entry.origin.name,
        entry.origin.text,
        entry.actionLabel,
        entry.target,
        entry.reason,
      ]
        .filter((value): value is string => value !== null)
        .map(fold)
        .join(" ");
      const words = hay.split(/[^\p{L}\p{N}]+/u);
      // Plusieurs mots : ET ; chaque mot est un début de mot (jamais une requête).
      for (const term of fold(filter.text).split(/\s+/)) {
        if (term !== "" && !words.some((word) => word.startsWith(term))) return false;
      }
    }
    return true;
  }

  private guard(serverId: string): void {
    if (this.options.role(serverId) !== "admin") {
      throw new LinkCommandError({ kind: "forbidden" });
    }
    if (!this.options.connected(serverId)) throw new LinkCommandError({ kind: "not_connected" });
  }

  read(serverId: string, filter: AuditFilter, before: number | null): AuditPage {
    this.reads.push({
      serverId,
      filter: JSON.parse(JSON.stringify(filter)) as AuditFilter,
      before,
    });
    this.guard(serverId);
    if (this.failReads) throw new LinkCommandError({ kind: "unreachable" });
    const found = this.journal(serverId)
      .filter((entry) => (before === null || entry.id < before) && this.matches(entry, filter))
      .sort((a, b) => b.id - a.id);
    const events = found.slice(0, SIM_PAGE_SIZE).map((entry) => ({ ...entry }));
    const more = found.length > SIM_PAGE_SIZE;
    const last = events[events.length - 1];
    return { events, nextBefore: more && last ? last.id : null };
  }

  export(serverId: string, filter: AuditFilter): AuditExportResult {
    this.exports.push({ serverId, filter: JSON.parse(JSON.stringify(filter)) as AuditFilter });
    this.guard(serverId);
    if (this.exportMode === "fail") throw new LinkCommandError({ kind: "storage" });
    return { saved: this.exportMode === "save", truncated: this.exportTruncated };
  }

  subscribe(serverId: string, listener: (entry: AuditEntry) => void): Unsubscribe {
    let set = this.listeners.get(serverId);
    if (!set) {
      set = new Set();
      this.listeners.set(serverId, set);
    }
    set.add(listener);
    return () => void set.delete(listener);
  }

  /** Nombre d'écouteurs du direct sur un serveur (les tests vérifient qu'on se désabonne). */
  listenerCount(serverId: string): number {
    return this.listeners.get(serverId)?.size ?? 0;
  }
}
