import type { AgentUpdateEvent, AgentUpdateOutcome, AgentUpdateView } from "./agent-update";
import type { AuditEntry, AuditExportResult, AuditFilter, AuditPage } from "./audit";
import type { DeviceRemovalOutcome, TrustedDevices } from "./devices";
import type { MachineEvent } from "./machine";
import type { AttackModeOutcome, SecurityRead, SecurityState } from "./security";
import type {
  AccountInputCheck,
  AccountList,
  AccountOutcome,
  FingerprintChange,
  LinkNotice,
  LinkStateEvent,
  NewServerInput,
  OperationEvent,
  ProbeResult,
  Role,
  ServerEdit,
  ServerInfo,
  Unsubscribe,
} from "./types";

/**
 * Pont entre l'interface et la liaison avec les serveurs. L'interface ne connaît que
 * cette interface : l'implémentation réelle (commandes et événements Tauri, branchée
 * dans `createLinkBridge`) et l'implémentation simulée (développement, tests) sont
 * interchangeables. Aucun accès réseau côté web (ADR-0002).
 *
 * Contrat des abonnements (calqué sur `listen` de Tauri, qui rend une promesse) :
 * - chaque `on...` rend une PROMESSE de désabonnement ;
 * - `onFingerprintChanged` rejoue les alertes d'empreinte en attente de décision, `onNotice` et
 *   `onOperation` rejouent ce qui est retenu et non acquitté : un événement n'est qu'un signal,
 *   l'état qui compte se relit à l'abonnement. Une lecture ne détruit rien ; l'écouteur est suivi
 *   d'un acquittement par identifiant, et peut recevoir deux fois le même avis (signal et lecture) :
 *   le récepteur dédoublonne ;
 * - `onServersChanged` et `onLinkState` REJOUENT l'état courant à l'abonnement (le
 *   récepteur est appelé avec la liste ou avec l'événement de chaque serveur, au plus
 *   tard avant que la promesse ne se résolve) : il n'y a donc pas de « lecture initiale »
 *   séparée, et aucune fenêtre où un ajout ou une suppression serait perdu ;
 * - un implémenteur réel s'abonne d'abord aux événements, puis envoie l'instantané ;
 * - toute commande qui échoue rejette avec une `LinkCommandError` (échec typé, sans texte) ;
 * - `LinkStateEvent.seq` croît strictement par serveur : l'interface écarte tout événement dont
 *   `seq` n'est pas supérieur au dernier connu (rejeu d'un instantané compris).
 */
export interface LinkBridge {
  /** Serveurs enregistrés : liste courante, puis à chaque ajout ou suppression. */
  onServersChanged(listener: (servers: ServerInfo[]) => void): Promise<Unsubscribe>;
  /** État du lien de chaque serveur (`link://state`) : état courant, puis chaque changement. */
  onLinkState(listener: (event: LinkStateEvent) => void): Promise<Unsubscribe>;
  /** « Réessayer maintenant » : relance une tentative immédiate (BR-RESIL-005). */
  retryNow(serverId: string): Promise<void>;
  /** Issues d'opérations incertaines (`link://operation`) ; rejoue celles d'avant l'abonnement, une fois. */
  onOperation(listener: (event: OperationEvent) => void): Promise<Unsubscribe>;
  /** Alertes d'empreinte changée (`link://fingerprint`, BR-CONN-003) ; rejoue celles en attente de décision. */
  onFingerprintChanged(listener: (change: FingerprintChange) => void): Promise<Unsubscribe>;
  /** Avis de la liaison (`link://notice`) : suivis d'actions perdus, écoute en retard ; rejoue les non lus, une fois. */
  onNotice(listener: (notice: LinkNotice) => void): Promise<Unsubscribe>;

  /**
   * Machine d'un serveur (tableau de bord) : la dernière vue connue (identité, historique de 5
   * minutes, niveaux), même hors ligne, puis chaque nouvel instantané (`view`) et chaque échantillon
   * (`metrics`, un par seconde). Le niveau d'alerte de chaque mesure arrive décidé : l'interface
   * ne connaît aucun seuil (BR-DASH-003, 004). Un échantillon plus ancien que le dernier connu est
   * ignoré par le récepteur.
   */
  onMachine(serverId: string, listener: (event: MachineEvent) => void): Promise<Unsubscribe>;

  /** Première prise de contact : l'empreinte à faire confirmer. Aucun identifiant n'est envoyé (BR-CONN-011). */
  probeServer(host: string, port: number | null): Promise<ProbeResult>;
  /**
   * Fin de l'assistant : contacte le serveur épinglé sur l'empreinte confirmée, ouvre la session,
   * et SEULEMENT si elle réussit enregistre le serveur, son empreinte et ses secrets (BR-CONN-002,
   * 004). Un échec ou un abandon ne laisse rien ; une application tuée pendant l'écriture laisse au pire un
   * serveur sans session, jamais un secret orphelin.
   */
  addAndLogin(input: NewServerInput): Promise<ServerInfo>;
  /** Ouvre la session ; `remember` : mot de passe au coffre de Windows (BR-CONN-004). */
  login(
    serverId: string,
    username: string,
    password: string,
    remember: boolean,
  ): Promise<{ role: Role }>;
  /** Déconnexion volontaire : le mot de passe mémorisé est conservé (BR-CONN-016). */
  logout(serverId: string): Promise<void>;
  /**
   * L'utilisateur accepte la nouvelle empreinte `fingerprint` (celle qu'il a sous les yeux, forme
   * complète) : refusée si ce n'est pas celle que le serveur a présentée et qui attend.
   */
  acceptFingerprint(serverId: string, fingerprint: string): Promise<void>;
  /** Modifie nom, couleur, adresse. Une autre adresse exige l'empreinte confirmée de nouveau (BR-CONN-009). */
  updateServer(serverId: string, edit: ServerEdit): Promise<ServerInfo>;
  /** Retire le serveur et efface ses secrets (BR-CONN-010). */
  removeServer(serverId: string): Promise<void>;
  /** Efface le mot de passe mémorisé ; la session en cours continue. */
  forgetCredentials(serverId: string): Promise<void>;

  // Les actions arrivent avec leurs tickets, UNE commande typée chacune : aucune commande générique
  // « envoie cette requête » (ADR-0013, ADR-0016). Comptes (HRT-13) : la méthode et le chemin sont
  // construits côté Rust ; un mot de passe ne traverse que ces paramètres (jamais un état, un
  // journal, un événement) ; l'agent est l'arbitre du rôle : un refus arrive en `refused`.

  /** Saisie en direct d'un identifiant et d'un mot de passe : la règle est celle de l'agent (`hearth-proto`), sans réseau. */
  checkAccountInput(username: string, password: string): Promise<AccountInputCheck>;
  /** Liste des comptes (une lecture : sans suivi, rien à relire au retour du lien). */
  listAccounts(serverId: string): Promise<AccountList>;
  createAccount(
    serverId: string,
    username: string,
    password: string,
    role: Role,
  ): Promise<AccountOutcome>;
  changeAccountRole(serverId: string, accountId: string, role: Role): Promise<AccountOutcome>;
  /** Un administrateur définit le mot de passe d'un AUTRE compte (la règle « sans l'identifiant » est jugée par l'agent). */
  setAccountPassword(
    serverId: string,
    accountId: string,
    password: string,
  ): Promise<AccountOutcome>;
  /**
   * Le titulaire change son mot de passe : ferme ses autres sessions, garde la courante.
   * `keepAddress` : la case « Garder ce poste reconnu » (Q15, BR-CONN-019), décochée par défaut :
   * l'adresse d'où part la demande est oubliée comme les autres.
   */
  changeOwnPassword(
    serverId: string,
    current: string,
    password: string,
    keepAddress: boolean,
  ): Promise<AccountOutcome>;
  closeAccountSessions(serverId: string, accountId: string): Promise<AccountOutcome>;
  /** `confirmation` : l'identifiant retapé quand on supprime son propre compte (BR-ACCT-012). */
  deleteAccount(
    serverId: string,
    accountId: string,
    confirmation: string | null,
  ): Promise<AccountOutcome>;

  // Postes de confiance (HRT-23) : UNE commande typée par lecture ou action. L'interface ne reçoit que
  // des noms, des dates, une adresse et des booléens : la clé de ce PC reste dans la coquille Rust.

  /** Les postes de confiance du compte de la session (une lecture, refaite au retour du lien) ; `unsupported` : agent d'avant la clé d'appareil. */
  listTrustedDevices(serverId: string): Promise<TrustedDevices>;
  /**
   * Retire un poste : un acte d'administration, le mot de passe actuel ET la preuve de la clé de CE PC
   * (la coquille signe ; le mot de passe ne traverse que ce paramètre et n'est jamais gardé). Une action
   * (clé d'opération, résultat inconnu à la coupure, jamais rejouée).
   */
  removeTrustedDevice(
    serverId: string,
    deviceId: string,
    password: string,
  ): Promise<DeviceRemovalOutcome>;

  // Sécurité (HRT-26) : UNE commande typée par lecture ou action. L'interface ne reçoit que des états,
  // des dates, des compteurs et des booléens : la clé de ce PC reste dans la coquille Rust.

  /**
   * L'état de sécurité de chaque serveur (`link://security`) : l'alerte « attaque probable » et le mode
   * attaque, rejoués à l'abonnement (le dernier état connu), puis à chaque changement. `seq` croît
   * strictement par serveur : le récepteur écarte tout état dont `seq` n'est pas supérieur au dernier
   * connu. `device` vaut `unknown` dans ces états : seule la lecture (`getSecurity`) le dit.
   */
  onSecurity(listener: (state: SecurityState) => void): Promise<Unsubscribe>;
  /** Lit l'état de sécurité (une lecture, refaite au retour du lien) ; `unsupported` : agent d'avant la fonction. */
  getSecurity(serverId: string): Promise<SecurityRead>;
  /**
   * Active (`active: true`) ou désactive le mode attaque : un acte d'administration, le mot de passe
   * actuel ET la preuve de la clé de CE PC (la coquille signe ; le mot de passe ne traverse que ce
   * paramètre et n'est jamais gardé). Rejette avec `not_recognized` sans rien envoyer si ce PC n'a pas
   * de clé, avec `forbidden` pour un compte Lecture seule. Une action (clé d'opération, résultat
   * inconnu à la coupure, jamais rejouée).
   */
  setAttackMode(serverId: string, active: boolean, password: string): Promise<AttackModeOutcome>;

  // Mise à jour de l'agent (HRT-17) : UNE commande typée par lecture ou action. L'interface ne fournit
  // NI adresse, NI signature, NI somme : la coquille tient la cible de sa propre lecture du flux de
  // versions ; `version` n'est que le numéro que l'utilisateur a sous les yeux (refusé s'il n'est plus
  // celui que la coquille retient). L'agent reste l'arbitre du rôle (refus = échec `forbidden`).

  /** L'état de la mise à jour de l'agent : version, installation gérée, en cours, dernier résultat, version disponible. Une lecture : refaite au retour du lien. */
  getAgentUpdate(serverId: string): Promise<AgentUpdateView>;
  /** Note que le résultat daté `at` a été annoncé : il ne le sera plus, même après un redémarrage du client. Ne parle pas à l'agent. */
  ackAgentResult(serverId: string, at: string): Promise<void>;
  /** « Mettre à jour l'agent » : une action (clé d'opération, résultat inconnu à la coupure, jamais rejouée). */
  updateAgent(serverId: string, version: string): Promise<AgentUpdateOutcome>;
  /** La progression de la mise à jour d'un agent (`agent-update://progress`) ; rien n'est rejoué : l'état se relit (`getAgentUpdate`). */
  onAgentUpdate(listener: (event: AgentUpdateEvent) => void): Promise<Unsubscribe>;

  /** Le serveur affiché dans la fenêtre (`null` : aucun) : l'icône de la zone de notification le suit (BR-RESIL-016). */
  setDisplayedServer(serverId: string | null): Promise<void>;

  // Journal d'activité (HRT-14) : UNE commande typée par lecture, aucune route ni aucun paramètre
  // libre (ADR-0013, 0016). Réservé aux administrateurs : un refus est `forbidden`.
  /** Une page du journal filtré, de la plus récente à la plus ancienne ; `before` : curseur (`nextBefore`). */
  readAudit(serverId: string, filter: AuditFilter, before: number | null): Promise<AuditPage>;
  /**
   * Exporte le résultat filtré en CSV : la boîte de dialogue d'enregistrement est celle du système,
   * l'utilisateur choisit le fichier ; l'interface ne désigne aucun chemin.
   */
  exportAudit(serverId: string, filter: AuditFilter): Promise<AuditExportResult>;
  /**
   * Entrées du journal reçues en direct (`link://audit`). Rien n'est rejoué : ce qui a été manqué
   * pendant une coupure se rattrape par `readAudit` (le récepteur dédoublonne par `id`). `onGap` : le
   * flux a PERDU des entrées (avis de retard de la liaison, `link://audit-gap`) : le récepteur relit.
   */
  onAudit(
    serverId: string,
    listener: (entry: AuditEntry) => void,
    onGap?: () => void,
  ): Promise<Unsubscribe>;
}
