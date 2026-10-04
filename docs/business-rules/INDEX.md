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
- [BR-DASH-004](./BR-DASH-004-hysteresis-processeur.md) — La charge du processeur doit tenir un niveau 30 secondes avant d'alerter — `hearth-proto/src/thresholds.rs::cpu_level` — invariant ✓
- [BR-DASH-003](./BR-DASH-003-trois-niveaux-d-alerte.md) — Un état d'alerte comporte trois niveaux : normal, attention, critique — `hearth-proto/src/thresholds.rs::{percent_level, usage_level, temperature_level}` — invariant ✓

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

## Pointeurs code

Logique métier = `crates/hearth-agent/src/domain/` (aucune I/O).
Appel client = `crates/hearth-link/src/` (lib réutilisable).
Routes HTTP = `crates/hearth-agent/src/entrypoint/http/`.
Entrypoint CLI = `crates/hearth-agent/src/entrypoint/cli.rs`.
Front Vue = `apps/desktop/src/pages/` (par user story).
