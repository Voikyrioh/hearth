---
id: BR-UPDATE-002
domaine: UPDATE
titre: Aucune mise à jour du client n'est installée sans le clic de l'utilisateur
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-002), HRT-16
maj: 2026-10-05
---

# BR-UPDATE-002 : Aucune mise à jour du client n'est installée sans le clic de l'utilisateur

## Règle
Détecter une version plus récente n'installe, ni ne télécharge, rien. Le seul chemin vers le téléchargement puis l'installation est la commande `install_update`, appelée par « Mettre à jour maintenant » (ou « Réessayer » après un échec). Aucun appel réseau n'a lieu hors de la vérification prévue (BR-UPDATE-001, BR-UPDATE-026) et de ce clic ; l'adresse du flux est une constante de la compilation, la WebView ne fournit ni adresse ni chemin ni clé. (L'installation d'une mise à jour de l'AGENT par un administrateur est BR-UPDATE-011.)

## Application (code)
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::{begin_install, run_install}` : seule porte vers `Feed::download` et `Feed::install`.
- `apps/desktop/src-tauri/src/update/commands.rs::install_update` : seule commande qui l'appelle.
- `apps/desktop/src-tauri/capabilities/default.json` : aucune permission du greffon de mise à jour ; `build.rs` : liste blanche des commandes.

## Vérification
- `apps/desktop/src-tauri/tests/update_service.rs` : `nothing_is_installed_without_a_click`, `a_newer_release_shows_the_banner_with_its_notes`, `a_second_click_while_busy_is_refused_and_so_is_a_click_without_a_release`.
- `apps/desktop/src-tauri/tests/update_store.rs` : `the_web_view_has_no_permission_of_the_update_plugin`, `the_configuration_opens_no_bypass_and_fixes_no_address`.
- `apps/desktop/src-tauri/tests/update_feed.rs` : `the_manifest_is_read_and_announces_the_newer_version` (la vérification ne télécharge pas l'installateur).

## Cas limites
- Une annonce retenue d'une session précédente est relue sur clic (une requête de plus, permise : c'est l'action consentie) avant le téléchargement.

## Règles liées
- BR-UPDATE-001, BR-UPDATE-004, BR-UPDATE-011, ADR-0008, ADR-0017

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
