import type {
  AttackMode,
  AttackModeOutcome,
  SecurityAlert,
  SecurityDevice,
  SecurityRead,
  SecurityState,
} from "./security";
import { LinkCommandError, type Role } from "./types";

/**
 * Sécurité du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit agent qui
 * applique les mêmes règles que le vrai, et que l'appelant PILOTE (alerte, mode attaque, état du poste,
 * agent trop ancien). Jamais livré (derrière `import.meta.env.DEV`). Aucune clé n'existe ici : seul
 * le fait « ce PC a une clé inscrite et prouvée » (`device`, `keyAtHand`) est simulé, comme le voit
 * l'interface. Le mot de passe n'est ni gardé ni noté.
 */
interface Book {
  seq: number;
  /** Faux : agent d'avant l'alerte et le mode attaque (la lecture dit `unsupported`). */
  supported: boolean;
  alert: SecurityAlert;
  attackMode: AttackMode;
  device: SecurityDevice;
  keyAtHand: boolean;
  erasurePending: boolean;
  /** Ce que la DERNIÈRE LECTURE a dit : le message du flux ne porte pas l'effacement, il garde cette valeur (comme la coquille). */
  erasureRead: boolean;
}

function freshBook(): Book {
  return {
    seq: 0,
    supported: true,
    alert: { own: false, since: null, others: null },
    attackMode: { state: "off", since: null, resumesInS: null, lastEnd: null },
    device: "proven",
    keyAtHand: true,
    erasurePending: false,
    erasureRead: false,
  };
}

export class SimulatedSecurity {
  private readonly books = new Map<string, Book>();
  private readonly listeners = new Set<(state: SecurityState) => void>();

  constructor(private readonly now: () => number) {}

  private book(serverId: string): Book {
    let book = this.books.get(serverId);
    if (!book) {
      book = freshBook();
      this.books.set(serverId, book);
    }
    return book;
  }

  private view(serverId: string, book: Book): SecurityState {
    return {
      serverId,
      seq: book.seq,
      alert: { ...book.alert },
      attackMode: { ...book.attackMode },
      device: book.device,
      keyAtHand: book.keyAtHand,
      erasurePending: book.erasurePending,
    };
  }

  /** Un message du flux : alerte et mode changent, pas le poste (l'interface relit). */
  private publish(serverId: string): void {
    const book = this.book(serverId);
    if (!book.supported) return;
    book.seq += 1;
    // Le message du flux ne dit rien du poste : `unknown` tant qu'une lecture ne l'a pas donné.
    const state = {
      ...this.view(serverId, book),
      device: "unknown" as const,
      erasurePending: book.erasureRead,
    };
    for (const listener of this.listeners) listener({ ...state });
  }

  // --- Pilotage par le test ---

  /** L'identifiant du compte est visé (ou plus). `others` : administrateur seulement. */
  setAlert(
    serverId: string,
    alert: { own: boolean; others?: number | null; since?: string | null },
  ): void {
    this.book(serverId).alert = {
      own: alert.own,
      since: alert.own ? (alert.since ?? new Date(this.now()).toISOString()) : null,
      others: alert.others ?? null,
    };
    this.publish(serverId);
  }

  /** Le mode attaque du serveur passe à cet état (activation par un autre poste, fin automatique…). */
  setMode(
    serverId: string,
    state: AttackMode["state"],
    options: { resumesInS?: number; lastEnd?: AttackMode["lastEnd"] } = {},
  ): void {
    const book = this.book(serverId);
    book.attackMode = {
      state,
      since: state === "off" ? null : (book.attackMode.since ?? new Date(this.now()).toISOString()),
      resumesInS: state === "suspended" ? (options.resumesInS ?? 1800) : null,
      lastEnd: state === "off" ? (options.lastEnd ?? book.attackMode.lastEnd) : null,
    };
    this.publish(serverId);
  }

  /** Ce que l'agent dit de ce poste : prouvé ou non, clé au coffre ou non. Ne publie rien. */
  setDevice(serverId: string, device: "proven" | "none", keyAtHand = device === "proven"): void {
    const book = this.book(serverId);
    book.device = device;
    book.keyAtHand = keyAtHand;
  }

  /** L'agent dit qu'un effacement est en attente (administrateur seulement). Ne publie rien : la lecture le donne. */
  setErasurePending(serverId: string, pending: boolean): void {
    this.book(serverId).erasurePending = pending;
  }

  /** Faux : agent d'avant l'alerte et le mode attaque. */
  setSupported(serverId: string, supported: boolean): void {
    this.book(serverId).supported = supported;
  }

  /** L'état courant (tests). */
  current(serverId: string): SecurityState {
    return this.view(serverId, this.book(serverId));
  }

  // --- Ce que fait le pont ---

  /** Abonnement : rejoue le dernier état de chaque serveur ayant un état (comme la coquille). */
  subscribe(listener: (state: SecurityState) => void): () => void {
    this.listeners.add(listener);
    for (const [serverId, book] of this.books) {
      if (book.supported && book.seq > 0) {
        listener({
          ...this.view(serverId, book),
          device: "unknown",
          erasurePending: book.erasureRead,
        });
      }
    }
    return () => void this.listeners.delete(listener);
  }

  /** `GET /security` : l'état complet, poste compris ; le numéro de séquence avance (comme la coquille). */
  read(serverId: string): SecurityRead {
    const book = this.book(serverId);
    if (!book.supported) return { kind: "unsupported" };
    book.seq += 1;
    book.erasureRead = book.erasurePending;
    return { kind: "known", state: this.view(serverId, book) };
  }

  /**
   * Mêmes refus que l'agent, dans le même ordre : rôle (Lecture seule : `forbidden`), poste sans clé
   * (`not_recognized`, rien n'est parti), agent trop ancien, mot de passe. L'activation d'un mode déjà
   * actif ne change rien (idempotente).
   */
  change(
    serverId: string,
    active: boolean,
    password: string,
    own: string,
    role: Role,
  ): AttackModeOutcome {
    const book = this.book(serverId);
    if (!book.supported) return { kind: "refused", refusal: { kind: "unsupported" } };
    // Sans clé au coffre : la coquille ne part pas (`NotRecognized`), même pour un Lecture seule.
    if (!book.keyAtHand) throw new LinkCommandError({ kind: "not_recognized" });
    if (role !== "admin") throw new LinkCommandError({ kind: "forbidden" });
    // Q18 : l'agent accepte la preuve de TOUTE clé inscrite du compte, pas seulement celle qui a prouvé
    // la session. La clé au coffre est donc la seule condition (`keyAtHand`, déjà vérifiée).
    if (password !== own) return { kind: "refused", refusal: { kind: "wrong_password" } };
    const wasOn = book.attackMode.state !== "off";
    if (active && !wasOn) {
      book.attackMode = {
        state: "active",
        since: new Date(this.now()).toISOString(),
        resumesInS: null,
        lastEnd: null,
      };
      this.publish(serverId);
    } else if (!active && wasOn) {
      book.attackMode = { state: "off", since: null, resumesInS: null, lastEnd: "manual" };
      this.publish(serverId);
    }
    return { kind: "done", attackMode: { ...book.attackMode } };
  }
}
