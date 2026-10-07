import { type AdminActKind, ELEVATION_SECONDS, type ReauthMode, type ReauthState } from "./reauth";
import { LinkCommandError, type Role } from "./types";

/**
 * La confirmation des actes du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit
 * agent qui applique les mêmes règles que le vrai (BR-TRUST-036, 040, 042, 043) et que l'appelant PILOTE
 * (agent ancien, poste sans clé, réglage, élévation ouverte ou fermée). Jamais livré (derrière
 * `import.meta.env.DEV`). Aucune clé n'existe ici : seul le fait « ce PC a une clé au coffre » est
 * simulé (c'est `keyAtHand` de la sécurité simulée). Le mot de passe n'est ni gardé ni noté.
 */

/** Faux : agent d'avant la confirmation des actes (aucune demande en plus). */
interface Book {
  supported: boolean;
  required: boolean;
  mode: ReauthMode;
  /** Fin de l'élévation (millisecondes), 0 : fermée. */
  elevatedUntil: number;
  /** Mots de passe faux de suite : au-delà de 5 l'agent fait attendre. */
  wrong: number;
}

const FREE_TRIES = 5;
const WAIT_SECONDS = 30;

/**
 * L'élévation couvre-t-elle cet acte ? Le miroir de `hearth_proto::admin_act::covered_by_elevation` pour
 * le simulateur seulement : le vrai pont demande la règle à la coquille (`reauthCovers`).
 */
export function simulatedCovers(kind: AdminActKind, role: Role | null): boolean {
  switch (kind) {
    case "account_create":
    case "account_role":
      return role === "readonly";
    case "account_delete":
    case "sessions_revoke":
      return true;
    case "account_password":
    case "agent_update":
    case "attack_mode_enable":
    case "attack_mode_disable":
    case "account_password_own":
    case "reauth_setting":
      return false;
  }
}

export type SimConfirmation =
  | { kind: "ok" }
  | {
      kind: "refused";
      refusal:
        | { kind: "wrong_password" }
        | { kind: "password_required" }
        | { kind: "too_many_attempts"; retry_after_s: number };
    };

export class SimulatedReauth {
  private readonly books = new Map<string, Book>();

  constructor(
    private readonly now: () => number,
    /** Ce PC a-t-il une clé au coffre pour ce serveur ? (la sécurité simulée le sait) */
    private readonly hasKey: (serverId: string) => boolean,
  ) {}

  private book(serverId: string): Book {
    let book = this.books.get(serverId);
    if (!book) {
      book = { supported: true, required: true, mode: "window", elevatedUntil: 0, wrong: 0 };
      this.books.set(serverId, book);
    }
    return book;
  }

  // --- Pilotage par le test ---

  /** Faux : agent d'avant la confirmation des actes. */
  setSupported(serverId: string, supported: boolean): void {
    this.book(serverId).supported = supported;
  }

  /** Le réglage du compte ; « à chaque action » ferme l'élévation. */
  setMode(serverId: string, mode: ReauthMode): void {
    const book = this.book(serverId);
    book.mode = mode;
    if (mode === "each") book.elevatedUntil = 0;
  }

  /** Ouvre l'élévation pour `seconds` secondes (défaut : 5 minutes), comme un mot de passe juste. */
  open(serverId: string, seconds = ELEVATION_SECONDS): void {
    this.book(serverId).elevatedUntil = this.now() + seconds * 1000;
  }

  /** Ferme l'élévation (délai écoulé, mode attaque, retrait d'un poste…). */
  close(serverId: string): void {
    this.book(serverId).elevatedUntil = 0;
  }

  // --- Ce que fait le pont ---

  state(serverId: string): ReauthState {
    const book = this.book(serverId);
    const left = Math.max(0, Math.ceil((book.elevatedUntil - this.now()) / 1000));
    return {
      supported: book.supported,
      required: book.supported && book.required,
      mode: book.mode,
      elevatedForS: book.supported ? left : 0,
      hasDeviceKey: this.hasKey(serverId),
    };
  }

  /**
   * La confirmation d'un acte, dans l'ordre de l'agent. Agent ancien : rien à confirmer. Sans clé au
   * coffre : la coquille ne part pas (`not_recognized`, rien n'est noté). Acte couvert sous élévation :
   * la preuve de la clé suffit, le mot de passe n'est pas examiné. Sinon le mot de passe : absent
   * (`password_required`), faux (le premier ferme l'élévation ; au-delà de 5, l'attente), juste (ouvre
   * l'élévation si le réglage est « 5 minutes »).
   */
  confirm(
    serverId: string,
    kind: AdminActKind,
    role: Role | null,
    password: string | null,
    own: string,
  ): SimConfirmation {
    const book = this.book(serverId);
    if (!book.supported) return { kind: "ok" };
    if (!this.hasKey(serverId)) throw new LinkCommandError({ kind: "not_recognized" });
    const elevated = book.elevatedUntil > this.now();
    if (elevated && simulatedCovers(kind, role) && !password) return { kind: "ok" };
    if (!password) return { kind: "refused", refusal: { kind: "password_required" } };
    if (password !== own) {
      book.elevatedUntil = 0;
      book.wrong += 1;
      return book.wrong > FREE_TRIES
        ? { kind: "refused", refusal: { kind: "too_many_attempts", retry_after_s: WAIT_SECONDS } }
        : { kind: "refused", refusal: { kind: "wrong_password" } };
    }
    book.wrong = 0;
    book.elevatedUntil = book.mode === "window" ? this.now() + ELEVATION_SECONDS * 1000 : 0;
    return { kind: "ok" };
  }
}
