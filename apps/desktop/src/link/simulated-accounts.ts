import type { ServiceRights } from "./services";
import {
  type Account,
  type AccountInputCheck,
  type AccountOutcome,
  type AccountRefusal,
  LinkCommandError,
  type PasswordRule,
  type Role,
  type ServerInfo,
  type UsernameProblem,
} from "./types";

/**
 * Comptes du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui
 * applique les mêmes règles que le vrai (dernier administrateur, rôle, confirmation de la suppression
 * de son propre compte, sessions fermées). Jamais livré (derrière `import.meta.env.DEV`).
 *
 * LA règle de format d'un identifiant et d'un mot de passe est celle de `hearth-proto`
 * (`account_rules.rs`), évaluée par la coquille. Ce fichier n'en est qu'une réplique pour le
 * navigateur : un fichier de vecteurs commun (`crates/hearth-proto/tests/vectors/account-input.json`)
 * est joué par un test Rust ET un test Vitest, une divergence casse l'un des deux.
 */
export function simulatedCheckInput(username: string, password: string): AccountInputCheck {
  const problem = usernameProblem(username);
  const normalized = problem === null ? asciiLower(username) : "";
  return { username: problem, password: unmetRules(password, normalized) };
}

/** Minuscules ASCII seulement, comme `eq_ignore_ascii_case` de l'agent (pas `toLowerCase`, qui replie aussi le signe kelvin). */
function asciiLower(text: string): string {
  return text.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

function usernameProblem(raw: string): UsernameProblem | null {
  if (raw === "") return "empty";
  if (!/^[A-Za-z0-9_-]*$/.test(raw)) return "invalid_chars";
  const length = [...raw].length;
  if (length < 3) return "too_short";
  if (length > 32) return "too_long";
  return null;
}

function unmetRules(password: string, username: string): PasswordRule[] {
  if (password === "") return ["required"];
  const unmet: PasswordRule[] = [];
  if ([...password].length < 12) unmet.push("min_length");
  if (!/[0-9]/.test(password)) unmet.push("digit");
  if (!/\p{Lowercase}/u.test(password)) unmet.push("lowercase");
  if (!/\p{Uppercase}/u.test(password)) unmet.push("uppercase");
  if (username !== "" && asciiLower(password).includes(username)) unmet.push("contains_username");
  return unmet;
}

interface SimAccount {
  id: string;
  username: string;
  role: Role;
  createdAt: string;
  lastLoginAt: string | null;
  sessions: number;
  /** Le droit « gérer les services » (ADR-0041) : il ne se donne que par `setRight`, comme la commande du serveur. */
  managesServices: boolean;
}

/** Le résultat d'une action simulée ; `ended` : la session de l'utilisateur lui-même a été fermée. */
export interface SimAccountResult {
  outcome: AccountOutcome;
  ended: boolean;
}

const refuse = (refusal: AccountRefusal): SimAccountResult => ({
  outcome: { kind: "refused", refusal },
  ended: false,
});

const done = (
  account: Account | null,
  sessionsClosed: number,
  ended = false,
): SimAccountResult => ({
  outcome: { kind: "done", account, sessionsClosed },
  ended,
});

export class SimulatedAccounts {
  private readonly books = new Map<string, SimAccount[]>();
  private nextId = 1;
  /** Comptes que l'agent a envoyés sans que ce client sache les lire (écartés, comptés), par serveur. */
  readonly unreadable = new Map<string, number>();

  constructor(private readonly now: () => number) {}

  /** Compte de départ d'un serveur : l'utilisateur lui-même, et de quoi remplir une liste. */
  private book(server: ServerInfo): SimAccount[] {
    let book = this.books.get(server.id);
    if (!book) {
      const day = 86_400_000;
      const at = (ago: number) => new Date(this.now() - ago).toISOString();
      book = [
        {
          id: this.newId(),
          username: server.username.toLowerCase(),
          role: server.role,
          createdAt: at(30 * day),
          lastLoginAt: at(0),
          sessions: 1,
          // Par défaut le compte connecté peut gérer les services : les écrans qui n'en parlent pas ne changent pas.
          managesServices: true,
        },
      ];
      if (server.role === "admin") {
        book.push(
          {
            id: this.newId(),
            username: "paul",
            role: "readonly",
            createdAt: at(12 * day),
            lastLoginAt: at(2 * day),
            sessions: 2,
            managesServices: false,
          },
          {
            id: this.newId(),
            username: "lea",
            role: "readonly",
            createdAt: at(3 * day),
            lastLoginAt: null,
            sessions: 0,
            managesServices: false,
          },
        );
      }
      this.books.set(server.id, book);
    }
    return book;
  }

  private newId(): string {
    return `SIMACCOUNT${String(this.nextId++).padStart(4, "0")}`;
  }

  private view(account: SimAccount): Account {
    return {
      id: account.id,
      username: account.username,
      role: account.role,
      createdAt: account.createdAt,
      lastLoginAt: account.lastLoginAt,
      sessionsOpen: account.sessions,
      managesServices: account.managesServices,
    };
  }

  /** Le compte de la session : l'agent simulé normalise l'identifiant saisi, comme le vrai. */
  private me(server: ServerInfo): SimAccount | undefined {
    const typed = server.username.toLowerCase();
    return this.book(server).find((account) => account.username === typed);
  }

  /** L'identifiant de l'agent du compte de la session (`GET /me`). */
  meId(server: ServerInfo): string {
    return this.me(server)?.id ?? "";
  }

  /** L'agent refuse tout compte Lecture seule, même client contourné (BR-ACCT-014). */
  private forbidden(server: ServerInfo): boolean {
    return server.role !== "admin";
  }

  /** Lecture seule : l'agent refuse (`forbidden`, la seule façon de dire « accès refusé »). */
  list(server: ServerInfo): Account[] {
    if (this.forbidden(server)) throw new LinkCommandError({ kind: "forbidden" });
    const known = this.rightsKnown(server);
    return this.book(server).map((account) => ({
      ...this.view(account),
      // Un agent qui ne connaît pas le droit n'envoie pas `rights` : lu comme vide (BR-GRANT-016).
      managesServices: known && account.managesServices,
    }));
  }

  /** La capacité `service-rights` de l'agent simulé (vraie par défaut ; `setRightsKnown(…, false)` : agent ancien). */
  private readonly oldAgents = new Set<string>();

  rightsKnown(server: ServerInfo): boolean {
    return !this.oldAgents.has(server.id);
  }

  /** La capacité `game-settings` de l'agent simulé (vraie par défaut, indépendante du droit : `false` ne retire aucun droit). */
  private readonly withoutGameSettings = new Set<string>();

  setGameSettingsServed(serverId: string, served: boolean): void {
    if (served) this.withoutGameSettings.delete(serverId);
    else this.withoutGameSettings.add(serverId);
  }

  setRightsKnown(serverId: string, known: boolean): void {
    if (known) this.oldAgents.delete(serverId);
    else this.oldAgents.add(serverId);
  }

  /**
   * Ce que le serveur fait quand on lui lance `hearth-agent account services {compte} --grant|--revoke` : le
   * SEUL chemin qui change le droit (BR-GRANT-005). Aucune action du pont ne l'appelle ; les tests le jouent pour
   * dire « le serveur a accordé / retiré le droit ».
   */
  setRight(server: ServerInfo, username: string, granted: boolean): void {
    const account = this.book(server).find((a) => a.username === username.toLowerCase());
    if (account) account.managesServices = granted;
  }

  /** Le droit du compte connecté tel que `GET /me` + `GET /hello` le donnent (tout rôle peut le lire). */
  ownRights(server: ServerInfo): ServiceRights {
    const me = this.me(server);
    const known = this.rightsKnown(server);
    return {
      known,
      granted: known && (me?.managesServices ?? false),
      // Par défaut l'agent qui connaît le droit annonce aussi les réglages de jeu (comme la fiche de référence) ;
      // `setGameSettingsServed(…, false)` retire la capacité SEULE.
      gameSettings: known && !this.withoutGameSettings.has(server.id),
      account: me?.username ?? server.username.toLowerCase(),
    };
  }

  create(server: ServerInfo, username: string, password: string, role: Role): SimAccountResult {
    if (this.forbidden(server)) throw new LinkCommandError({ kind: "forbidden" });
    const check = simulatedCheckInput(username, password);
    if (check.username) return refuse({ kind: "invalid_username", problem: check.username });
    if (check.password.length > 0) return refuse({ kind: "weak_password", rules: check.password });
    const book = this.book(server);
    if (book.some((account) => account.username === username.toLowerCase())) {
      return refuse({ kind: "username_taken" });
    }
    const account: SimAccount = {
      id: this.newId(),
      username: username.toLowerCase(),
      role,
      createdAt: new Date(this.now()).toISOString(),
      lastLoginAt: null,
      sessions: 0,
      // Un compte naît sans le droit, quel que soit son rôle (BR-GRANT-002).
      managesServices: false,
    };
    book.push(account);
    return done(this.view(account), 0);
  }

  private adminCount(server: ServerInfo): number {
    return this.book(server).filter((account) => account.role === "admin").length;
  }

  changeRole(server: ServerInfo, accountId: string, role: Role): SimAccountResult {
    if (this.forbidden(server)) throw new LinkCommandError({ kind: "forbidden" });
    const account = this.book(server).find((a) => a.id === accountId);
    if (!account) return refuse({ kind: "not_found" });
    if (account.role === "admin" && role !== "admin" && this.adminCount(server) <= 1) {
      return refuse({ kind: "last_admin" });
    }
    account.role = role;
    return done(null, 0);
  }

  setPassword(server: ServerInfo, accountId: string, password: string): SimAccountResult {
    if (this.forbidden(server)) throw new LinkCommandError({ kind: "forbidden" });
    const account = this.book(server).find((a) => a.id === accountId);
    if (!account) return refuse({ kind: "not_found" });
    const rules = simulatedCheckInput(account.username, password).password;
    if (rules.length > 0) return refuse({ kind: "weak_password", rules });
    const closed = account.sessions;
    account.sessions = 0;
    return done(null, closed, account.id === this.me(server)?.id);
  }

  /** Le titulaire change son mot de passe : l'ancien est vérifié par `currentIsRight`. */
  changeOwn(server: ServerInfo, currentIsRight: boolean, password: string): SimAccountResult {
    const me = this.me(server);
    if (!me) return refuse({ kind: "not_found" });
    if (!currentIsRight) return refuse({ kind: "wrong_password" });
    const rules = simulatedCheckInput(me.username, password).password;
    if (rules.length > 0) return refuse({ kind: "weak_password", rules });
    const closed = Math.max(0, me.sessions - 1);
    me.sessions = Math.min(me.sessions, 1);
    return done(null, closed);
  }

  closeSessions(server: ServerInfo, accountId: string): SimAccountResult {
    if (this.forbidden(server)) throw new LinkCommandError({ kind: "forbidden" });
    const account = this.book(server).find((a) => a.id === accountId);
    if (!account) return refuse({ kind: "not_found" });
    const closed = account.sessions;
    account.sessions = 0;
    return done(null, closed, account.id === this.me(server)?.id);
  }

  delete(server: ServerInfo, accountId: string, confirmation: string | null): SimAccountResult {
    if (this.forbidden(server)) throw new LinkCommandError({ kind: "forbidden" });
    const book = this.book(server);
    const account = book.find((a) => a.id === accountId);
    if (!account) return refuse({ kind: "not_found" });
    const self = account.id === this.me(server)?.id;
    // BR-GRANT-005 : le compte d'un titulaire n'est supprimé que par lui-même, un autre titulaire ou le serveur.
    if (
      this.rightsKnown(server) &&
      account.managesServices &&
      !self &&
      this.me(server)?.managesServices !== true
    ) {
      throw new LinkCommandError({ kind: "missing_services_right" });
    }
    if (self && (confirmation ?? "").trim().toLowerCase() !== account.username) {
      return refuse({ kind: "confirmation_mismatch" });
    }
    if (account.role === "admin" && this.adminCount(server) <= 1) {
      return refuse({ kind: "last_admin" });
    }
    this.books.set(
      server.id,
      book.filter((a) => a.id !== accountId),
    );
    return done(null, account.sessions, self);
  }

  /** Amorçage des tests : remplace les comptes d'un serveur. */
  seed(serverId: string, accounts: Array<Partial<SimAccount> & { username: string }>): void {
    this.books.set(
      serverId,
      accounts.map((account) => ({
        id: account.id ?? this.newId(),
        username: account.username,
        role: account.role ?? "readonly",
        createdAt: account.createdAt ?? new Date(this.now()).toISOString(),
        lastLoginAt: account.lastLoginAt ?? null,
        sessions: account.sessions ?? 0,
        managesServices: account.managesServices ?? false,
      })),
    );
  }
}
