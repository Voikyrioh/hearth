import type { AgentUpdateEvent, AgentUpdateOutcome, AgentUpdateView } from "./agent-update";
import type { AuditEntry, AuditExportResult, AuditFilter, AuditPage } from "./audit";
import type { DeviceRemovalOutcome, TrustedDevices } from "./devices";
import type { LogMessage, LogReading } from "./logs";
import type { MachineEvent } from "./machine";
import type {
  AdminActKind,
  PasswordNeed,
  ReauthMode,
  ReauthSettingOutcome,
  ReauthState,
  TrustLevel,
} from "./reauth";
import type { AttackModeOutcome, SecurityRead, SecurityState } from "./security";
import type { LifecycleGesture, LifecycleOutcome } from "./serviceLifecycle";
import type { DecisionOutcome, OperationChoice, ServiceOperationEvent } from "./serviceOps";
import type { EngineInfo, EnvVariable, ServiceRights, ServicesEvent } from "./services";
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
  /**
   * Toute action d'administration (comptes, mise à jour de l'agent, mode attaque, réglage) est CONFIRMÉE
   * (HRT-30) : `adminPassword` est le mot de passe actuel de l'administrateur, `null` seulement sous
   * l'élévation de 5 minutes pour un acte couvert (jamais pour : mot de passe d'un autre, rôle
   * Administrateur, mise à jour de l'agent, mode attaque, réglage). La preuve de la clé de CE PC est
   * faite par la coquille. Sans clé au coffre l'action rejette avec `not_recognized` sans rien envoyer ;
   * un agent qui exige la confirmation et ce client trop ancien : `incompatible_client`. Un défi neuf et
   * une clé d'opération neuve à chaque appel.
   */
  createAccount(
    serverId: string,
    username: string,
    password: string,
    role: Role,
    adminPassword: string | null,
  ): Promise<AccountOutcome>;
  changeAccountRole(
    serverId: string,
    accountId: string,
    role: Role,
    adminPassword: string | null,
  ): Promise<AccountOutcome>;
  /** Un administrateur définit le mot de passe d'un AUTRE compte (la règle « sans l'identifiant » est jugée par l'agent). */
  setAccountPassword(
    serverId: string,
    accountId: string,
    password: string,
    adminPassword: string,
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
  closeAccountSessions(
    serverId: string,
    accountId: string,
    adminPassword: string | null,
  ): Promise<AccountOutcome>;
  /** `confirmation` : l'identifiant retapé quand on supprime son propre compte (BR-ACCT-012). */
  deleteAccount(
    serverId: string,
    accountId: string,
    confirmation: string | null,
    adminPassword: string | null,
  ): Promise<AccountOutcome>;

  // Confirmation des actes (HRT-30) : UNE lecture, UN calcul, UN réglage. L'interface ne devine ni la
  // capacité de l'agent ni l'élévation de 5 minutes : elle les lit de l'agent à l'ouverture d'une fenêtre.

  /** Ce que l'agent annonce de la confirmation des actes, et si ce PC a une clé au coffre (une lecture). */
  getReauthState(serverId: string): Promise<ReauthState>;
  /**
   * Que demande l'agent du mot de passe pour cet acte, dans l'état de confiance qu'il a annoncé (`trust`,
   * `null` pour un agent d'avant : règle d'avant) ? La règle est celle de l'agent (ne parle pas à l'agent).
   */
  reauthPasswordNeed(
    kind: AdminActKind,
    role: Role | null,
    trust: TrustLevel | null,
  ): Promise<PasswordNeed>;
  /**
   * Change « Demander mon mot de passe » (à chaque action, ou 5 minutes) : un acte d'administration, le
   * mot de passe actuel ET la preuve de la clé de CE PC. Jamais couvert par l'élévation.
   */
  setReauthSetting(
    serverId: string,
    mode: ReauthMode,
    password: string,
  ): Promise<ReauthSettingOutcome>;

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
  /**
   * « Mettre à jour l'agent » : une action (clé d'opération, résultat inconnu à la coupure, jamais
   * rejouée). Remplace le binaire qui tourne en root : le mot de passe est toujours demandé.
   */
  updateAgent(
    serverId: string,
    version: string,
    adminPassword: string,
  ): Promise<AgentUpdateOutcome>;
  /** La progression de la mise à jour d'un agent (`agent-update://progress`) ; rien n'est rejoué : l'état se relit (`getAgentUpdate`). */
  onAgentUpdate(listener: (event: AgentUpdateEvent) => void): Promise<Unsubscribe>;

  // Moteur de conteneurs et services (HRT-62) : LECTURE SEULE, UNE commande typée par geste. Les noms de
  // variables et la commande de lancement arrivent avec la liste (détail, HRT-63) ; une VALEUR de variable ne
  // se lit que par `revealServiceEnv`, à la demande.

  /**
   * L'état du moteur et la liste des services de chaque serveur (`link://services`) : le dernier état connu
   * (même hors ligne : la dernière liste enregistrée, périmée), puis chaque changement. La liaison fusionne
   * les lots : l'interface reçoit toujours un état COMPLET, ou « cet agent ne gère pas les services » (pas
   * de capacité `services` : ni sujet ni route envoyés à cet agent). `seq` croît strictement par serveur :
   * le récepteur écarte tout événement dont `seq` n'est pas supérieur au dernier connu.
   */
  onServices(listener: (event: ServicesEvent) => void): Promise<Unsubscribe>;
  /**
   * « Vérifier à nouveau » : relance la détection du moteur tout de suite et rend son état. Une lecture :
   * sans suivi, refaite par l'interface si le lien tombe. Hors « Connecté » : échec `not_connected`.
   */
  recheckEngine(serverId: string): Promise<EngineInfo>;
  /**
   * Le droit « gérer les services » du compte connecté (HRT-78, BR-GRANT-011) : une lecture, relue à l'ouverture de
   * la section Services, à chaque reconnexion et après tout refus pour absence du droit. Aucune méthode de ce pont
   * ne l'accorde ni ne le retire : il se donne depuis le serveur seulement (BR-GRANT-005). Hors « Connecté » :
   * échec `not_connected`.
   */
  readServiceRights(serverId: string): Promise<ServiceRights>;
  /**
   * Révèle les valeurs des variables d'UN service (HRT-63, BR-SVC-009) : une lecture réservée aux comptes qui ont le
   * droit de gérer les services, que l'agent consigne au journal (sans nom ni valeur). Hors « Connecté » : échec
   * `not_connected` ; sans le droit : `missing_services_right`. La réponse n'est ni gardée ni tracée par la coquille :
   * l'appelant la garde en mémoire le temps de l'affichage et l'efface au remasquage.
   */
  revealServiceEnv(serverId: string, key: string): Promise<EnvVariable[]>;
  /**
   * Ouvre la lecture des journaux d'UN service (HRT-67, `BR-SLOG-*`) : une lecture par onglet, qui compte dans le
   * plafond du compte (trois). Les messages arrivent à `listener`, dans l'ordre, jusqu'à la fermeture ou une fin
   * (`ended`). La lecture se reprend seule après une coupure (`interrupted` puis un nouveau `status`) ; un refus de
   * l'agent est une fin, jamais retentée. Hors « Connecté » : échec `not_connected`, rien n'est envoyé. Le texte des
   * lignes est du contenu sensible : ni gardé, ni tracé, ni écrit.
   */
  openServiceLogs(
    serverId: string,
    key: string,
    listener: (message: LogMessage) => void,
  ): Promise<LogReading>;
  /** Ouvre la documentation officielle de Docker dans le navigateur du PC (l'adresse est une constante de la coquille). */
  openEngineDocs(): Promise<void>;

  // Opérations de service (HRT-84, ADR-0045) : le suivi, uniforme pour toute opération longue. Aucune méthode
  // de ce pont ne lance ni n'annule une opération ; la seule qui agit est la décision, un acte confirmé.

  /**
   * Les opérations de service de chaque serveur (`link://service-operations`) : celles qui courent et les issues
   * pas encore acquittées (rejouées à l'abonnement, même hors ligne), puis chaque changement. Une opération qui
   * court encore au retour du lien est reconstruite par l'état tenu côté Rust, sans nouveau message ; l'issue
   * d'une opération terminée est annoncée UNE fois (BR-RESIL-010). `seq` croît strictement : le récepteur écarte
   * tout changement dont `seq` n'est pas supérieur au dernier connu pour la même opération.
   */
  onServiceOperations(listener: (event: ServiceOperationEvent) => void): Promise<Unsubscribe>;
  /** L'interface a annoncé l'issue de ces opérations : elles ne sont plus rejouées. Ne parle pas à l'agent. */
  ackServiceOperations(serverId: string, opIds: string[]): Promise<void>;
  /**
   * Décide d'une opération qui attend le compte (`BR-SVC-029`) : un acte d'administration confirmé, le mot de passe
   * est redemandé à chaque réponse (`adminPassword`, jamais gardé). Sans le droit « gérer les services » : échec
   * `missing_services_right`. Jamais rejoué : un résultat inconnu se lit sur le flux.
   */
  decideServiceOperation(
    serverId: string,
    opId: string,
    choice: OperationChoice,
    adminPassword: string | null,
  ): Promise<DecisionOutcome>;
  /**
   * Démarre, arrête ou redémarre un service (HRT-65, BR-LIFE-016) : un acte d'administration confirmé, couvert par
   * l'élévation de cinq minutes (`adminPassword` : `null` sous l'élévation, jamais gardé). L'agent répond « accepté »
   * (`202`) : l'issue ne se lit jamais ici, elle arrive par le suivi des opérations (`onServiceOperations`).
   * Sans le droit « gérer les services » : échec `missing_services_right`. Jamais rejoué : un résultat inconnu
   * (lien coupé avant la réponse) se lit sur le flux (BR-LIFE-020).
   */
  pilotService(
    serverId: string,
    key: string,
    gesture: LifecycleGesture,
    adminPassword: string | null,
  ): Promise<LifecycleOutcome>;

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
