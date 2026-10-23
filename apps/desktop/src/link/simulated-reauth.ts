import {
  type AdminActKind,
  ELEVATION_SECONDS,
  type PasswordNeed,
  type ReauthMode,
  type ReauthState,
  type TrustLevel,
} from "./reauth";
import { LinkCommandError, type Role } from "./types";

/**
 * La confirmation des actes du pont SIMULÉ (navigateur de développement, Vitest, Playwright) : un petit
 * agent qui applique les mêmes règles que le vrai (BR-TRUST-036, 040, 042, 043) et que l'appelant PILOTE
 * (agent ancien, poste sans clé, réglage, élévation ouverte ou fermée). Jamais livré (derrière
 * `import.meta.env.DEV`). Aucune clé n'existe ici : seul le fait « ce PC a une clé au coffre » est
 * simulé (c'est `keyAtHand` de la sécurité simulée). Le mot de passe n'est ni gardé ni noté.
 */

/** Faux : l'agent n'annonce pas la confirmation des actes (aucun acte ne lui part). */
interface Book {
  supported: boolean;
  required: boolean;
  mode: ReauthMode;
  /** Fin de l'élévation (millisecondes), 0 : fermée. */
  elevatedUntil: number;
  /** Mots de passe faux de suite : au-delà de 5 l'agent fait attendre. */
  wrong: number;
  /** L'état de confiance que l'agent annonce (HRT-194) ; `null` : agent d'avant, règle d'avant. */
  trust: TrustLevel | null;
}

const FREE_TRIES = 5;
const WAIT_SECONDS = 30;

/**
 * L'élévation couvre-t-elle cet acte ? Le miroir de `hearth_proto::admin_act::covered_by_elevation` pour
 * le simulateur seulement : le vrai pont demande la règle à la coquille (`reauthPasswordNeed`). C'est la
 * règle d'AVANT la grille (agent qui n'annonce pas l'état de confiance) et celle des actes hors pilotage.
 */
export function simulatedCovers(kind: AdminActKind, role: Role | null): boolean {
  switch (kind) {
    case "account_create":
    case "account_role":
      return role === "readonly";
    case "account_delete":
    case "sessions_revoke":
    // Démarrer, arrêter, redémarrer : couverts par l'élévation de cinq minutes (agent d'avant la grille).
    case "service_start":
    case "service_stop":
    case "service_restart":
      return true;
    // Annuler une opération (le déploiement qui récupère son image) : couvert par l'élévation (BR-DEPLOY-016).
    case "operation_cancel":
      return true;
    case "account_password":
    case "agent_update":
    case "attack_mode_enable":
    case "attack_mode_disable":
    case "account_password_own":
    case "reauth_setting":
    // Déployer une image n'est JAMAIS couvert par l'élévation (BR-DEPLOY-015) : le mot de passe est redemandé.
    case "service_deploy":
    // Répondre à une opération n'est jamais couvert par l'élévation seule (agent d'avant la grille).
    case "operation_decide":
      return false;
  }
}

/** Les actes de pilotage d'un service (HRT-194) : leur mot de passe suit l'état de confiance. */
function isPiloting(kind: AdminActKind): boolean {
  switch (kind) {
    case "service_start":
    case "service_stop":
    case "service_restart":
    case "operation_decide":
      return true;
    case "account_create":
    case "account_role":
    case "account_delete":
    case "sessions_revoke":
    case "account_password":
    case "agent_update":
    case "attack_mode_enable":
    case "attack_mode_disable":
    case "account_password_own":
    case "reauth_setting":
    case "service_deploy":
    case "operation_cancel":
      return false;
  }
}

/**
 * Le miroir de `hearth_proto::admin_act::password_need` pour le simulateur SEUL (BR-GRANT-017) : le vrai pont
 * demande la règle à la coquille. Pilotage : normal = rien, alerte = sauf délai ouvert, attaque ou inconnu =
 * à chaque fois. Hors pilotage ou état non annoncé (agent d'avant) : la table de l'élévation.
 */
export function simulatedPasswordNeed(
  kind: AdminActKind,
  role: Role | null,
  trust: TrustLevel | null,
): PasswordNeed {
  if (trust !== null && isPiloting(kind)) {
    switch (trust) {
      case "normal":
        return "no";
      case "alert":
        return "unless_elevated";
      case "attack":
      case "unknown":
        return "yes";
    }
  }
  return simulatedCovers(kind, role) ? "unless_elevated" : "yes";
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
      book = {
        supported: true,
        required: true,
        mode: "window",
        elevatedUntil: 0,
        wrong: 0,
        trust: "normal",
      };
      this.books.set(serverId, book);
    }
    return book;
  }

  // --- Pilotage par le test ---

  /** Faux : l'agent n'annonce pas la confirmation des actes. */
  setSupported(serverId: string, supported: boolean): void {
    this.book(serverId).supported = supported;
  }

  /**
   * L'état de confiance annoncé (HRT-194) : normal, alerte, attaque. `null` : agent d'avant la grille (règle
   * d'avant). Le mode attaque et l'alerte ferment l'élévation en cours si c'est l'attaque.
   */
  setTrust(serverId: string, trust: TrustLevel | null): void {
    const book = this.book(serverId);
    book.trust = trust;
    if (trust === "attack" || trust === "unknown") book.elevatedUntil = 0;
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
      trust: book.supported ? book.trust : null,
    };
  }

  /**
   * La confirmation d'un acte, dans l'ordre de l'agent. Agent ancien : rien à confirmer. Sans clé au
   * coffre : la coquille ne part pas (`not_recognized`, rien n'est noté). Acte de pilotage en état normal :
   * la preuve de la clé suffit, aucune élévation ne s'ouvre. Acte couvert sous élévation : idem, le mot de
   * passe n'est pas examiné. Sinon le mot de passe : absent (`password_required`), faux (le premier ferme
   * l'élévation ; au-delà de 5, l'attente), juste (ouvre l'élévation si le réglage est « 5 minutes », sauf
   * en mode attaque et sauf pour un acte qui n'en demandait pas).
   */
  confirm(
    serverId: string,
    kind: AdminActKind,
    role: Role | null,
    password: string | null,
    own: string,
  ): SimConfirmation {
    const book = this.book(serverId);
    // Comme la liaison : un agent qui n'annonce pas la confirmation ne reçoit aucun acte.
    if (!book.supported) throw new LinkCommandError({ kind: "incompatible_agent" });
    if (!this.hasKey(serverId)) throw new LinkCommandError({ kind: "not_recognized" });
    const need = simulatedPasswordNeed(kind, role, book.trust);
    // La ceinture de la liaison : un acte de pilotage que l'état annoncé (attaque) fait demander à chaque fois ne
    // part pas sans mot de passe (HRT-194) ; rien n'est envoyé à l'agent.
    if (!password && need === "yes" && book.trust !== null && isPiloting(kind)) {
      throw new LinkCommandError({ kind: "invalid_input", field: "credentials" });
    }
    const attacking = book.trust === "attack" || book.trust === "unknown";
    const elevated = !attacking && book.elevatedUntil > this.now();
    if (!password && need === "no") return { kind: "ok" };
    if (!password && need === "unless_elevated" && elevated) return { kind: "ok" };
    if (!password) return { kind: "refused", refusal: { kind: "password_required" } };
    if (password !== own) {
      book.elevatedUntil = 0;
      book.wrong += 1;
      return book.wrong > FREE_TRIES
        ? { kind: "refused", refusal: { kind: "too_many_attempts", retry_after_s: WAIT_SECONDS } }
        : { kind: "refused", refusal: { kind: "wrong_password" } };
    }
    book.wrong = 0;
    // Un mot de passe superflu (acte qui n'en demande pas) est vérifié mais n'ouvre rien.
    book.elevatedUntil =
      book.mode === "window" && need !== "no" && !attacking
        ? this.now() + ELEVATION_SECONDS * 1000
        : 0;
    return { kind: "ok" };
  }
}
