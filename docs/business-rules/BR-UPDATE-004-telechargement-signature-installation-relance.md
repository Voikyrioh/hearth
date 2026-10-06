---
id: BR-UPDATE-004
domaine: UPDATE
titre: La mise à jour est téléchargée, sa signature vérifiée, puis installée et le client se relance
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-004), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-004 : La mise à jour est téléchargée, sa signature vérifiée, puis installée et le client se relance

## Règle
« Mettre à jour maintenant » télécharge l'installateur EN MÉMOIRE, vérifie sa signature minisign (Ed25519) contre la clé publique EMBARQUÉE dans le client (`update-key.pub`), et seulement alors l'écrit dans le dossier temporaire et le lance (NSIS par utilisateur, mode passif, relance du client). Rien n'est écrit sur le disque ni exécuté avant une signature valide ; une signature d'une autre clé, ou absente, ou un fichier modifié, refuse la mise à jour (BR-UPDATE-010). La clé n'est jamais lue sur le disque du PC. HTTPS seulement : à chaque saut de redirection aussi (au plus 5, sans liste d'hôtes), et le greffon refuse tout flux non HTTPS dans un binaire de publication. Pas de rétrogradation : une version égale ou inférieure est ignorée (BR-UPDATE-003).

## Application (code)
- `apps/desktop/src-tauri/src/update/feed.rs::TauriFeed::{check, download, install}` : greffon `tauri-plugin-updater` (API Rust seule, aucune permission web), clé `EMBEDDED_PUBLIC_KEY`.
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::run_install` : téléchargement, puis `Feed::install` ; progression publiée par pourcentage.
- `apps/desktop/src-tauri/src/update/domain.rs::DownloadPolicy` : source de l'installateur (github.com, dépôt public, HTTPS).

## Vérification
- `apps/desktop/src-tauri/tests/update_feed.rs` : `a_download_with_a_valid_signature_is_returned_with_its_progress`, `a_signature_from_another_key_is_refused_as_corrupted`, `a_garbage_signature_is_refused_as_corrupted`, `end_to_end_check_then_click_installs_only_a_verified_file`, `end_to_end_with_a_different_key_embedded_nothing_ever_verifies`, `nothing_is_downloaded_for_a_version_that_was_not_announced`, `a_redirect_of_the_installer_to_plain_http_is_not_followed`, `the_installer_follows_exactly_the_maximum_number_of_redirects`, `the_manifest_follows_the_same_bound_and_works_behind_githubs_two_redirects`, `a_redirect_to_another_host_over_the_allowed_scheme_is_followed`, `a_release_made_by_the_publication_code_is_accepted_by_the_real_plugin`, `redirects_are_https_at_every_rank_bounded_exactly_and_not_limited_to_hosts` (domaine), `an_announcement_from_a_source_the_policy_refuses_is_never_staged`.
- `apps/desktop/src-tauri/tests/update_service.rs` : `a_click_downloads_reports_progress_then_installs`.
- Non vérifié sans vraie publication : le lancement réel de l'installateur NSIS et la relance (voir le runbook, § « Premier essai »).

## Cas limites
- Le téléchargement est tenu en mémoire, sans plafond de taille côté client ; la signature n'est vérifiée qu'après le téléchargement complet. Limites : source du premier saut restreinte à GitHub en HTTPS, redirections en HTTPS à chaque saut et au plus 5 (`DownloadPolicy::allows_redirect`), sans liste d'hôtes sur les sauts suivants (compromis écrit dans ADR-0017), délai de 15 min.
- Tant que le dépôt porte la clé de développement (sans clé secrète), AUCUNE mise à jour n'est acceptée : c'est voulu (runbook `publier-une-version-du-client`).

## Règles liées
- BR-UPDATE-002, BR-UPDATE-003, BR-UPDATE-010, ADR-0008, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
