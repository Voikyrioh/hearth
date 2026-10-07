# Business Rules — Hearth

Règles métier par domaine. Chaque fiche `BR-{DOMAINE}-{NNN}-{slug}.md` documente une règle : énoncé, code qui l'applique, vérification, cas limites. Créées par ticket lors du dev.

## Fiches

- [BR-CONN-001](./BR-CONN-001-verification-empreinte.md) — Empreinte du serveur : format 8 × 4 hexadécimaux et confirmation — `hearth-proto/src/fingerprint.rs::Fingerprint::short` — invariant ✓
- [BR-CONN-004](./BR-CONN-004-identifiants-memorises-dans-le-coffre.md) — Les identifiants ne sont mémorisés que dans le coffre de Windows, si « Se souvenir de moi » est cochée — crates/hearth-link/src/manager/mod.rs::LinkManager::login, ::forget_credentials ; coquille : apps/desktop/src-tauri/src/vault.rs::CredentialVault — invariant ✓
- [BR-CONN-005](./BR-CONN-005-connexion-automatique-au-lancement.md) — Au lancement, les serveurs dont les identifiants sont mémorisés se reconnectent seuls — crates/hearth-link/src/manager/mod.rs::initial_start — —
- [BR-CONN-008](./BR-CONN-008-nom-de-serveur-unique.md) — Nom de serveur unique (sans tenir compte de la casse), adresse et port valides — crates/hearth-link/src/domain/book.rs::check_name, check_address ; interface : apps/desktop/src/validation/server.ts — invariant ✓
- [BR-CONN-006](./BR-CONN-006-verrouillage-apres-echecs.md) — 5 échecs : attente 1 min doublée, plafond 15 min — `domain/lockout.rs::step` — invariant ✓
- [BR-CONN-007](./BR-CONN-007-tentatives-par-identifiant-et-adresse.md) — Tentatives comptées par identifiant et adresse du client — `domain/lockout.rs::AttemptKey::new` — —
- [BR-CONN-013](./BR-CONN-013-message-de-connexion-generique.md) — Refus de connexion générique, même chemin identifiant inconnu / mot de passe faux — `application/sessions.rs::SessionService::login` — invariant ✓
- [BR-CONN-018](./BR-CONN-018-ralentissement-par-identifiant.md) — Un identifiant attaqué depuis de nombreuses adresses est ralenti (2 s doublées, plafond 2 min), jamais bloqué — `domain/identifier_slowdown.rs::record_failure` — invariant ✓
- [BR-CONN-019](./BR-CONN-019-adresses-connues-d-un-compte.md) — Une adresse connue d'un compte (connexion réussie, 8 par compte, 30 jours) évite le ralentissement des autres mais ne donne aucun droit — `domain/known_address.rs::learn`, `domain/login_policy.rs::admit` — invariant ✓
- [BR-CONN-020](./BR-CONN-020-places-reservees-aux-adresses-connues.md) — Connexions en cours (32, dont 8 réservées) et attentes du flux (16, dont 4 réservées) réservées en priorité aux adresses connues — `domain/login_policy.rs::admits_login`, `domain/stream.rs::admit_pending` — invariant ✓
- [BR-CONN-014](./BR-CONN-014-incompatibilite-de-version.md) — Version d'interface incompatible : 426 et qui met à jour — `domain/compat.rs::check` — invariant ✓
- [BR-RESIL-010](./BR-RESIL-010-operation-rejouee-sans-reexecution.md) — Clé d'opération rejouée : premier résultat, sans ré-exécution — `domain/operations.rs::classify` ; client : `hearth-link domain/pending_ops.rs::PendingOps::resolve` — invariant ✓
- [BR-RESIL-012](./BR-RESIL-012-session-expiree-glissante.md) — Session glissante 30 jours, jeton stocké haché — `domain/sessions.rs::check`, `domain/session_token.rs` — invariant ✓
- [BR-RESIL-014](./BR-RESIL-014-acces-revoque.md) — Session fermée par l'administration : SESSION_REVOKED — `domain/sessions.rs::check` — invariant ✓
- [BR-CONN-002](./BR-CONN-002-empreinte-memorisee-et-verifiee.md) — L'empreinte confirmée est mémorisée et vérifiée à chaque connexion — crates/hearth-link/src/domain/pinning.rs::decide — invariant ✓
- [BR-CONN-003](./BR-CONN-003-empreinte-changee-blocage.md) — Empreinte changée : blocage, aucune requête authentifiée n'est envoyée — crates/hearth-link/src/domain/state.rs::LinkMachine::handle — invariant ✓
- [BR-CONN-011](./BR-CONN-011-aucun-identifiant-avant-confirmation.md) — Aucun identifiant n'est envoyé avant la confirmation de l'empreinte — crates/hearth-link/src/domain/pinning.rs::PinDecision::allows_credentials — invariant ✓
- [BR-RESIL-001](./BR-RESIL-001-etat-du-lien-toujours-visible.md) — L'état du lien du serveur affiché est toujours visible dans l'en-tête — crates/hearth-link/src/domain/state.rs::LinkMachine::status ; interface : apps/desktop/src/components/molecules/LinkStatePill.vue — —
- [BR-RESIL-002](./BR-RESIL-002-coupure-sous-3s-silencieuse.md) — Une coupure de moins de 3 secondes ne change rien à l'écran — crates/hearth-link/src/domain/state.rs::LinkMachine::handle — invariant ✓
- [BR-RESIL-003](./BR-RESIL-003-reconnexion-de-3-a-30-secondes.md) — De 3 à 30 secondes de coupure, l'état du lien est « Reconnexion en cours » — crates/hearth-link/src/domain/state.rs::LinkMachine::derive_down — invariant ✓
- [BR-RESIL-004](./BR-RESIL-004-bandeau-hors-ligne.md) — Au-delà de 30 secondes de coupure, l'état du lien est « Hors ligne » : bandeau avec l'heure du dernier contact et « Réessayer maintenant » — crates/hearth-link/src/domain/state.rs::LinkMachine::derive_down ; interface : apps/desktop/src/components/organisms/OfflineBanner.vue — invariant ✓
- [BR-RESIL-005](./BR-RESIL-005-reconnexion-automatique-et-reessayer.md) — Reconnexion automatique sans fin, délais de 0,5 s à 30 s, et « Réessayer maintenant » relance tout de suite — crates/hearth-link/src/domain/backoff.rs::Backoff::{next_delay, reset} ; interface : apps/desktop/src/stores/link.ts::retryNow — invariant ✓
- [BR-RESIL-006](./BR-RESIL-006-reconnexion-immediate-reveil-reseau.md) — Au réveil du PC ou au changement de réseau, le client retente tout de suite — crates/hearth-link/src/domain/triggers.rs::{detect_wake, network_changed} — invariant ✓
- [BR-RESIL-007](./BR-RESIL-007-donnees-perimees-datees.md) — Hors « Connecté », les dernières données restent affichées, désaturées et datées — crates/hearth-link/src/domain/state.rs::LinkMachine::status ; interface : apps/desktop/src/components/molecules/StaleSurface.vue — —
- [BR-RESIL-008](./BR-RESIL-008-actions-desactivees-expliquees.md) — Hors « Connecté », les actions qui exigent le serveur sont désactivées et expliquées — bibliothèque : LinkError::NotConnected ; interface : apps/desktop/src/composables/useNeedsLink.ts::useNeedsLink — —
- [BR-RESIL-009](./BR-RESIL-009-action-incertaine-jamais-rejouee.md) — Une action coupée avant sa réponse est « résultat inconnu » et n'est jamais rejouée — crates/hearth-link/src/domain/pending_ops.rs::PendingOps::{register, complete, link_lost} — invariant ✓
- [BR-RESIL-011](./BR-RESIL-011-notifications-jamais-bloquantes.md) — Aucune fenêtre bloquante pour une perte de lien ou une erreur ; notifications discrètes — bibliothèque : états et événements typés, jamais d'erreur bloquante ; interface : apps/desktop/src/stores/toasts.ts — —
- [BR-RESIL-013](./BR-RESIL-013-reconnexion-silencieuse-apres-expiration.md) — Session expirée et mot de passe mémorisé : reconnexion sans ressaisie — crates/hearth-link/src/domain/state.rs::LinkMachine::on_session_expired — invariant ✓
- [BR-RESIL-015](./BR-RESIL-015-notifications-systeme-limitees.md) — Notifications système optionnelles, une par minute et par serveur — active — —
- [BR-RESIL-016](./BR-RESIL-016-icone-zone-de-notification.md) — L'icône de la zone de notification reflète l'état du lien — active — —
- [BR-RESIL-017](./BR-RESIL-017-session-longue-hors-ligne-sans-croissance.md) — Une session de plusieurs jours hors ligne ne fait grossir ni la bibliothèque ni l'interface — crates/hearth-link/src/domain/server.rs::HISTORY_CAP ; interface : apps/desktop/src/composables/useNow.ts — invariant ✓
- [BR-RESIL-018](./BR-RESIL-018-notifications-agregees-par-compteur.md) — Coupures répétées : une même notification répétée devient un compteur — bibliothèque : Status::failed_attempts ; interface : apps/desktop/src/stores/toasts.ts::push — —
- [BR-RESIL-019](./BR-RESIL-019-comportement-identique-tous-roles.md) — Le comportement du lien ne dépend pas du rôle du compte — crates/hearth-link/src/domain/state.rs::Input — invariant ✓
- [BR-RESIL-020](./BR-RESIL-020-etat-du-lien-par-serveur.md) — Chaque serveur enregistré a son propre état de lien, indépendant des autres — crates/hearth-link/src/manager/mod.rs::LinkManager::{state, states} ; interface : apps/desktop/src/stores/link.ts — invariant ✓
- [BR-CONN-009](./BR-CONN-009-changement-d-adresse-nouvelle-verification.md) — Modifier l'adresse d'un serveur enregistré impose une nouvelle vérification de l'empreinte — crates/hearth-link/src/manager/mod.rs::LinkManager::update_server, domain/book.rs::address_changed ; interface : apps/desktop/src/components/organisms/ServerEditForm.vue — invariant ✓
- [BR-CONN-010](./BR-CONN-010-suppression-efface-les-secrets.md) — Supprimer un serveur efface aussi ses identifiants mémorisés du coffre — crates/hearth-link/src/manager/mod.rs::LinkManager::remove_server — invariant ✓
- [BR-CONN-012](./BR-CONN-012-aucun-identifiant-hors-agent-hearth.md) — Aucun identifiant n'est envoyé si le serveur n'est pas un agent Hearth — crates/hearth-link/src/domain/agent_identity.rs::check_product — invariant ✓
- [BR-CONN-015](./BR-CONN-015-plusieurs-serveurs-simultanes.md) — Plusieurs serveurs peuvent être connectés en même temps ; basculer ne ferme rien — crates/hearth-link/src/manager/mod.rs::LinkManager::{states, servers} — invariant ✓
- [BR-CONN-016](./BR-CONN-016-deconnexion-garde-le-mot-de-passe-memorise.md) — La déconnexion ferme la session et efface le jeton, pas le mot de passe mémorisé — crates/hearth-link/src/domain/state.rs::LinkMachine — invariant ✓
- [BR-CONN-017](./BR-CONN-017-mot-de-passe-memorise-refuse.md) — Mot de passe mémorisé devenu invalide : retour au formulaire, pas « Accès révoqué » — crates/hearth-link/src/domain/state.rs::LinkMachine — invariant ✓
- [BR-INSTALL-004](./BR-INSTALL-004-empreinte-generee-une-fois.md) — Empreinte générée une seule fois, jamais modifiée — `domain/identity_policy.rs::decide` — invariant ✓
- [BR-INSTALL-001](./BR-INSTALL-001-droits-administration.md) — Seuls les administrateurs de la machine installent — `domain/install/prerequisites.rs::check_rights` — invariant ✓
- [BR-INSTALL-002](./BR-INSTALL-002-premier-compte-obligatoire.md) — Une première installation crée le premier compte administrateur — `domain/install/plan.rs::plan_install` — invariant ✓
- [BR-INSTALL-003](./BR-INSTALL-003-reinstallation-conserve-les-donnees.md) — Réinstallation : comptes, journal, empreinte et configuration conservés — `domain/install/plan.rs::plan_install` — invariant ✓
- [BR-INSTALL-005](./BR-INSTALL-005-service-au-demarrage.md) — Le service démarre avec le système (systemd, ou installation gérée) — `infrastructure/service/systemd.rs::render_unit` — invariant ✓
- [BR-INSTALL-006](./BR-INSTALL-006-prerequis-avant-toute-ecriture.md) — Un prérequis manquant arrête l'installation sans rien écrire — `domain/install/prerequisites.rs::check_prerequisites` — invariant ✓
- [BR-INSTALL-007](./BR-INSTALL-007-reinstallation-meme-version-sans-interruption.md) — Même version : le service n'est pas interrompu inutilement — `domain/install/plan.rs::plan_install` — invariant ✓
- [BR-INSTALL-008](./BR-INSTALL-008-interruption-laisse-la-machine-comme-avant.md) — Erreur ou interruption : la machine revient à l'état d'avant — `domain/install/rollback.rs::undo_plan` — invariant ✓
- [BR-INSTALL-009](./BR-INSTALL-009-format-identifiant-premier-compte.md) — Identifiant du premier compte : 3 à 32 caractères, minuscules, chiffres, tiret, underscore — `domain/install/credentials.rs::parse_admin_name` — invariant ✓
- [BR-INSTALL-010](./BR-INSTALL-010-format-mot-de-passe-premier-compte.md) — Mot de passe du premier compte : 12 caractères, majuscule, minuscule, chiffre — `domain/install/credentials.rs::check_admin_password` — invariant ✓
- [BR-INSTALL-011](./BR-INSTALL-011-desinstallation-conserver-ou-supprimer.md) — Désinstallation : conserver ou supprimer comptes, journal et configuration — `domain/install/uninstall.rs::uninstall_plan` — invariant ✓
- [BR-INSTALL-012](./BR-INSTALL-012-architecture-prise-en-charge.md) — Architectures prises en charge : x86_64 et arm64 — `domain/install/platform.rs::parse_arch` — invariant ✓
- [BR-ACCT-001](./BR-ACCT-001-creation-compte.md) — Un compte est créé avec un identifiant unique, un mot de passe et un rôle — `application/accounts.rs::AccountService::create` — —
- [BR-ACCT-002](./BR-ACCT-002-format-identifiant.md) — Identifiant : 3 à 32 caractères, minuscules, chiffres, tiret, underscore — `hearth-proto/account_rules.rs::check_username` (via `domain/accounts/username.rs::Username::parse`) — ✓
- [BR-ACCT-003](./BR-ACCT-003-unicite-insensible-casse.md) — Identifiant unique, insensible à la casse — `hearth-proto/account_rules.rs::check_username` (via `domain/accounts/username.rs::Username::parse`) — ✓
- [BR-ACCT-004](./BR-ACCT-004-complexite-mot-de-passe.md) — Mot de passe : 12 caractères, majuscule, minuscule, chiffre — `hearth-proto/account_rules.rs::unmet_password_rules` (via `domain/accounts/password.rs::unmet_rules`) — ✓
- [BR-ACCT-005](./BR-ACCT-005-mot-de-passe-sans-identifiant.md) — Le mot de passe ne contient pas l'identifiant — `hearth-proto/account_rules.rs::unmet_password_rules` (via `domain/accounts/password.rs::unmet_rules`) — ✓
- [BR-ACCT-006](./BR-ACCT-006-mot-de-passe-jamais-affiche.md) — Aucun mot de passe existant n'est affiché ni récupérable — `domain/secret.rs::Secret` — ✓
- [BR-ACCT-007](./BR-ACCT-007-dernier-administrateur.md) — Il reste toujours au moins un administrateur — `domain/accounts/admin_guard.rs::check_removal`, `::check_role_change` — ✓
- [BR-ACCT-008](./BR-ACCT-008-mot-de-passe-autrui-ferme-sessions.md) — Changer le mot de passe d'autrui ferme ses sessions — `domain/sessions.rs::closure_on_password_change` — ✓
- [BR-ACCT-009](./BR-ACCT-009-mot-de-passe-propre-ferme-autres-sessions.md) — Changer son mot de passe ferme les autres sessions — `domain/sessions.rs::closure_on_password_change` — ✓
- [BR-ACCT-010](./BR-ACCT-010-suppression-ferme-sessions.md) — Supprimer un compte ferme ses sessions — `domain/sessions.rs::closure_on_account_deletion` — ✓
- [BR-ACCT-011](./BR-ACCT-011-revocation-sessions.md) — Révoquer les sessions sans changer le mot de passe — `domain/sessions.rs::closure_on_revocation` — ✓
- [BR-ACCT-012](./BR-ACCT-012-confirmation-suppression-propre-compte.md) — Supprimer son propre compte : retaper son identifiant — `domain/accounts/self_deletion.rs::confirm_self_deletion` — ✓
- [BR-ACCT-013](./BR-ACCT-013-lecture-seule-sans-gestion-comptes.md) — Lecture seule : pas d'accès à la gestion des comptes — `domain/accounts/role.rs::Role::can_manage_accounts`, `entrypoint/http/auth.rs::guard` — ✓
- [BR-ACCT-014](./BR-ACCT-014-serveur-refuse-gestion-lecture-seule.md) — Le serveur refuse la gestion de comptes à un compte lecture seule  — `domain/accounts/role.rs::Role::can_manage_accounts` — ✓
- [BR-ACCT-015](./BR-ACCT-015-gestion-en-ligne-de-commande.md) — Gestion de comptes en ligne de commande sur le serveur — `entrypoint/account.rs::execute` — —
- [BR-ACCT-016](./BR-ACCT-016-changements-consignes-au-journal.md) — Changements de compte consignés au journal — `application/accounts.rs::AccountService`, `domain/audit/policy.rs::is_journaled` — ✓
- [BR-AUDIT-001](./BR-AUDIT-001-controle-acces-journal.md) — Seul un administrateur lit le journal — `domain/audit/policy.rs::can_read_journal` — invariant ✓
- [BR-AUDIT-002](./BR-AUDIT-002-structure-entree.md) — Chaque entrée dit quand, qui, d'où, quoi, sur quoi et avec quel résultat — `domain/audit/event.rs::{AuditEvent, Actor, Origin, Target, Outcome, Reason, AuditRecord}` — —
- [BR-AUDIT-003](./BR-AUDIT-003-evenements-journalises.md) — Connexions, refus, déconnexions, modifications et refus faute de droits sont journalisés — `domain/audit/policy.rs::is_journaled` — —
- [BR-AUDIT-004](./BR-AUDIT-004-consultations-non-journalisees.md) — Les consultations ne sont pas journalisées — `domain/audit/policy.rs::is_journaled` — —
- [BR-AUDIT-005](./BR-AUDIT-005-aucun-secret.md) — Aucun secret n'entre dans le journal — `domain/audit/event.rs` — invariant ✓
- [BR-AUDIT-006](./BR-AUDIT-006-confidentialite-existence-compte.md) — Un échec de connexion ne dit pas si le compte existe — `domain/audit/event.rs::Reason::{InvalidCredentials, InvalidIdentifier}` — invariant ✓
- [BR-AUDIT-007](./BR-AUDIT-007-blocage-temporaire.md) — Le blocage temporaire d'un compte est journalisé — `application/sessions.rs::SessionService::login_in_turn` — —
- [BR-AUDIT-008](./BR-AUDIT-008-conservation-journal.md) — Le journal est conservé 90 jours ou 50 000 entrées — `domain/audit/policy.rs::{RETENTION, MAX_ENTRIES, retention_cutoff, excess_entries}` — invariant ✓
- [BR-AUDIT-009](./BR-AUDIT-009-immutabilite-journal.md) — Le journal ne se modifie ni ne se vide depuis le client — `migrations/0003_audit_events.sql` — invariant ✓
- [BR-AUDIT-010](./BR-AUDIT-010-affichage-temps-reel.md) — Les nouvelles entrées arrivent en direct — `application/ports/audit_sink.rs::AuditFeed` ; interface : apps/desktop/src/stores/audit.ts::useAuditStore — —
- [BR-AUDIT-011](./BR-AUDIT-011-donnees-perimees.md) — Données périmées quand le lien est coupé — interface : apps/desktop/src/stores/audit.ts::{resume, catchUp} — —
- [BR-AUDIT-012](./BR-AUDIT-012-fuseau-horaire-client.md) — L'heure s'affiche dans le fuseau du PC client — `entrypoint/http/wire.rs::date` ; interface : apps/desktop/src/audit/format.ts::formatWhen — —
- [BR-AUDIT-013](./BR-AUDIT-013-regroupement-rafales.md) — Les refus en rafale sont regroupés à l'affichage — interface : apps/desktop/src/audit/grouping.ts::groupBursts — —
- [BR-AUDIT-014](./BR-AUDIT-014-gestion-filtres.md) — Les filtres appliqués restent visibles et se réinitialisent en un clic — `domain/audit/filter.rs::{AuditFilter, RawFilter}` ; interface : apps/desktop/src/audit/filters.ts — —
- [BR-AUDIT-015](./BR-AUDIT-015-filtres-multi-valeurs-periode.md) — Compte, action et résultat acceptent plusieurs valeurs ; une période à la fois — `domain/audit/filter.rs::AuditFilter::new` ; crates/hearth-link/src/domain/audit_query.rs::AuditFilter::plan — —
- [BR-AUDIT-016](./BR-AUDIT-016-recherche-plein-texte.md) — La recherche plein texte porte sur tous les champs visibles — `domain/audit/filter.rs::SearchQuery::parse` ; crates/hearth-link/src/domain/audit_query.rs::AuditFilter::new — —
- [BR-AUDIT-017](./BR-AUDIT-017-export-resultat-filtre.md) — L'export porte sur le résultat filtré — `domain/audit/csv.rs::{render, field}` ; crates/hearth-proto/src/api/audit_csv.rs::field — —
- [BR-AUDIT-018](./BR-AUDIT-018-resultat-vide.md) — Aucun événement : message et bouton d'effacement — `application/audit.rs::AuditService::search` ; interface : apps/desktop/src/pages/Audit.vue — —
- [BR-AUDIT-019](./BR-AUDIT-019-indicateur-conservation.md) — L'indicateur de conservation est toujours visible — `domain/audit/policy.rs::{RETENTION, MAX_ENTRIES}` ; interface : apps/desktop/src/pages/Audit.vue — —
- [BR-AUDIT-020](./BR-AUDIT-020-rechargement-manuel.md) — Rechargement manuel quand le lien est coupé — interface : apps/desktop/src/stores/audit.ts::reloadManually — —
- [BR-AUDIT-021](./BR-AUDIT-021-journalisation-refus-acces.md) — Un refus d'accès au journal est lui-même journalisé — `entrypoint/http/mod.rs::ENDPOINTS` — invariant ✓
- [BR-DASH-001](./BR-DASH-001-affichage-initial-complet.md) — Le tableau de bord affiche l'état complet de la machine dès sa première ouverture — `application/metrics.rs::MetricsService::{identity, history}, entrypoint/ws/connection.rs` — invariant ✓
- [BR-DASH-002](./BR-DASH-002-rafraichissement-chaque-seconde.md) — Les mesures sont rafraîchies automatiquement chaque seconde — `entrypoint/tasks.rs::spawn_sampler, application/metrics.rs::MetricsService::sample_once` — invariant ✓
- [BR-DASH-003](./BR-DASH-003-trois-niveaux-d-alerte.md) — Un état d'alerte comporte trois niveaux : normal, attention, critique — `hearth-proto/src/thresholds.rs::{percent_level, usage_level, temperature_level}` — invariant ✓
- [BR-DASH-004](./BR-DASH-004-hysteresis-processeur.md) — La charge du processeur doit tenir un niveau 30 secondes avant d'alerter — `hearth-proto/src/thresholds.rs::cpu_level` — invariant ✓
- [BR-DASH-005](./BR-DASH-005-machine-sans-carte-graphique.md) — Une machine sans carte graphique affiche la section avec « Non disponible sur cette machine » — `domain/machine.rs::MachineIdentity::capabilities, application/metrics.rs::MetricsService::identity` — invariant ✓
- [BR-DASH-006](./BR-DASH-006-machine-sans-sondes.md) — Une machine sans sonde de température affiche un texte explicatif — `domain/machine.rs::MachineIdentity::capabilities` — invariant ✓
- [BR-DASH-007](./BR-DASH-007-gpu-sans-temperature.md) — Une carte graphique sans mesure de température affiche « Non disponible » — `infrastructure/system/gpu/nvidia_smi.rs::parse_line, infrastructure/system/gpu/sysfs.rs::read_card` — invariant ✓
- [BR-DASH-008](./BR-DASH-008-mesure-indisponible.md) — Une mesure momentanément indisponible n'affecte pas les autres — `application/metrics.rs::MetricsService::sample_once, domain/metrics.rs::resample` — invariant ✓
- [BR-DASH-009](./BR-DASH-009-lien-rompu-donnees-perimees.md) — Lien rompu : dernières valeurs grisées avec leur âge — `hearth-proto/src/api/metrics.rs::Sample` — invariant ✓
- [BR-DASH-010](./BR-DASH-010-courbes-fenetres.md) — Courbes : 5 minutes par défaut, bascule 1 minute, 5 minutes, 1 heure — `domain/metrics.rs::{RING_CAPACITY, Ring, HistoryWindow, resample}, application/metrics.rs::MetricsService::history` — invariant ✓
- [BR-DASH-011](./BR-DASH-011-reprise-apres-reconnexion.md) — Après une reconnexion, les valeurs reprennent en direct sans déformer les courbes — `entrypoint/ws/connection.rs, domain/stream.rs::is_new` — invariant ✓
- [BR-DASH-012](./BR-DASH-012-disques-montes-demontes.md) — Un disque monté ou retiré apparaît ou disparaît automatiquement — `domain/machine.rs::{is_real_filesystem, visible_volumes}, infrastructure/system/sysinfo_probe.rs` — invariant ✓
- [BR-DASH-013](./BR-DASH-013-deux-roles-une-vue.md) — Les deux rôles voient le tableau de bord à l'identique — `entrypoint/http/mod.rs::ENDPOINTS, entrypoint/ws/connection.rs` — invariant ✓
- [BR-DASH-014](./BR-DASH-014-formats-d-unites.md) — Les nombres suivent les unités : pourcentages entiers, Go à une décimale, débit adaptatif, durée longue — `hearth-proto/src/api/metrics.rs` — —
- [BR-DASH-015](./BR-DASH-015-interfaces-reseau-comptees.md) — Seules les interfaces réseau physiques comptent dans le débit — `domain/machine.rs::throughput_interfaces` — invariant ✓

### UPDATE (client, HRT-16)

- [BR-UPDATE-001](./BR-UPDATE-001-verification-au-lancement-puis-une-fois-par-jour.md) — Le client vérifie au lancement, puis une fois par jour au plus (une tentative qui échoue compte) — `update/domain.rs::check_is_due`, `update/service.rs::UpdateService::check` — invariant ✓
- [BR-UPDATE-002](./BR-UPDATE-002-aucune-installation-sans-consentement.md) — Aucune mise à jour du client n'est installée sans le clic de l'utilisateur — `update/service.rs::UpdateService::begin_install` — invariant ✓
- [BR-UPDATE-003](./BR-UPDATE-003-bandeau-nouvelle-version-avec-notes.md) — Une version plus récente est annoncée par un bandeau discret avec les notes (texte brut) — `update/domain.rs::{validate_candidate, banner_visible}` — invariant —
- [BR-UPDATE-004](./BR-UPDATE-004-telechargement-signature-installation-relance.md) — Téléchargement en mémoire, signature minisign vérifiée avant toute écriture, installation et relance — `update/feed.rs::TauriFeed::{download, install}` — invariant ✓
- [BR-UPDATE-005](./BR-UPDATE-005-serveurs-et-reglages-conserves.md) — Serveurs et réglages conservés après la mise à jour (à vérifier sur une vraie publication) — `update/domain.rs::forget_installed` — invariant —
- [BR-UPDATE-006](./BR-UPDATE-006-plus-tard-masque-le-bandeau-24-h.md) — « Plus tard » masque le bandeau 24 h, report enregistré côté Rust — `update/domain.rs::{postponed_until, is_postponed}` — invariant ✓
- [BR-UPDATE-007](./BR-UPDATE-007-sans-internet-echec-silencieux.md) — Sans Internet : échec silencieux, « Dernière vérification » dans les réglages — `update/service.rs::UpdateService::absorb` — invariant —
- [BR-UPDATE-008](./BR-UPDATE-008-service-de-versions-muet-abandon-silencieux.md) — Service de versions muet : abandon silencieux, retenté plus tard — `update/service.rs::UpdateService::absorb` — invariant —
- [BR-UPDATE-009](./BR-UPDATE-009-telechargement-interrompu-relancable.md) — Téléchargement interrompu : relançable, la version en cours reste utilisable — `update/feed.rs::classify` — invariant —
- [BR-UPDATE-010](./BR-UPDATE-010-mise-a-jour-corrompue-refusee.md) — Mise à jour corrompue : refusée, la version en cours reste utilisable — `update/feed.rs::classify` — invariant ✓
- [BR-UPDATE-025](./BR-UPDATE-025-versions-dans-les-reglages.md) — Les réglages affichent la version du client et celle de l'agent de chaque serveur — `pages/Settings.vue`, `UpdatePanel.vue`, `AgentUpdateCard.vue` — invariant —
- [BR-UPDATE-026](./BR-UPDATE-026-verifier-maintenant.md) — « Vérifier maintenant » force une vérification, hors de la règle des 24 h — `update/service.rs::UpdateService::check_now` — invariant —

### UPDATE (agent, HRT-17)

Côté interface du client (HRT-17, lot interface) :

- [BR-UPDATE-020](./BR-UPDATE-020-versions-incompatibles-quel-cote-mettre-a-jour.md) — Versions incompatibles : le message dit lequel mettre à jour, avec le bouton qui convient (client seulement) — `OfflineBanner.vue`, `AgentUpdateCard.vue` — invariant —
- [BR-UPDATE-021](./BR-UPDATE-021-versions-incompatibles-lecture-seule.md) — Versions incompatibles et compte Lecture seule : demander à un administrateur — `OfflineBanner.vue`, `AgentUpdateCard.vue` — invariant —
- [BR-UPDATE-022](./BR-UPDATE-022-version-de-l-agent-et-version-disponible.md) — Version de l'agent et version disponible (jamais de rétrogradation) — `agent_update/domain.rs::is_newer`, `agent_update/service.rs::view` — invariant —
- [BR-UPDATE-023](./BR-UPDATE-023-mention-mise-a-jour-disponible-dans-la-liste.md) — « Mise à jour disponible » seulement sur le serveur concerné, dans la liste — `ServerRow.vue`, `stores/agentUpdates.ts::withUpdate` — invariant —

Côté agent :

- [BR-UPDATE-011](./BR-UPDATE-011-seul-admin-met-a-jour-agent.md) — Seul un administrateur déclenche la mise à jour de l'agent — `entrypoint/http/mod.rs::ENDPOINTS` — invariant ✓
- [BR-UPDATE-012](./BR-UPDATE-012-une-seule-mise-a-jour-a-la-fois.md) — Une seule mise à jour de l'agent à la fois — `domain/update/target.rs::plan_update` — invariant ✓
- [BR-UPDATE-013](./BR-UPDATE-013-avancement-visible-etape-par-etape.md) — Une mise à jour de l'agent est visible étape par étape — `domain/update/progress.rs::PercentTracker` — invariant ✓
- [BR-UPDATE-014](./BR-UPDATE-014-etape-redemarrage-annoncee.md) — L'étape « redémarrage » est annoncée avant l'arrêt de l'agent — `application/update.rs::UpdateService::{execute, progress, resume}` — invariant ✗
- [BR-UPDATE-015](./BR-UPDATE-015-retour-automatique-si-le-nouvel-agent-ne-repond-pas.md) — Retour automatique si le nouvel agent ne répond pas dans les 60 secondes — `domain/update/supervise.rs::check_verdict` — invariant ✓
- [BR-UPDATE-016](./BR-UPDATE-016-pas-de-relance-apres-une-mise-a-jour-annulee.md) — Une mise à jour annulée n'est pas relancée automatiquement — `application/update.rs::UpdateService::{resume, report_pending, report}` — invariant ✗
- [BR-UPDATE-017](./BR-UPDATE-017-la-mise-a-jour-continue-malgre-la-coupure.md) — La mise à jour continue côté serveur malgré une coupure réseau — `application/update.rs::UpdateService::{start, last, status}` — invariant ✓
- [BR-UPDATE-018](./BR-UPDATE-018-comptes-journal-empreinte-survivent.md) — Comptes, journal et empreinte survivent à la mise à jour — `application/update_supervisor.rs::Supervisor::{run, ask}` — invariant ✓
- [BR-UPDATE-019](./BR-UPDATE-019-serveur-sans-internet.md) — Serveur sans accès à Internet : l'agent actuel continue de fonctionner — `infrastructure/update/download.rs::{HttpsDownloader::fetch, map_error}` — invariant ✗
- [BR-UPDATE-024](./BR-UPDATE-024-refus-de-mise-a-jour-consignes-au-journal.md) — Les refus de mise à jour sont consignés au journal d'activité — `entrypoint/http/auth.rs::{guard, failure_of}` — invariant ✗

- [BR-UPDATE-027](./BR-UPDATE-027-adresse-de-telechargement-publique-et-https.md) — Le serveur ne télécharge qu'en HTTPS, depuis une adresse publique — `domain/update/target.rs::{host_is_local, is_local_address, plan_update}` — invariant ✓
- [BR-UPDATE-028](./BR-UPDATE-028-travail-de-mise-a-jour-laisse-en-cours.md) — Un travail de mise à jour laissé en cours est conclu au démarrage — `domain/update/orphan.rs::classify_orphan` — invariant ✓
- [BR-UPDATE-029](./BR-UPDATE-029-retour-arriere-ne-laisse-pas-une-base-migree.md) — Un retour arrière ne laisse jamais un ancien binaire devant une base déjà migrée — `domain/update/space.rs::check_space`, `domain/update/resume.rs::{Phase, binary_restored}`, `application/update_supervisor.rs::Supervisor::{run, drive}` — invariant ✓

- [BR-UPDATE-030](./BR-UPDATE-030-un-superviseur-tombe-en-echec-est-relance.md) — Un superviseur tombé en échec est relancé (`Restart=on-failure`, pas de minuterie) et reprend où il s'est arrêté — `infrastructure/update/host.rs::systemd_run_arguments`, `app/update.rs::run_supervisor` — invariant ✓
- [BR-UPDATE-031](./BR-UPDATE-031-un-arret-voulu-du-service-n-est-jamais-defait.md) — Un arrêt voulu du service n'est jamais défait par la mise à jour — `application/update_supervisor.rs::Supervisor::{check, pause}`, `infrastructure/service/systemd.rs::Systemd::state` — invariant ✓
- [BR-UPDATE-032](./BR-UPDATE-032-le-superviseur-rejoue-ne-refait-aucune-etape.md) — Un superviseur rejoué ne refait aucune étape (marqueur durable, issues sûres) — `domain/update/resume.rs`, `application/update_supervisor.rs::Supervisor::drive` — invariant ✓
- [BR-UPDATE-033](./BR-UPDATE-033-les-reprises-sont-bornees.md) — Les reprises sont bornées (3), puis abandon journalisé une fois, copies gardées — `domain/update/resume.rs::{MAX_RESUMES, enter}`, `application/update_supervisor.rs::Supervisor::abandon` — invariant ✓
- [BR-UPDATE-034](./BR-UPDATE-034-machine-redemarree-pendant-une-mise-a-jour.md) — Une machine redémarrée pendant une mise à jour : l'agent qui démarre reprend le marqueur — `domain/update/resume.rs::resume_phase`, `application/update.rs::UpdateService::{resume, recover}` — invariant ✓

Les règles BR-UPDATE-020 à 023 concernent l'écran de mise à jour de l'agent côté client (HRT-17, lot interface). Les règles 027 à 029 sont nées de la revue de HRT-17 (adresse de téléchargement, travail orphelin, base migrée) ; 030 à 034 de HRT-27 (superviseur relancé et rejouable, arrêt voulu, reprises bornées, machine redémarrée).

| Domaine | Rôle | Nombre fiches | Référence conception |
|---|---|---|---|
| **INSTALL** | Installation agent : déploiement, service système, génération certificat | 12 | us-installer-agent |
| **CLIENT** | Installation et configuration client desktop : NSIS, Tauri, démarrage, zone notification | 14 | us-installer-client |
| **CONN** | Connexion au serveur : découverte, épinglage TLS, authentification, session, coffre | 17 | us-connecter-serveur |
| **DASH** | Tableau de bord machine : mesures système, GPU, historique, seuils | 15 | us-tableau-de-bord-machine |
| **RESIL** | Résilience du lien : reconnexion, opérations idempotentes, instantanés, affichage état | 20 | us-lien-resilient |
| **ACCT** | Gestion comptes : création, suppression, rôles, mots de passe, dernier administrateur | 16 | us-gerer-comptes |
| **AUDIT** | Journal d'activité : logging, filtrage, recherche FTS5, purge, export CSV | 21 | us-journal-activite |
| **UPDATE** | Mises à jour agent et client : flux de versions, signatures minisign, superviseur, rollback | 29 (28 livrées et 020 à moitié, tenue pour le client seulement : côté client 001 à 010, 021 à 023, 025, 026 ; côté agent 011 à 019, 024, 027 à 029) | us-mises-a-jour |

## Comment documenter une règle

Fichier `BR-{DOMAINE}-{NNN}-{slug-titre}.md` :

```markdown
---
id: BR-CONN-007
titre: Épinglage du certificat à la première connexion
domaine: CONN
story: us-connecter-serveur
---

# BR-CONN-007 — Épinglage du certificat à la première connexion

## Acteur
Utilisateur installant le client.

## Condition
Première connexion vers un agent inconnu (pas dans `servers.json`).

## Action
1. Client lance `hearth-link::probe()` sans confiance TLS.
2. Reçoit réponse `/hello` du serveur.
3. Calcule SHA-256 empreinte du certificat.
4. Affiche empreinte en 8 groupes 4 hex majuscules.
5. Demande confirmation utilisateur.

## Résultat
- **Confirmé** : empreinte ajoutée à `servers.json` + coffre token ; prochaines connexions vérifient l'empreinte.
- **Rejeté** : abandon, serveur non ajouté.

## Exception
- Empreinte change (certificat regénéré) : alerte bloquante, user doit supprimer et ré-ajouter le serveur.
- Proxy TLS interceptant : user accepte probe via proxy (même résultat).
```

## Fiches du client Windows (BR-CLIENT)

- [BR-CLIENT-001](./BR-CLIENT-001-installation-sans-droits-administrateur.md) — L'installation se déroule sans droits administrateur ni logiciel tiers — apps/desktop/src-tauri/tauri.conf.json — invariant ✓
- [BR-CLIENT-002](./BR-CLIENT-002-installation-par-utilisateur.md) — L'installation se fait pour le compte Windows courant seulement — apps/desktop/src-tauri/tauri.conf.json — invariant ✓
- [BR-CLIENT-003](./BR-CLIENT-003-instance-unique.md) — Une seule instance du client à la fois ; relancer ramène la fenêtre au premier plan — apps/desktop/src-tauri/src/lib.rs::run — invariant ✓
- [BR-CLIENT-004](./BR-CLIENT-004-fermer-reduit-dans-la-zone-de-notification.md) — Fermer la fenêtre la réduit dans la zone de notification — apps/desktop/src-tauri/src/domain.rs::hides_on_close — invariant ✓
- [BR-CLIENT-005](./BR-CLIENT-005-explication-de-fermeture-une-seule-fois.md) — L'explication de la réduction n'est donnée qu'à la première fermeture — apps/desktop/src-tauri/src/domain.rs::should_explain_close — —
- [BR-CLIENT-006](./BR-CLIENT-006-demarrage-avec-windows-desactive-par-defaut.md) — Le lancement au démarrage de Windows est proposé à l'installation, désactivé par défaut (HRT-21 : case dans l'installateur) — apps/desktop/src-tauri/src/settings.rs::read — —
- [BR-CLIENT-007](./BR-CLIENT-007-demarrage-avec-windows-reglable.md) — Le lancement au démarrage se règle depuis l'application — apps/desktop/src-tauri/src/settings.rs::set_launch_at_startup — —
- [BR-CLIENT-008](./BR-CLIENT-008-reinstallation-conserve-les-donnees.md) — Réinstaller par-dessus une version existante conserve serveurs et réglages — domain/ — invariant ✓
- [BR-CLIENT-009](./BR-CLIENT-009-desinstallation-garder-ou-tout-effacer.md) — La désinstallation propose de garder ou d'effacer serveurs et mots de passe — apps/desktop/src-tauri/installer/French.nsh — —
- [BR-CLIENT-010](./BR-CLIENT-010-desinstallation-arret-propre-et-retrait-du-demarrage.md) — La désinstallation arrête l'application et retire l'entrée de démarrage — CheckIfAppIsRunning — —
- [BR-CLIENT-011](./BR-CLIENT-011-menu-de-la-zone-de-notification.md) — Le menu de l'icône propose « Ouvrir Hearth » et « Quitter » — apps/desktop/src-tauri/src/domain.rs::tray_action — —
- [BR-CLIENT-013](./BR-CLIENT-013-ecran-de-premier-lancement.md) — Sans serveur enregistré, l'application s'ouvre sur un écran d'accueil — apps/desktop/src/pages/Welcome.vue — —
- [BR-CLIENT-014](./BR-CLIENT-014-controles-avant-installation.md) — Windows 10 64 bits et 50 Mo libres sont vérifiés avant toute écriture — apps/desktop/src-tauri/installer/hooks.nsh — —

- [BR-TRUST-003](./BR-TRUST-003-creation-de-la-cle-a-la-premiere-connexion.md) — À la première connexion réussie par mot de passe, le client crée une clé propre à l'appareil, gardée dans le coffre de Windows — —
- [BR-TRUST-004](./BR-TRUST-004-serveur-retient-la-cle-publique.md) — Le serveur retient la clé publique de chaque appareil reconnu, inscrite dans la transaction de la connexion par mot de passe réussie — invariant ✓
- [BR-TRUST-005](./BR-TRUST-005-preuve-de-possession-de-la-cle.md) — À chaque connexion, le client prouve qu'il possède la clé privée : défi sans état de 60 secondes, usage unique, signature liée au serveur, à l'identifiant et à l'usage — invariant ✓
- [BR-TRUST-007](./BR-TRUST-007-mise-a-jour-de-l-adresse-retenue.md) — Le serveur retient l'adresse d'une connexion réussie ; une session valide accompagnée d'une preuve de clé fait retenir l'adresse, et l'usage d'une session la rafraîchit — —
- [BR-TRUST-022](./BR-TRUST-022-limite-de-huit-postes-par-compte.md) — Un compte a jusqu'à 8 postes inscrits, le 9e n'est pas inscrit sans rien supprimer ; le titulaire voit la liste et en retire un — invariant ✓
- [BR-TRUST-023](./BR-TRUST-023-la-cle-survit-au-changement-de-son-mot-de-passe.md) — Quand un utilisateur change son propre mot de passe, la clé de chacun de ses postes reconnus survit et reste valide — invariant ✓
- [BR-TRUST-024](./BR-TRUST-024-cles-oubliees-au-changement-de-mot-de-passe-par-un-administrateur.md) — Quand un administrateur change le mot de passe d'un autre compte, ou ferme ses sessions, tous les postes reconnus de ce compte sont oubliés — invariant ✓
- [BR-TRUST-025](./BR-TRUST-025-cles-effacees-a-la-suppression-du-compte.md) — Quand un compte est supprimé, toutes les clés de tous ses postes reconnus sont effacées du serveur — invariant ✓
- [BR-TRUST-001](./BR-TRUST-001-reconnaissance-d-un-poste-deux-criteres-sur-trois.md) — Un poste est reconnu s'il présente deux des trois critères (adresse retenue, session ou premier coup, clé) ; fonction pure en alerte (NORMAL et ALERTE) — `domain/trust/recognition.rs::judge_login` — invariant ✓
- [BR-TRUST-002](./BR-TRUST-002-l-authentification-reste-toujours-requise.md) — Être reconnu ne remplace jamais l'authentification — `domain/login_policy.rs::conclude` — invariant ✓
- [BR-TRUST-006](./BR-TRUST-006-en-alerte-un-poste-reconnu-n-est-pas-ralenti.md) — En alerte, un poste reconnu n'est pas ralenti, un poste non reconnu l'est sans être bloqué — `domain/trust/recognition.rs::judge_login` — invariant ✓
- [BR-TRUST-008](./BR-TRUST-008-alerte-quand-l-identifiant-est-vise.md) — Alerte quand l'identifiant est visé : dérivée du compteur, une fois par épisode, titulaire et administrateurs seulement — `domain/identifier_slowdown.rs::is_alert`, `application/security.rs` — —
- [BR-TRUST-034](./BR-TRUST-034-une-session-seule-ne-vaut-qu-un-critere.md) — Une session seule ne vaut qu'un critère, jamais deux — `application/sessions.rs::authenticate_inner` — invariant ✓
- [BR-TRUST-036](./BR-TRUST-036-tout-acte-d-administration-exige-le-mot-de-passe-et-la-cle.md) — Tout acte d'administration exige, dans la requête même, le mot de passe actuel et la preuve d'une clé inscrite ; l'agent exige dès la construction du service — `entrypoint/http/reauth.rs::layer`, `hearth-proto/admin_act.rs` — invariant ✓
- [BR-TRUST-037](./BR-TRUST-037-la-liste-des-actes-est-fermee.md) — La liste des actes d'administration est fermée : toute route qui modifie est un acte ou une exception nommée, tenue par un test de garde — `hearth-proto/admin_act.rs::{ROUTES, NOT_AN_ACT}` — invariant ✓
- [BR-TRUST-038](./BR-TRUST-038-lire-les-comptes-et-le-journal-sont-des-consultations.md) — Lire les comptes, lire, exporter et suivre le journal sont des consultations : session et rôle — `entrypoint/http/mod.rs::ENDPOINTS` — invariant ✓
- [BR-TRUST-044](./BR-TRUST-044-un-compte-sans-poste-inscrit-n-est-jamais-enferme-dehors.md) — Un compte sans poste inscrit n'est jamais enfermé dehors : reconnexion par mot de passe ou commandes du serveur ; aucune voie par session seule — `entrypoint/http/reauth.rs::layer` — invariant ✓
- [BR-TRUST-049](./BR-TRUST-049-le-client-ne-donne-jamais-un-mot-de-passe-sans-la-preuve-de-cle.md) — Le client ne donne jamais un mot de passe de confirmation sans la preuve de clé ; sans clé au coffre, aucun appel d'administration ne part — `hearth-link` `manager/reauth.rs::send_confirmed` — invariant ✓
- [BR-TRUST-050](./BR-TRUST-050-ce-que-la-cle-signe-est-ce-qui-part-et-la-liste-est-fermee.md) — Ce que la clé signe est ce qui part (acte reconstruit depuis la requête) ; la liaison refuse un acte de la liste fermée sans confirmation — `hearth-link` `domain/act.rs::classify` — invariant ✓
- [BR-TRUST-051](./BR-TRUST-051-chaque-essai-prend-un-defi-neuf-et-une-cle-d-operation-neuve.md) — Chaque essai d'un acte confirmé prend un défi neuf et une clé d'opération neuve — `hearth-link` `manager/reauth.rs::send_confirmed` — invariant ✓
- [BR-TRUST-052](./BR-TRUST-052-la-fenetre-d-un-acte-demande-le-mot-de-passe-selon-ce-que-l-agent-annonce.md) — La fenêtre d'un acte demande le mot de passe selon ce que l'agent annonce, lu à chaque ouverture ; une seule fenêtre pour tous les actes — `hearth_proto::admin_act::covered_by_elevation` — invariant ✓
- [BR-TRUST-045](./BR-TRUST-045-un-acte-sans-confirmation-recoit-un-refus-qui-dit-de-mettre-a-jour-le-client.md) — Un acte sans confirmation reçoit `426` quand l'agent exige ; aucun repli vers la session seule (exigence activée par HRT-30) — `entrypoint/http/reauth.rs::layer` — invariant ✓
- [BR-TRUST-039](./BR-TRUST-039-la-preuve-d-un-acte-est-liee-a-l-acte.md) — La preuve d'un acte est liée à l'acte, sa cible, ses paramètres non secrets, au compte, à la session, au serveur et à un défi frais, à usage unique — `hearth-proto/device_proof.rs::signing_bytes` — invariant ✓
- [BR-TRUST-040](./BR-TRUST-040-la-preuve-avant-le-mot-de-passe-par-le-chemin-de-la-connexion.md) — La preuve est vérifiée avant le mot de passe ; le mot de passe passe par le chemin de la connexion et un échec compte comme une connexion ratée (aussi l'ancien mot de passe de `PUT /me/password`) — `application/sessions.rs::reauthenticate` — invariant ✓
- [BR-TRUST-041](./BR-TRUST-041-la-cle-exigee-selon-l-acte.md) — Clé exigée (Q18) : retrait d'un poste, clé du poste courant ; mode attaque et autres actes, une clé inscrite du compte — `application/trust.rs::{verify_act, verify_removal, verify_attack_mode}` — invariant ✓
- [BR-TRUST-042](./BR-TRUST-042-la-frequence-du-mot-de-passe-est-un-reglage-du-compte.md) — La fréquence du mot de passe est un réglage par compte, défaut une saisie pour 5 minutes (Q19), « à chaque action » offert ; la clé est donnée à chaque acte — `domain/trust/admin_act.rs::{mode_from_seconds, seconds_of}` — invariant ✓
- [BR-TRUST-043](./BR-TRUST-043-l-elevation-vit-dans-l-agent-cinq-minutes-non-glissantes.md) — L'élévation vit dans l'agent, dure 5 minutes non glissantes, est liée à la session, au poste et à l'adresse, se ferme sur les causes listées, n'existe pas en mode attaque, ne couvre jamais les actes exclus — `domain/trust/admin_act.rs::{covered_by_elevation, elevation_holds}` — invariant ✓
- [BR-TRUST-046](./BR-TRUST-046-chaque-acte-confirme-et-chaque-refus-sont-consignes.md) — Chaque acte confirmé et chaque refus de confirmation sont consignés, sans secret — `entrypoint/http/reauth.rs`, `domain/audit/event.rs::Reason` — invariant ✓
- [BR-TRUST-047](./BR-TRUST-047-aucune-decision-d-un-acte-ne-lit-l-horloge-murale.md) — Défi et élévation sur l'horloge monotone ; la garde de réactivation du mode attaque ne lit l'horloge murale qu'avant 30 minutes de marche ou si l'identifiant de démarrage est illisible — `domain/trust/attack_mode.rs::plan_activation` — invariant ✓
- [BR-TRUST-048](./BR-TRUST-048-le-poste-d-une-session-ne-change-jamais.md) — Le poste d'une session est posé à la connexion par mot de passe et n'est jamais réécrit ; sur une session reliée, seule la clé de son poste sert de preuve — `domain/trust/device.rs::session_proof_serves` — invariant ✓
- [BR-TRUST-026](./BR-TRUST-026-la-cle-survit-a-la-reinstallation-du-client.md) — Une réinstallation ou une mise à jour du client garde la clé de l'appareil ; seule une désinstallation complète ou un nouveau PC en crée une nouvelle — —
- [BR-TRUST-035](./BR-TRUST-035-la-regle-deux-sur-trois-ne-joue-qu-en-alerte-et-en-mode-attaque.md) — La règle 2 sur 3 ne s'applique qu'en alerte et en mode attaque ; en état normal aucun poste n'est ralenti ni bloqué au nom de cette règle (HRT-22 ne l'applique encore dans aucun état) — invariant ✓

Les règles BR-CLIENT-012 (état du lien dans l'icône) et le reste de l'installateur sur mesure arrivent avec `hearth-link` et les tickets suivants.

## Pointeurs code

Logique métier = `crates/hearth-agent/src/domain/` (aucune I/O).
Appel client = `crates/hearth-link/src/` (lib réutilisable).
Routes HTTP = `crates/hearth-agent/src/entrypoint/http/`.
Entrypoint CLI = `crates/hearth-agent/src/entrypoint/cli.rs`.
Front Vue = `apps/desktop/src/pages/` (par user story).
- [BR-TRUST-009](./BR-TRUST-009-l-alerte-est-affichee-en-bandeau-et-notifiee.md) — L'alerte est affichée en bandeau sur toutes les pages du serveur (hors données périmées, non fermable) et notifiée par Windows, une fois par épisode — `apps/desktop/src-tauri/src/presence.rs::SecurityWatch::observe`, `alerts.rs::Alerts::on_security` — —
- [BR-TRUST-010](./BR-TRUST-010-activation-du-mode-attaque-depuis-le-client.md) — Un administrateur active le mode attaque depuis le client : confirmation, mot de passe, clé de ce PC (sans clé : indisponible, expliqué, rien envoyé) — `crates/hearth-link/src/manager/security.rs::set_attack_mode`, `apps/desktop/src/security/gate.ts::attackModeBlock` — —
- [BR-TRUST-029](./BR-TRUST-029-un-compte-lecture-seule-voit-l-alerte-mais-ne-change-rien.md) — Un compte Lecture seule voit l'alerte et l'état du mode mais ne peut ni l'activer ni le désactiver — `apps/desktop/src-tauri/src/security/service.rs::interpret`, `apps/desktop/src/security/gate.ts` — invariant ✓
- [BR-TRUST-033](./BR-TRUST-033-notification-windows-d-alerte-limitee-et-reglable-a-part.md) — Notifications de sécurité : une par minute et par serveur avec le lien, priorité à la sécurité, réglage « Alertes de sécurité » à part — `apps/desktop/src-tauri/src/presence.rs::NotificationGate::push_security` — —
- [BR-TRUST-011](./BR-TRUST-011-en-mode-attaque-seuls-les-postes-reconnus-passent.md) — En mode attaque, seuls les postes reconnus (deux critères avant le mot de passe) sont acceptés — `domain/trust/recognition.rs::judge_login` — invariant ✓
- [BR-TRUST-012](./BR-TRUST-012-essai-unique-pour-une-adresse-seule-ou-une-cle-seule.md) — UN essai par critère présenté, par compte et par activation ; garde de réactivation de 30 minutes — `domain/trust/recognition.rs::judge_login`, `domain/trust/attack_mode.rs::plan_activation` — invariant ✓
- [BR-TRUST-013](./BR-TRUST-013-session-seule-refusee-en-mode-attaque.md) — Session seule refusée en mode attaque comme une session expirée, sans essai, non détruite — `domain/trust/recognition.rs::judge_session`, `application/sessions.rs::authenticate_inner` — invariant ✓
- [BR-TRUST-014](./BR-TRUST-014-essai-unique-reussi-le-poste-est-reconnu.md) — Essai unique réussi : le poste est reconnu, l'adresse apprise, l'inscription gelée — `application/sessions.rs::verify` — invariant ✓
- [BR-TRUST-015](./BR-TRUST-015-essai-unique-rate-le-poste-est-bloque.md) — Essai unique raté : bloqué, le bon mot de passe ne compte plus jusqu'à la fin du mode — `domain/trust/recognition.rs::judge_login` — invariant ✓
- [BR-TRUST-016](./BR-TRUST-016-aucun-critere-bloque-sans-essai.md) — Aucun critère : bloqué sans essai ni écriture d'essai — `domain/trust/recognition.rs::judge_login` — invariant ✓
- [BR-TRUST-017](./BR-TRUST-017-un-poste-bloque-ne-sait-rien.md) — Un poste non reconnu reçoit le refus d'un mot de passe faux et ne distingue rien (mode, essai, identifiant) — `application/sessions.rs::verify` — invariant ✓
- [BR-TRUST-018](./BR-TRUST-018-desactivation-manuelle-du-mode-attaque.md) — Désactivation manuelle : administrateur, mot de passe, preuve de clé liée au geste — `application/sessions.rs::set_attack_mode` — invariant ✓
- [BR-TRUST-019](./BR-TRUST-019-sortie-automatique-du-mode-attaque.md) — Sortie automatique après 30 minutes sans tentative refusée, sur l'horloge monotone — `domain/trust/attack_mode.rs::quiet_elapsed` — invariant ✓
- [BR-TRUST-020](./BR-TRUST-020-suspension-du-mode-attaque-au-redemarrage-de-la-machine.md) — Un démarrage de la machine suspend le mode 30 minutes (régime d'alerte), sans horloge murale — `domain/trust/attack_mode.rs::{effective, on_start}` — invariant ✓
- [BR-TRUST-021](./BR-TRUST-021-redemarrage-du-service-ne-suspend-pas-le-mode-attaque.md) — Un redémarrage du service ou une mise à jour de l'agent ne suspend pas le mode — `domain/trust/attack_mode.rs::on_start` — invariant ✓
- [BR-TRUST-027](./BR-TRUST-027-debouclage-perte-de-cle-et-changement-d-adresse.md) — Trois voies de sortie : autre poste reconnu, redémarrage physique, `hearth-agent attack-mode off` — `entrypoint/attack_mode.rs::execute` — —
- [BR-TRUST-028](./BR-TRUST-028-activation-reservee-aux-administrateurs-avec-cle-et-mot-de-passe.md) — Activer ou désactiver exige un administrateur, le mot de passe actuel et la preuve d'une clé inscrite (usage 0x03, liée au jeton et au geste) — `application/sessions.rs::set_attack_mode`, `application/trust.rs::verify_attack_mode` — invariant ✓
- [BR-TRUST-030](./BR-TRUST-030-journal-des-activations-et-desactivations.md) — Le journal enregistre activations, désactivations et sorties automatiques — `application/attack_mode.rs::change` — invariant ✓
- [BR-TRUST-031](./BR-TRUST-031-journal-des-suspensions.md) — Le journal enregistre la suspension du mode et sa reprise — `application/attack_mode.rs::{on_start, resume}` — invariant ✓
- [BR-TRUST-032](./BR-TRUST-032-journal-des-essais-uniques.md) — Le journal enregistre chaque essai unique et son issue, sans secret — `application/sessions.rs::verify` — invariant ✓
