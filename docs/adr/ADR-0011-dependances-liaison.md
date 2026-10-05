---
id: ADR-0011
titre: Dépendances de la bibliothèque de liaison : reqwest, rustls + ring, tokio-rustls, tokio-tungstenite, futures-util, if-addrs
type: librairie
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0004, ADR-0005, ADR-0007, ADR-0009, conception technique 2026-10-04 sections 1, 3, 9 et 10, HRT-07]
---

# ADR-0011 — Dépendances de la bibliothèque de liaison

## Contexte

`hearth-link` (HRT-07) est le seul code client qui parle à l'agent : requêtes HTTPS, flux WebSocket, épinglage de l'empreinte du certificat (ADR-0005), reconnexion (ADR-0007). Elle tourne dans le client Tauri sous Windows (et sous Linux pour les tests) : pas de contrainte musl, mais la même règle que pour l'agent : **pas d'`aws-lc`, pas d'OpenSSL** (un seul fournisseur cryptographique, `ring`, déjà celui de l'agent). Toutes les versions sont déclarées une fois dans `[workspace.dependencies]` du `Cargo.toml` racine.

## Décision

| Crate | Rôle | Pourquoi | Limites / quand ne pas l'utiliser |
|---|---|---|---|
| `reqwest` 0.13 (`rustls-no-provider`, sans `default-features`) | Requêtes HTTPS : `hello`, connexion, déconnexion, actions, relecture d'opération | Client HTTP éprouvé, délais, lecture en flux. `rustls-no-provider` évite `aws-lc-rs` ; la configuration TLS (fournisseur `ring`, TLS 1.3 seul, vérificateur d'empreinte) est fournie par `use_preconfigured_tls`. HTTP/1.1 seulement, sans proxy système (`no_proxy`) : un proxy d'environnement casserait l'épinglage. | La feature `rustls-no-provider` tire aussi `rustls-platform-verifier`, jamais utilisé ici (le vérificateur est le nôtre). Ne jamais activer `rustls` (feature par défaut : `aws-lc-rs`). Une connexion neuve par appel (`pool_max_idle_per_host(0)`) : une coupure se voit à l'appel suivant, jamais sur une connexion morte restée au chaud ; coût négligeable à ces volumes. |
| `rustls` 0.23 (`ring`, `std`) | TLS 1.3 : vérificateurs sur mesure « sonde » et « épinglé » (`adapters/tls.rs`) | Seule façon de refuser un certificat par son empreinte, quelle que soit la chaîne ou le nom. La signature de la poignée de main est vérifiée (`verify_tls13_signature`) : sans cela, quiconque connaît le certificat public pourrait se faire passer pour le serveur. | Le mode « sonde » (accepte tout) n'est utilisé que pour le premier `/hello` ; aucune requête authentifiée n'y passe (BR-CONN-011). TLS 1.2 refusé. |
| `tokio-rustls` 0.26 (`ring`) | Poignée de main TLS du flux WebSocket | Même configuration rustls que reqwest ; donne la connexion à `tokio-tungstenite`. | — |
| `tokio-tungstenite` 0.29 (`handshake`) | Client WebSocket du flux `/stream` (`client_async_with_config`) | Même version que celle d'`axum`. Sans la feature `connect` ni TLS propre : on lui passe la connexion TLS épinglée. Taille de message bornée (4 Mio). | Les messages mal formés sont ignorés par l'adaptateur (`Frame::Other`), jamais une erreur. |
| `futures-util` 0.3 (`sink`, `std`) | `StreamExt`/`SinkExt` du WebSocket ; `FutureExt::catch_unwind` de la supervision | Déjà dans l'arbre. | La supervision (`catch_unwind`) exige `panic = "unwind"` (profil release du dépôt). |
| `if-addrs` 0.15 | Adresses réseau locales (détection d'un changement de réseau, BR-RESIL-006) | Windows et Linux, Rust pur côté appels système. | Lecture bloquante : toujours dans `spawn_blocking`. Ne dit rien de la qualité du réseau : un simple changement de liste d'adresses déclenche une tentative immédiate. |
| `async-trait` 0.1 | Ports asynchrones en objet (`Arc<dyn Transport>`) | Même raison que l'agent (ADR-0009). | À retirer avec les objets-traits asynchrones stables. |
| `zeroize` 1 | Efface les `Secret` (mot de passe, jeton) et le mot de passe d'une requête de connexion après usage | Minimal. | N'efface pas les copies faites par des bibliothèques tierces (reqwest copie l'en-tête `Authorization`, marqué sensible). |
| `ulid` 3 | Clé d'opération (`Idempotency-Key`) et identifiant de serveur | Format imposé par ADR-0004. | — |
| `getrandom` 0.4 | Aléa des délais de reconnexion (`OsRng`) | Déjà dans l'arbre. | Repli sur un mélange horloge + compteur si le système échoue : suffisant pour décaler des tentatives, jamais pour un secret. |
| `serde`, `serde_json`, `thiserror`, `tokio`, `tracing`, `sha2` (via `hearth-proto`) | Corps JSON, erreurs typées par couche, runtime, journaux | Conventions du dépôt. | Pas d'`anyhow`. Aucun secret dans un journal. |

Dépendances de développement : `hearth-agent` (un vrai agent démarré dans le processus des tests, comme le font ses propres tests d'intégration), `rcgen` (certificats des tests du vérificateur), `tempfile`, `time`, `tokio` avec `test-util` (temps virtuel du test de robustesse).

## Comment l'appliquer

- Vérification : `cargo tree -p hearth-link -i aws-lc-rs` et `-i openssl` doivent répondre « did not match any packages ».
- Toute nouvelle dépendance de `hearth-link` passe par une mise à jour de cette ADR.
- `domain/` n'en importe aucune de réseau, de fichiers ou de runtime : seulement `hearth-proto`, `serde`, `thiserror`, `zeroize`.

## Alternatives rejetées

- **`hyper` + `hyper-rustls` directement** : plus de code à écrire (timeouts, corps, redirections) pour aucun gain ici.
- **`reqwest` avec `rustls` (par défaut)** : tire `aws-lc-rs` (second fournisseur cryptographique, compilation C lourde).
- **`native-tls`** : n'épingle pas une empreinte, dépend du système.
- **Réutiliser une connexion HTTP** : gagne quelques millisecondes, perd la détection immédiate d'une coupure.
- **`tungstenite` synchrone** : bloquerait un fil par serveur.

## Conséquences

- Un seul fournisseur cryptographique (`ring`) dans tout le dépôt.
- La bibliothèque est testable de bout en bout contre un vrai agent (mandataire à pannes), sans réseau extérieur.
