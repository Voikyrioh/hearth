---
id: BR-FICHE-062
domaine: FICHE
titre: L'image d'un jeu vient de Steam (identifiant du jeu) ou d'une illustration originale livrée avec l'agent, jamais d'une image d'éditeur copiée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-16-fonctionnelle-choisir-un-jeu-dans-le-catalogue.md (BR-FICHE-062), HRT-238, HRT-240
maj: 2026-11-16
---

# BR-FICHE-062 — D'où vient l'image d'un jeu

## Règle
Une fiche dit d'où vient l'image de son jeu, ou rien. Un jeu présent sur Steam porte `steam_app_id` : l'identifiant du JEU dans le magasin (un entier de 1 à 4 294 967 295, jamais celui de son serveur dédié). Un jeu absent de Steam peut porter `illustration` : le nom d'un fichier JPEG du dossier de la fiche (`[a-z0-9-]+.jpg`), original (aucun logo, personnage officiel, visuel d'éditeur ni lettrage du nom), de 920 sur 430 pixels, de 300 Kio au plus, avec sa provenance dans `illustration.provenance.toml` (`tool`, `made`, `requested_by`, `prompt_ref`, `original = true`). Une fiche ne porte jamais les deux. Aucune image d'éditeur n'est copiée dans Hearth. Une fiche sans image est valable (le client montre un visuel neutre). Ni l'identifiant ni l'illustration n'entrent dans l'empreinte de la fiche : changer d'image ne déclenche jamais `FICHE_CHANGED`.

Refus au chargement : `SteamAppIdHorsBorne`, `IllustrationEtSteam`, `IllustrationNomInvalide` (domaine) ; à la compilation (`build.rs`) : `illustration_missing`, `illustration_not_jpeg`, `illustration_size`, `illustration_dimensions`, `illustration_provenance`. L'agent sert l'illustration (octets du binaire, aucune lecture de fichier à l'exécution) sur `GET /catalog/{id}/illustration` pour une fiche LISTÉE : `image/jpeg`, `nosniff`, `Content-Security-Policy: default-src 'none'`, `ETag` = empreinte, `304` sur `If-None-Match`, `404 FICHE_UNKNOWN` (inconnue, identifiant non stable) ou `404 ILLUSTRATION_ABSENT`. Lecture ouverte à tout compte authentifié, jamais journalisée.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_image_source, illustration_name_ok}`
- `crates/hearth-proto/src/jpeg.rs` (lecteur d'en-tête pur, partagé)
- `crates/hearth-link/src/manager/catalog.rs::{read_fiche_illustration, check_illustration_body}` (lecture bornée et vérifiée, HRT-240)
- `crates/hearth-agent/build.rs::{embed_illustration, check_provenance}`
- `crates/hearth-agent/src/application/catalog.rs::{CatalogService::illustration, illustration_of}`
- `crates/hearth-agent/src/entrypoint/http/catalog.rs::illustration`

## Vérification
- `crates/hearth-proto/src/jpeg.rs` (module `tests` : SOF0, SOF2, marqueurs avant la trame, troncature, longueurs fausses, graine fixe)
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::{a_steam_app_id_is_an_integer_from_one_to_u32_max, a_fiche_never_carries_both_a_steam_id_and_an_illustration, an_illustration_name_is_a_simple_lowercase_jpg_name, every_embedded_fiche_has_a_steam_id_or_an_illustration_or_nothing_never_both, the_embedded_illustration_of_minecraft_is_a_920_by_430_jpeg_under_300_kib}`
- `crates/hearth-agent/tests/catalog_https.rs::{the_card_carries_the_steam_app_id, the_illustration_is_served_with_its_type_length_etag_and_nosniff, if_none_match_gives_304, an_unknown_or_non_stable_id_is_404_without_echo, a_steam_fiche_has_no_illustration_404_illustration_absent, an_anonymous_request_is_401, the_route_is_open_read_and_never_journaled}`

## Cas limites
- Les octets d'une image hors règle ne se testent pas par `cargo test` (la compilation échoue) : la règle est jugée par `jpeg::check_illustration`, testée.
- Provenance de l'illustration de Minecraft : `crates/hearth-agent/catalog/minecraft/illustration.provenance.toml`.

## Règles liées
- BR-FICHE-003, BR-FICHE-061, BR-CLIENT-015

## Historique
- 2026-11-16 — création (HRT-238, HRT-240).
