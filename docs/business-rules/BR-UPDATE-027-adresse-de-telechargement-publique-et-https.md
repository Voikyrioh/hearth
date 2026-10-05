---
id: BR-UPDATE-027
domaine: UPDATE
titre: Le serveur ne télécharge qu'en HTTPS, depuis une adresse publique
statut: active
invariant: true
source: revue Stephen de HRT-17 (round 1), ADR-0014
maj: 2026-10-05
---

# BR-UPDATE-027 : Le serveur ne télécharge qu'en HTTPS, depuis une adresse publique

## Règle
Le binaire d'une mise à jour se télécharge en HTTPS seulement (aucune redirection vers HTTP, 5 redirections au plus, TLS 1.3, 128 Mio au plus, délais), depuis une adresse qui n'est ni de bouclage (`127.0.0.0/8`, `::1`, `localhost`), ni non spécifiée, ni privée (`10/8`, `172.16/12`, `192.168/16`, `fc00::/7`), ni lien-local (`169.254/16`, `fe80::/10`), ni partagée (`100.64/10`), ni multicast. **Un seul parseur** : l'adresse est lue par la crate `url`, celle que `reqwest` utilise, et la décision porte sur l'hôte **normalisé** (`2130706433`, `127.1`, `0x7f.0.0.1`, `0177.0.0.1`, `%31%32%37.0.0.1`, `127.0.0.1.`, `[::ffff:127.0.0.1]`, `[::127.0.0.1]` valent tous le bouclage), jamais sur la chaîne brute ; `::/96` (obsolète) est refusée et `64:ff9b::/96` (NAT64) décidée par l'IPv4 incorporée. Le contrôle porte sur l'adresse à la demande (`422 VALIDATION_ERROR`, champ `url`, « adresse locale ou privée refusée »), sur chaque redirection, et **sur ce que le nom devient après résolution** (le résolveur du téléchargeur écarte ces adresses). Sans cela, un jeton administrateur volé ferait sonder le réseau local par un agent root : `unreachable` ou `download_failed` disent si un port est ouvert. Seuls les tests de bout en bout ouvrent les adresses locales (construction avec `HEARTH_UPDATE_ALLOW_LOCAL_ADDRESSES`, jamais en publication).

## Application (code)
- `crates/hearth-agent/src/domain/update/target.rs::{host_is_local, is_local_address, plan_update}` (hôte normalisé par `url`, `localhost`).
- `crates/hearth-agent/src/infrastructure/update/download.rs::HttpsDownloader::fetch` relit l'adresse en `reqwest::Url` et applique le même filtre avant tout envoi.
- `crates/hearth-agent/src/infrastructure/update/download.rs::{PublicOnlyResolver, build_client}` (résolution et redirections).
- `crates/hearth-agent/src/build_info.rs::ALLOW_LOCAL_DOWNLOADS`.

## Vérification
- `domain::update::target::tests::local_and_private_addresses_are_refused_by_literal_and_by_name`.
- `tests/update_use_cases.rs::a_local_or_private_address_is_refused_before_anything_is_downloaded`.
- `tests/update_download.rs` : `exotic_spellings_of_a_local_address_are_refused_before_any_connection`, `local_and_private_addresses_are_refused_by_name_and_by_literal_unless_allowed`, `a_redirect_to_clear_text_is_refused`.

## Cas limites
- Un nom public qui pointe vers le réseau local est refusé à la résolution. Un serveur de versions interne (LAN) n'est pas pris en charge : l'héberger sur une adresse publique, ou mettre à jour par `install.sh --binary`.

## Règles liées
- BR-UPDATE-015, BR-UPDATE-018, ADR-0014

## Historique
- 2026-10-05 : création (HRT-17, suite de la revue de code).
