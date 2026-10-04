---
id: ADR-0009
titre: Dépendances de l'agent : axum, axum-server, rustls + ring, rcgen, clap, toml, mac_address, tower-http, tracing
type: librairie
statut: acceptée
date: 2026-10-04
portee: projet
remplace: —
liens: [ADR-0003, ADR-0004, ADR-0005, conception technique 2026-10-04 sections 3 et 10, HRT-02]
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
| `tracing` + `tracing-subscriber` (`env-filter`, `fmt`, `json`) | Journaux structurés | Sortie standard reprise par journald ; texte en terminal, JSON sinon (`HEARTH_LOG_FORMAT`). | `anyhow` seulement dans `main` ; ailleurs `thiserror`. |
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
- Les tests d'intégration activent `tls12` côté client pour prouver le refus de TLS 1.2.

## Références

- ADR-0003 (musl), ADR-0005 (TLS épinglé), ADR-0006 (SQLx).
- https://docs.rs/axum-server · https://docs.rs/rustls · https://docs.rs/rcgen
