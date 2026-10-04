---
id: ADR-0009
titre: Dépendances de l'agent : axum, axum-server, rustls + ring, rcgen, clap, toml, mac_address, tower-http, tracing, sqlx, argon2, zeroize, ulid, rpassword, async-trait
type: librairie
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [ADR-0003, ADR-0004, ADR-0005, ADR-0006, conception technique 2026-10-04 sections 3 et 10, HRT-02, HRT-03]
---

# ADR-0009 — Dépendances de l'agent

## Contexte

HRT-02 pose le premier code de l'agent ; les tickets suivants repartiront de cette liste. L'agent doit rester compilable en `x86_64-unknown-linux-musl` sans dépendance C lourde (ADR-0003) et parler TLS 1.3 seul avec un certificat auto-signé (ADR-0005). Toutes les versions sont déclarées une fois dans `[workspace.dependencies]` du `Cargo.toml` racine.

## Décision

| Crate | Rôle | Pourquoi | Limites / quand ne pas l'utiliser |
|---|---|---|---|
| `axum` 0.8 (`http1`, `http2`, `json`, `tokio`) | Routage HTTP | Standard du hub (API Hono côté Node, axum côté Rust), typé, écosystème tower | Pas de macros ni de WebSocket activés : la feature `ws` sera ajoutée avec HRT-06. Ne pas l'utiliser dans `domain/` ni `hearth-proto`. |
| `axum-server` 0.8 (`tls-rustls-no-provider`) | Serveur HTTPS | Fournit l'acceptation TLS et l'arrêt propre pour axum. La feature `tls-rustls` est **interdite** : elle active `aws-lc-rs`. | Si on a besoin de contrôle fin des connexions, passer à hyper + tokio-rustls. |
| `rustls` 0.23 + `ring` | TLS 1.3 | Pure Rust côté API, fournisseur `ring` compilable en musl, pas d'OpenSSL. TLS 1.2 non activé dans le binaire (feature `tls12` seulement en tests). | `ring` compile du C et de l'assembleur : le conteneur de build musl doit avoir un compilateur C. |
| `rcgen` 0.14 (`ring`, `pem`) | Génération du certificat auto-signé | Simple, sans OpenSSL. | Génération seulement ; ne vérifie rien. |
| `clap` 4 (derive) | Ligne de commande | Standard, aide et erreurs gratuites. | Ne pas y mettre de logique ; elle vit dans `app.rs`. |
| `toml` 1 + `serde` | Lecture de `agent.toml` | Format lisible par l'administrateur, clés inconnues refusées. | Fichier de petite taille seulement. |
| `mac_address` 1 + `gethostname` 1 | Adresses MAC, nom de machine | Évitent `sysinfo` (lourd) pour deux lectures au démarrage. À remplacer par `sysinfo` si HRT-06 l'adopte déjà. | Lectures faites une fois au démarrage, jamais par requête. |
| `tower-http` 0.6 (`trace`) | Journal de chaque requête | Méthode, chemin, statut, durée en une ligne. | Ne jamais journaliser d'en-têtes `Authorization` ni de corps. |
| `tracing` + `tracing-subscriber` (`env-filter`, `fmt`, `json`) | Journaux structurés | Sortie standard reprise par journald ; texte en terminal, JSON sinon (`HEARTH_LOG_FORMAT`). | Erreur fatale journalisée une seule fois, dans `main`. |
| `serde` + `serde_json` | Corps JSON, configuration | Standard Rust. | `hearth-proto` n'a que ces deux crates plus `sha2` et `thiserror`. |
| `thiserror` 2 | Erreurs typées par couche | Convention du dépôt (CLAUDE.md) : pas d'`anyhow` dans le code de l'agent. | Chaque message d'erreur porte sa cause ; ne pas la ré-imprimer en parcourant la chaîne. |
| `tokio` 1 | Runtime asynchrone, signaux, tâches | Imposé par axum et rustls. | Rien de bloquant dans les handlers ; le démarrage est synchrone (verrou d'identité). |
| `time` 0.3 (`formatting`, `parsing`) | Dates de validité du certificat (rcgen), dates des comptes et sessions (RFC 3339 en base) | Type attendu par `rcgen` ; déjà dans l'arbre. | L'heure courante passe par le port `Clock` : jamais `now_utc()` dans `domain/` ni `application/`. |
| `sqlx` 0.9 (`runtime-tokio`, `sqlite`, `migrate`, `macros`, `default-features = false`) | Base SQLite `hearth.db` : pool, migrations embarquées, requêtes `query!` vérifiées à la compilation | ADR-0006. `sqlite` compile `libsqlite3` embarquée avec `cc` (comme `ring`, pas de bibliothèque système). Pas de feature TLS, ni `any`, ni `chrono`/`uuid` (dates et identifiants sont des textes). Mode hors ligne : `.sqlx/` versionné. | Pas d'accès à SQLx hors de `infrastructure/sqlite/`. Requête construite dynamiquement : `query_as` non vérifiée, à justifier. |
| `argon2` 0.6 | Hachage des mots de passe : Argon2id, m = 19 Mio, t = 2, p = 1 (OWASP) | Pur Rust. Paramètres inscrits dans le haché PHC : ils pourront évoluer sans casser les anciens hachés. | Calcul coûteux : toujours dans `spawn_blocking`, jamais dans le runtime asynchrone. |
| `zeroize` 1 | Efface la mémoire des `Secret` à la libération | Minimal. | N'efface pas les copies faites avant la libération : ne pas cloner un secret. |
| `ulid` 3 | Identifiants techniques des comptes et sessions (triables par date) | Format imposé par la conception technique. | Pas d'ULID pour les jetons de session (32 octets aléatoires, HRT-04). |
| `rpassword` 7 | Saisie du mot de passe sans écho au terminal (`account add`, `passwd`) | Windows et Unix. | Terminal requis : sans terminal, `HEARTH_ACCOUNT_PASSWORD`. Entrypoint seulement. |
| `async-trait` 0.1 | Ports asynchrones utilisables en objet (`Arc<dyn AccountRepo>`) | Les traits asynchrones natifs ne sont pas utilisables en objet. | À retirer quand les objets-traits asynchrones seront stables. |
| `sha2` (dans `hearth-proto`) | Empreinte SHA-256 | Pur Rust, partagé avec `hearth-link`. | Pas de cryptographie d'authentification avec cette crate seule. |

**Règle** : pas d'`aws-lc`, pas d'OpenSSL, aucune dépendance qui ne compile pas en musl. Une nouvelle dépendance de l'agent passe par une mise à jour de cette ADR.

## Comment l'appliquer

- `Cargo.toml` racine : version et features dans `[workspace.dependencies]`, `xxx.workspace = true` dans les crates.
- Vérification : `cargo tree -p hearth-agent -i aws-lc-rs` et `-i openssl-sys` doivent répondre « did not match any packages ».

## Alternatives rejetées

- **`axum-server` avec `tls-rustls`** : tire `aws-lc-rs` (C lourd, musl difficile).
- **`native-tls` / OpenSSL** : dépendance système, incompatible avec le binaire statique.
- **`figment`** pour la configuration : plus puissant que nécessaire pour quatre clés.

## Conséquences

- Le conteneur `rust:alpine` de `cargo xtask agent` doit fournir un compilateur C pour `ring`.
- SQLx en mode hors ligne : `cargo sqlx prepare` régénère `.sqlx/` (voir `CLAUDE.md`) ; la CI compile sans `DATABASE_URL`.
- Dépendances de test seulement : `tempfile` (dossiers temporaires), `tokio-rustls` (client TLS des tests d'intégration), `tower` (`oneshot` sur le routeur).
- Les tests d'intégration activent `tls12` côté client pour prouver le refus de TLS 1.2.

## Références

- ADR-0003 (musl), ADR-0005 (TLS épinglé), ADR-0006 (SQLx).
- https://docs.rs/axum-server · https://docs.rs/rustls · https://docs.rs/rcgen
