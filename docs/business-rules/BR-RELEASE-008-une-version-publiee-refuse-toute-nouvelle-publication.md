---
id: BR-RELEASE-008
domaine: RELEASE
titre: Une version publiée (manifeste et release GitHub) refuse toute nouvelle publication ; une étiquette sans manifeste est une anomalie
statut: active
invariant: true
source: HRT-172, contexts/hearth/conceptions/2026-10-16-fonctionnelle-publier-une-version.md (BR-RELEASE-008), 2026-10-16-technique-publier-une-version.md (7.1, 9.2)
maj: 2026-10-16
---

# BR-RELEASE-008 : État publié

## Règle
Une version est « publiée » quand le manifeste public (`GET https://hearth.voikyrioh.fr/api/releases/latest`) l'annonce ET que sa release GitHub `vX.Y.Z` existe. `cargo xtask release-preflight` lit trois choses, dans cet ordre, puis décide (`X` = le numéro du dépôt, `M` = la plus récente des versions du manifeste, client et agent) :

| Lecture | Décision | Message |
|---|---|---|
| manifeste illisible | arrêt | « Le manifeste public n'a pas pu être lu ({cause}) : publication arrêtée. » |
| `M` > `X` | arrêt | « Le manifeste annonce déjà une version plus récente ({M}) que celle du dépôt ({X}). » |
| `M` = `X` et release | arrêt | « La version X est déjà publiée. » |
| `M` = `X`, pas de release | arrêt | « La version X est annoncée mais sa release GitHub manque : relancez l'étape en échec de l'exécution d'origine. » |
| étiquette ou release, et `M` ≠ `X` (ou pas de manifeste) | arrêt | « L'étiquette vX existe mais le manifeste ne l'annonce pas : à examiner à la main. » |
| rien de tout cela | continuer | |

Doctrine de lecture : une ABSENCE ne se déduit jamais d'un texte d'erreur. Le manifeste se lit par code HTTP exact (200 + JSON dont `version` et `agent.version` sont `X.Y.Z` = lu ; 404 = aucune version publiée ; tout autre code, page HTML d'un défi Cloudflare, JSON d'une autre forme, `curl` en échec = illisible = arrêt). La release GitHub se lit par `GET /repos/Voikyrioh/hearth/releases/tags/vX.Y.Z` (200 avec le bon `tag_name` = présente, 404 = absente, tout le reste = arrêt) ; le jeton passe par l'entrée standard de `curl` (`--config -`), jamais par un argument. L'étiquette se lit par le code de sortie de `git ls-remote --exit-code --tags origin` (0 présente, 2 absente, autre = arrêt). Une lecture en échec (étiquette, release) arrête avec « L'étiquette vX n'a pas pu être cherchée ({cause}) : publication arrêtée. » ou « La release GitHub vX n'a pas pu être cherchée ({cause}) : publication arrêtée. ».

Une version déposée en partie sans manifeste n'est pas vue ici (le contrôle n'a aucune clé du bucket) : c'est BR-RELEASE-001 qui s'arrête au premier fichier différent.

## Application (code)
- `xtask/src/release_preflight.rs::published_state` : la table ci-dessus (pure).
- `xtask/src/release_preflight.rs::read_public_manifest` (`parse_manifest`), `::lookup_release`, `::lookup_tag` : les trois lectures, derrière le port `Host` (faux dans les tests).
- `xtask/src/release_preflight.rs::preflight` : l'ordre ; le manifeste illisible arrête avant de chercher l'étiquette et la release.

## Vérification
- `xtask/src/release_preflight.rs` : une ligne de la table par test (`nothing_published_and_no_tag_lets_the_release_through`, `an_unreadable_manifest_stops_everything`, `a_manifest_newer_than_the_repository_is_refused`, `an_announced_version_with_its_release_is_already_published`, `an_announced_version_without_its_release_says_to_rerun_the_original_run`, `a_tag_or_a_release_the_manifest_does_not_announce_is_an_anomaly`) ; lectures : `the_public_manifest_is_read_by_exact_http_code`, `anything_but_a_clean_200_or_404_is_unreadable_never_an_absence`, `the_manifest_is_asked_over_https_without_following_a_redirect`, `the_release_is_looked_up_by_http_code_with_the_token_on_standard_input`, `without_a_token_the_lookup_is_anonymous`, `a_release_lookup_that_is_not_a_clean_200_or_404_is_an_error`, `a_token_that_could_break_the_curl_config_is_refused_without_being_echoed`, `the_tag_is_looked_up_by_the_exit_code_of_ls_remote` ; orchestration : `the_published_state_decides_last_with_the_three_readings`, `an_unreadable_public_state_stops_before_asking_about_the_tag_and_the_release`, `a_tag_or_release_lookup_that_fails_stops_the_publication`.

## Cas limites
- Cloudflare devant `hearth.voikyrioh.fr` peut défier les adresses de centres de données (HRT-54, non vérifié) : un défi est « illisible », donc un arrêt, jamais une absence. Le premier essai à blanc du flux l'exerce.
- Compare des nombres (`0.10.0` > `0.9.0`), pas des textes.
- Une release en brouillon n'est pas vue par l'API des étiquettes : non traitée.

## Règles liées
- BR-RELEASE-001 (une version déposée ne se réécrit jamais) ; les règles de reprise après échec viennent avec HRT-174.

## Historique
- 2026-10-16 : création (HRT-172).
