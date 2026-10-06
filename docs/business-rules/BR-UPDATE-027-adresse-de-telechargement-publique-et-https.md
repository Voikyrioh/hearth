---
id: BR-UPDATE-027
domaine: UPDATE
titre: Le serveur ne télécharge qu'en HTTPS, depuis une adresse publique
statut: active
invariant: true
source: revue Stephen de HRT-17 (round 1), ADR-0014
maj: 2026-10-06
---

# BR-UPDATE-027 : Le serveur ne télécharge qu'en HTTPS, depuis une adresse publique

## Règle
Le binaire d'une mise à jour se télécharge en HTTPS seulement (aucune redirection vers HTTP, 5 redirections au plus, TLS 1.3, 128 Mio au plus, délais), depuis une adresse qui n'est ni de bouclage (`127.0.0.0/8`, `::1`, `localhost`), ni non spécifiée, ni privée (`10/8`, `172.16/12`, `192.168/16`, `fc00::/7`), ni lien-local (`169.254/16`, `fe80::/10`), ni partagée (`100.64/10`), ni multicast. **Un seul parseur** : l'adresse est lue par la crate `url`, celle que `reqwest` utilise, et la décision porte sur l'hôte **normalisé** (`2130706433`, `127.1`, `0x7f.0.0.1`, `0177.0.0.1`, `%31%32%37.0.0.1`, `127.0.0.1.`, `[::ffff:127.0.0.1]`, `[::127.0.0.1]` valent tous le bouclage), jamais sur la chaîne brute ; `::/96` (obsolète) est refusée et `64:ff9b::/96` (NAT64) décidée par l'IPv4 incorporée. Le contrôle porte sur l'adresse à la demande (`422 VALIDATION_ERROR`, champ `url`, « adresse locale ou privée refusée »), sur chaque redirection, et **sur ce que le nom devient après résolution** (le résolveur du téléchargeur écarte ces adresses). Sans cela, un jeton administrateur volé ferait sonder le réseau local par un agent root : `unreachable` ou `download_failed` disent si un port est ouvert. **Aucun proxy d'environnement** : le client de téléchargement ignore `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` et `NO_PROXY` (`.no_proxy()` ; FIX-01M47XJXQ0GHV77FN4J6R1NXPZ). Avec un proxy, le nom de l'hôte partirait au proxy, qui le résoudrait lui-même : le filtre appliqué après résolution ne verrait rien et contournerait la règle. Conséquence assumée : un serveur qui ne sort que par un proxy ne peut pas se mettre à jour à distance (`unreachable`) ; mise à jour par `install.sh --binary`. Seuls les tests de bout en bout ouvrent les adresses locales (construction avec `HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES`, jamais en publication).

## Application (code)
- `crates/hearth-agent/src/domain/update/target.rs::{host_is_local, is_local_address, plan_update}` (hôte normalisé par `url`, `localhost`).
- `crates/hearth-agent/src/infrastructure/update/download.rs::HttpsDownloader::fetch` relit l'adresse en `reqwest::Url` et applique le même filtre avant tout envoi.
- `crates/hearth-agent/src/infrastructure/update/download.rs::{PublicOnlyResolver, build_client}` (résolution, redirections, aucun proxy d'environnement).
- `crates/hearth-agent/src/build_info.rs::ALLOW_LOCAL_DOWNLOADS`.

## Vérification
- `domain::update::target::tests::local_and_private_addresses_are_refused_by_literal_and_by_name` (refus) et `legitimate_public_addresses_names_and_ports_are_accepted` (pas de sur-blocage : adresses publiques IPv4 et IPv6, voisins immédiats des plages refusées, noms ordinaires, ports).
- `tests/update_use_cases.rs::a_local_or_private_address_is_refused_before_anything_is_downloaded`.
- `tests/update_download.rs` : `an_environment_proxy_is_never_used_so_the_address_filter_cannot_be_bypassed` (le test se relance avec `HTTPS_PROXY` posé vers un faux proxy : le serveur est joint directement, le filtre s'applique, aucune connexion ne part vers le proxy), `exotic_spellings_of_a_local_address_are_refused_before_any_connection`, `local_and_private_addresses_are_refused_by_name_and_by_literal_unless_allowed`, `a_redirect_to_clear_text_is_refused`.

## Interface (HRT-17, lot interface)
- Le client ne transmet jamais une adresse que l'agent refuserait : `domain::validate_target` refuse une adresse non HTTPS, avec identifiant ou fragment, locale ou privée (bouclage, privées, lien local, partagées, noms internes), hors des releases du dépôt public, et une version non `X.Y.Z` (ADR-0021). L'adresse vient du flux de versions lu par la coquille, jamais de l'interface ; l'agent refait tout.
- Code : `apps/desktop/src-tauri/src/agent_update/domain.rs::{validate_target, host_is_local}`. Tests : `apps/desktop/src-tauri/tests/agent_update_domain.rs`, `agent_target_feed.rs`.

## Cas limites
- Un nom public qui pointe vers le réseau local est refusé à la résolution. Un serveur de versions interne (LAN) n'est pas pris en charge : l'héberger sur une adresse publique, ou mettre à jour par `install.sh --binary`.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-018, ADR-0014

## Historique
- 2026-10-05 : création (HRT-17, suite de la revue de code).
- 2026-10-06 : aucun proxy d'environnement ; table des cas acceptés (HRT-17, suivis de la review).
- 2026-10-06 : section Interface (HRT-17, lot interface, session 2026-10-04-hearth-creation, T28).
