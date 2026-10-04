# Architecture — hearth-agent

Service installé sur la machine pilotée. Hexagonal : les dépendances vont de `entrypoint` et `infrastructure` vers `application` puis `domain`, jamais l'inverse ; `entrypoint` et `infrastructure` ne se connaissent pas, seul `app.rs` (racine de composition) les assemble. `lib.rs` expose le tout (les tests d'intégration démarrent un vrai serveur) ; `main.rs` initialise les journaux, appelle `app::run` et journalise toute erreur fatale.

```
src/
├── domain/          → Règles pures, sans E/S. identity_policy.rs (créer / réutiliser / refuser / nettoyer l'identité), install_id.rs, secret.rs (`Secret` : jamais affiché, effacé à la libération), sessions.rs (session ouverte, sessions à fermer), accounts/ (username, password, role, admin_guard, self_deletion, account). L'empreinte (`Fingerprint`) vit dans `hearth-proto`.
├── application/     → hello.rs (cas d'usage `GET /hello`, rend `AgentDescription`), accounts.rs (`AccountService`, rend des `AccountView` sans haché : créer, lister, changer le rôle, définir ou changer un mot de passe, supprimer, fermer les sessions) ; ports/ : IdentityStore (partie publique seulement), MachineInfo, AccountRepo, SessionRepo (lectures), Store + UnitOfWork (écritures), PasswordHasher, Clock, IdGen, StoreError
├── infrastructure/  → Adaptateurs des ports et services techniques
│   ├── tls/         → identity.rs (FileIdentityStore : cert.pem, key.pem, install_id, verrou identity.lock ; rcgen), server_config.rs (rustls, TLS 1.3 seul, ring ; seul lecteur de la clé privée)
│   ├── sqlite/      → mod.rs (`Database` : hearth.db, WAL, clés étrangères, migrations embarquées), account_repo.rs (lectures + transaction `BEGIN IMMEDIATE`), session_repo.rs, convert.rs (dates RFC 3339, erreurs de stockage) ; requêtes SQLx vérifiées à la compilation (`.sqlx/` versionné)
│   ├── argon2.rs    → Argon2Hasher : Argon2id m=19 Mio, t=2, p=1, dans `spawn_blocking`
│   ├── clock.rs, ids.rs → horloge système, ULID
│   ├── config/      → agent.toml + variables HEARTH_* + options CLI, fusionnés par `load`
│   ├── logging.rs   → tracing : texte en terminal, JSON sinon (HEARTH_LOG_FORMAT)
│   └── system/      → machine_info.rs (nom d'hôte, adresses MAC)
├── entrypoint/
│   ├── http/        → mod.rs (routeur /api/v1, journal des requêtes, erreurs de routage en ErrorBody), hello.rs, error.rs (ApiError), server.rs (axum-server + rustls, arrêt propre, poignées de main refusées en debug)
│   ├── cli.rs       → clap : options et sous-commandes `serve` (défaut), `fingerprint`, `account add|list|passwd|role|remove|revoke`
│   ├── account.rs   → exécution des sous-commandes `account` (messages de la spec, table de la liste) ; port d'entrée `PasswordInput`
│   ├── terminal.rs  → `TerminalPasswords` : saisie sans écho avec confirmation (rpassword) ou `HEARTH_ACCOUNT_PASSWORD`
│   └── signal.rs    → Ctrl+C / SIGTERM
└── app.rs           → Racine de composition : load_config, load_identity, account_service, start, run
migrations/          → Migrations SQLx embarquées (0001 : accounts, sessions, meta) ; `build.rs` les surveille
tests/hello.rs       → Intégration : serveur sur port libre, client rustls sans vérification, empreinte, TLS 1.2 refusé
tests/accounts_repo.rs, accounts_use_cases.rs → Intégration : dépôts et cas d'usage sur base SQLite temporaire (support/ : horloge, ids, sessions de test)
tests/account_cli.rs → Intégration : le vrai binaire lancé en processus sur un dossier temporaire
```

## Conventions

- **Le cas d'usage rend une structure applicative ; `entrypoint/http` la convertit en type du fil de `hearth-proto`.** `application` ne connaît jamais le contrat JSON.
- La clé privée ne sort pas de `infrastructure/tls` : les ports n'exposent que du public.
- Les décisions métier sont des fonctions pures de `domain/` ; les adaptateurs observent et exécutent (ex. dernier administrateur : le dépôt compte dans la transaction, `domain::accounts::check_removal` décide).
- **Écritures atomiques** : toute écriture passe par `Store::begin` → `UnitOfWork`, une transaction neutre qui couvre tous les sujets (comptes, sessions, plus tard journal et opérations) et lit, garde, écrit en une fois ; SQLite `BEGIN IMMEDIATE` sérialise les écrivains, sans `commit` tout est annulé. Convention : les dépôts (`AccountRepo`, `SessionRepo`…) ne font que lire ; chaque nouvelle écriture est une méthode de `UnitOfWork` (un seul chemin par opération, par exemple `close_sessions`), et HRT-04 y ajoutera la création d'une session et la mise à jour de `last_login_at`, dans la même transaction.
- **Dossier de données** : `infrastructure/data_dir.rs` est le seul endroit qui le crée (0700, resserré s'il est plus ouvert) et qui rend `hearth.db` privé (0600, `-wal` et `-shm` suivent) ; `app.rs` l'appelle avant la base et le magasin d'identité.
- **Dates en base** : texte UTC à largeur fixe `YYYY-MM-DDTHH:MM:SS.mmmZ`, triable ; écriture et lecture faillibles (`StoreError`).
- Les cas d'usage ne rendent jamais le haché du mot de passe (`AccountView`).
- **Secrets** : mots de passe et hachés sont des `Secret` (pas de `Display`, `Debug` masqué, effacés à la libération) ; aucun message d'erreur ni journal ne les contient.
- **SQLx hors ligne** : les `query!` sont vérifiées contre `.sqlx/` ; voir « SQLx » dans `CLAUDE.md` pour régénérer après toute modification de requête ou de migration.
- Les ports sont consommés en `Arc<dyn Port>` / `&dyn Port` : seul `app.rs` connaît les types concrets.

## Pour ajouter une table ou une requête

1. Migration `migrations/NNNN_nom.sql` (jamais modifier une migration déjà publiée).
2. Port dans `application/ports/`, adaptateur dans `infrastructure/sqlite/`, requêtes `query!`.
3. Régénérer `.sqlx/` (voir `CLAUDE.md`), committer le dossier.

## Pour ajouter une route

1. Types de corps dans `hearth-proto/src/api/`.
2. Cas d'usage dans `application/` (ports si besoin), règles dans `domain/`.
3. Handler dans `entrypoint/http/{route}.rs` (conversion vers le type du fil), déclaré dans `router` (`mod.rs`) ; erreurs via `ApiError`.
4. Fiche `docs/open-api/{route}.md` et ligne d'index ; fiche `docs/business-rules/BR-*` si règle.
