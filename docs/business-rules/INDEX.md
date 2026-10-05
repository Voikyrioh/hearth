# Business Rules — Hearth

Règles métier par domaine. Chaque fiche `BR-{DOMAINE}-{NNN}-{slug}.md` documente une règle : énoncé, code qui l'applique, vérification, cas limites. Créées par ticket lors du dev.

## Fiches

- [BR-CONN-001](./BR-CONN-001-verification-empreinte.md) — Empreinte du serveur : format 8 × 4 hexadécimaux et confirmation — `hearth-proto/src/fingerprint.rs::Fingerprint::short` — invariant ✓
- [BR-CONN-006](./BR-CONN-006-verrouillage-apres-echecs.md) — 5 échecs : attente 1 min doublée, plafond 15 min — `domain/lockout.rs::step` — invariant ✓
- [BR-CONN-007](./BR-CONN-007-tentatives-par-identifiant-et-adresse.md) — Tentatives comptées par identifiant et adresse du client — `domain/lockout.rs::AttemptKey::new` — —
- [BR-CONN-013](./BR-CONN-013-message-de-connexion-generique.md) — Refus de connexion générique, même chemin identifiant inconnu / mot de passe faux — `application/sessions.rs::SessionService::login` — invariant ✓
- [BR-CONN-014](./BR-CONN-014-incompatibilite-de-version.md) — Version d'interface incompatible : 426 et qui met à jour — `domain/compat.rs::check` — invariant ✓
- [BR-RESIL-001](./BR-RESIL-001-etat-du-lien-toujours-visible.md) — L'état du lien du serveur affiché est toujours visible dans l'en-tête — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-004](./BR-RESIL-004-bandeau-hors-ligne.md) — « Hors ligne » : bandeau avec l'heure du dernier contact et « Réessayer maintenant » — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-005](./BR-RESIL-005-reconnexion-automatique-et-reessayer.md) — Reconnexion automatique sans fin, et « Réessayer maintenant » relance tout de suite — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-007](./BR-RESIL-007-donnees-perimees-datees.md) — Hors « Connecté », les dernières données restent affichées, désaturées et datées — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-008](./BR-RESIL-008-actions-desactivees-expliquees.md) — Hors « Connecté », les actions qui exigent le serveur sont désactivées et expliquées — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-010](./BR-RESIL-010-operation-rejouee-sans-reexecution.md) — Clé d'opération rejouée : premier résultat, sans ré-exécution — `domain/operations.rs::classify` — invariant ✓
- [BR-RESIL-012](./BR-RESIL-012-session-expiree-glissante.md) — Session glissante 30 jours, jeton stocké haché — `domain/sessions.rs::check`, `domain/session_token.rs` — invariant ✓
- [BR-RESIL-014](./BR-RESIL-014-acces-revoque.md) — Session fermée par l'administration : SESSION_REVOKED — `domain/sessions.rs::check` — invariant ✓
- [BR-RESIL-017](./BR-RESIL-017-session-longue-hors-ligne-sans-croissance.md) — Une longue session hors ligne ne fait pas grossir l'interface — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-011](./BR-RESIL-011-notifications-jamais-bloquantes.md) — Aucune fenêtre bloquante pour une perte de lien ou une erreur ; notifications discrètes — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-018](./BR-RESIL-018-notifications-agregees-par-compteur.md) — Une même notification répétée devient un compteur — apps/desktop/src (voir la fiche) — —
- [BR-RESIL-020](./BR-RESIL-020-etat-du-lien-par-serveur.md) — Chaque serveur a son propre état de lien, indépendant — apps/desktop/src (voir la fiche) — —
- [BR-INSTALL-004](./BR-INSTALL-004-empreinte-generee-une-fois.md) — Empreinte générée une seule fois, jamais modifiée — `domain/identity_policy.rs::decide` — invariant ✓
- [BR-ACCT-001](./BR-ACCT-001-creation-compte.md) — Un compte est créé avec un identifiant unique, un mot de passe et un rôle — `application/accounts.rs::AccountService::create` — —
- [BR-ACCT-002](./BR-ACCT-002-format-identifiant.md) — Identifiant : 3 à 32 caractères, minuscules, chiffres, tiret, underscore — `domain/accounts/username.rs::Username::parse` — ✓
- [BR-ACCT-003](./BR-ACCT-003-unicite-insensible-casse.md) — Identifiant unique, insensible à la casse — `domain/accounts/username.rs::Username::parse` — ✓
- [BR-ACCT-004](./BR-ACCT-004-complexite-mot-de-passe.md) — Mot de passe : 12 caractères, majuscule, minuscule, chiffre — `domain/accounts/password.rs::unmet_rules` — ✓
- [BR-ACCT-005](./BR-ACCT-005-mot-de-passe-sans-identifiant.md) — Le mot de passe ne contient pas l'identifiant — `domain/accounts/password.rs::unmet_rules` — ✓
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
- [BR-AUDIT-010](./BR-AUDIT-010-affichage-temps-reel.md) — Les nouvelles entrées arrivent en direct — `application/ports/audit_sink.rs::AuditFeed` — —
- [BR-AUDIT-011](./BR-AUDIT-011-donnees-perimees.md) — Données périmées quand le lien est coupé — — — —
- [BR-AUDIT-012](./BR-AUDIT-012-fuseau-horaire-client.md) — L'heure s'affiche dans le fuseau du PC client — `entrypoint/http/wire.rs::date` — —
- [BR-AUDIT-013](./BR-AUDIT-013-regroupement-rafales.md) — Les refus en rafale sont regroupés à l'affichage — — — —
- [BR-AUDIT-014](./BR-AUDIT-014-gestion-filtres.md) — Les filtres appliqués restent visibles et se réinitialisent en un clic — `domain/audit/filter.rs::{AuditFilter, RawFilter}` — —
- [BR-AUDIT-015](./BR-AUDIT-015-filtres-multi-valeurs-periode.md) — Compte, action et résultat acceptent plusieurs valeurs ; une période à la fois — `domain/audit/filter.rs::AuditFilter::new` — —
- [BR-AUDIT-016](./BR-AUDIT-016-recherche-plein-texte.md) — La recherche plein texte porte sur tous les champs visibles — `domain/audit/filter.rs::SearchQuery::parse` — —
- [BR-AUDIT-017](./BR-AUDIT-017-export-resultat-filtre.md) — L'export porte sur le résultat filtré — `domain/audit/csv.rs::{render, field}` — —
- [BR-AUDIT-018](./BR-AUDIT-018-resultat-vide.md) — Aucun événement : message et bouton d'effacement — `application/audit.rs::AuditService::search` — —
- [BR-AUDIT-019](./BR-AUDIT-019-indicateur-conservation.md) — L'indicateur de conservation est toujours visible — `domain/audit/policy.rs::{RETENTION, MAX_ENTRIES}` — —
- [BR-AUDIT-020](./BR-AUDIT-020-rechargement-manuel.md) — Rechargement manuel quand le lien est coupé — — — —
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

| Domaine | Rôle | Nombre fiches | Référence conception |
|---|---|---|---|
| **INSTALL** | Installation agent : déploiement, service système, génération certificat | 12 | us-installer-agent |
| **CLIENT** | Installation et configuration client desktop : NSIS, Tauri, démarrage, zone notification | 14 | us-installer-client |
| **CONN** | Connexion au serveur : découverte, épinglage TLS, authentification, session, coffre | 17 | us-connecter-serveur |
| **DASH** | Tableau de bord machine : mesures système, GPU, historique, seuils | 15 | us-tableau-de-bord-machine |
| **RESIL** | Résilience du lien : reconnexion, opérations idempotentes, instantanés, affichage état | 20 | us-lien-resilient |
| **ACCT** | Gestion comptes : création, suppression, rôles, mots de passe, dernier administrateur | 16 | us-gerer-comptes |
| **AUDIT** | Journal d'activité : logging, filtrage, recherche FTS5, purge, export CSV | 21 | us-journal-activite |
| **UPDATE** | Mises à jour agent et client : flux de versions, signatures minisign, superviseur, rollback | 26 | us-mises-a-jour |

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
- [BR-CLIENT-006](./BR-CLIENT-006-demarrage-avec-windows-desactive-par-defaut.md) — Le lancement au démarrage de Windows est proposé à l'installation, désactivé par défaut (partiellement appliquée, HRT-21) — apps/desktop/src-tauri/src/settings.rs::read — —
- [BR-CLIENT-007](./BR-CLIENT-007-demarrage-avec-windows-reglable.md) — Le lancement au démarrage se règle depuis l'application — apps/desktop/src-tauri/src/settings.rs::set_launch_at_startup — —
- [BR-CLIENT-008](./BR-CLIENT-008-reinstallation-conserve-les-donnees.md) — Réinstaller par-dessus une version existante conserve serveurs et réglages — domain/ — invariant ✓
- [BR-CLIENT-009](./BR-CLIENT-009-desinstallation-garder-ou-tout-effacer.md) — La désinstallation propose de garder ou d'effacer serveurs et mots de passe — apps/desktop/src-tauri/installer/French.nsh — —
- [BR-CLIENT-010](./BR-CLIENT-010-desinstallation-arret-propre-et-retrait-du-demarrage.md) — La désinstallation arrête l'application et retire l'entrée de démarrage — CheckIfAppIsRunning — —
- [BR-CLIENT-011](./BR-CLIENT-011-menu-de-la-zone-de-notification.md) — Le menu de l'icône propose « Ouvrir Hearth » et « Quitter » — apps/desktop/src-tauri/src/domain.rs::tray_action — —
- [BR-CLIENT-013](./BR-CLIENT-013-ecran-de-premier-lancement.md) — Sans serveur enregistré, l'application s'ouvre sur un écran d'accueil — apps/desktop/src/pages/Welcome.vue — —
- [BR-CLIENT-014](./BR-CLIENT-014-controles-avant-installation.md) — Windows 10 64 bits et 50 Mo libres sont vérifiés avant toute écriture — apps/desktop/src-tauri/installer/hooks.nsh — —

Les règles BR-CLIENT-012 (état du lien dans l'icône) et le reste de l'installateur sur mesure arrivent avec `hearth-link` et les tickets suivants.

## Pointeurs code

Logique métier = `crates/hearth-agent/src/domain/` (aucune I/O).
Appel client = `crates/hearth-link/src/` (lib réutilisable).
Routes HTTP = `crates/hearth-agent/src/entrypoint/http/`.
Entrypoint CLI = `crates/hearth-agent/src/entrypoint/cli.rs`.
Front Vue = `apps/desktop/src/pages/` (par user story).
