# Business Rules — Hearth

Règles métier par domaine. Chaque fiche `BR-{DOMAINE}-{NNN}-{slug}.md` documente une règle : énoncé, code qui l'applique, vérification, cas limites. Créées par ticket lors du dev.

## Fiches

- [BR-CONN-001](./BR-CONN-001-verification-empreinte.md) — Empreinte du serveur : format 8 × 4 hexadécimaux et confirmation — `hearth-proto/src/fingerprint.rs::Fingerprint::short` — invariant ✓
- [BR-CONN-006](./BR-CONN-006-verrouillage-apres-echecs.md) — 5 échecs : attente 1 min doublée, plafond 15 min — `domain/lockout.rs::step` — invariant ✓
- [BR-CONN-007](./BR-CONN-007-tentatives-par-identifiant-et-adresse.md) — Tentatives comptées par identifiant et adresse du client — `domain/lockout.rs::AttemptKey::new` — —
- [BR-CONN-013](./BR-CONN-013-message-de-connexion-generique.md) — Refus de connexion générique, même chemin identifiant inconnu / mot de passe faux — `application/sessions.rs::SessionService::login` — invariant ✓
- [BR-CONN-014](./BR-CONN-014-incompatibilite-de-version.md) — Version d'interface incompatible : 426 et qui met à jour — `domain/compat.rs::check` — invariant ✓
- [BR-RESIL-010](./BR-RESIL-010-operation-rejouee-sans-reexecution.md) — Clé d'opération rejouée : premier résultat, sans ré-exécution — `domain/operations.rs::classify` — invariant ✓
- [BR-RESIL-012](./BR-RESIL-012-session-expiree-glissante.md) — Session glissante 30 jours, jeton stocké haché — `domain/sessions.rs::check`, `domain/session_token.rs` — invariant ✓
- [BR-RESIL-014](./BR-RESIL-014-acces-revoque.md) — Session fermée par l'administration : SESSION_REVOKED — `domain/sessions.rs::check` — invariant ✓
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
- [BR-ACCT-016](./BR-ACCT-016-changements-consignes-au-journal.md) — Changements de compte consignés au journal (HRT-05, pas encore appliquée) — — — —

| Domaine | Rôle | Nombre fiches | Référence conception |
|---|---|---|---|
| **INSTALL** | Installation agent : déploiement, service système, génération certificat | 12 | us-installer-agent |
| **CLIENT** | Installation et configuration client desktop : NSIS, Tauri, démarrage, zone notification | 14 | us-installer-client |
| **CONN** | Connexion au serveur : découverte, épinglage TLS, authentification, session, coffre | 17 | us-connecter-serveur |
| **DASH** | Tableau de bord machine : mesures système, GPU, historique, seuils | 14 | us-tableau-de-bord-machine |
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
