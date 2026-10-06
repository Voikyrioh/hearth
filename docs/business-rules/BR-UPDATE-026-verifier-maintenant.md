---
id: BR-UPDATE-026
domaine: UPDATE
titre: « Vérifier maintenant » force une vérification des mises à jour
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-026), HRT-16
maj: 2026-10-06
---

# BR-UPDATE-026 : « Vérifier maintenant » force une vérification des mises à jour

## Règle
Le bouton « Vérifier maintenant » des réglages lance une vérification à tout moment, hors de la règle des 24 h (BR-UPDATE-001), et la fait compter comme la dernière tentative. Pendant la vérification le bouton est désactivé et dit « Vérification en cours… » ; une seule vérification à la fois. Résultat : « Tu es à jour » ou la version trouvée (avec « Notes de version » et « Mettre à jour maintenant »), et la date de la vérification ; sans réponse, aucune erreur, la date précédente reste (BR-UPDATE-007). Il n'installe jamais rien de lui-même. La vérification lit AUSSI la cible de l'agent (`agent.json`, ADR-0021), dans la même tentative : une requête de plus vers le même hôte, aucune tentative de plus.

## Application (code)
- `apps/desktop/src-tauri/src/update/commands.rs::check_for_updates`.
- `apps/desktop/src-tauri/src/update/service.rs::UpdateService::check_now`.
- `apps/desktop/src/components/organisms/UpdatePanel.vue`, `apps/desktop/src/stores/updates.ts::checkNow`.

## Vérification
- `apps/desktop/src-tauri/tests/update_service.rs` : `check_now_ignores_the_daily_limit_and_restarts_the_window`, `a_manual_check_without_internet_shows_no_error_either`.
- `apps/desktop/src/stores/updates.test.ts` : `one manual check at a time`, `'Vérifier maintenant' finds a release, or says the client is up to date` ; `apps/desktop/src/components/organisms/updates.test.ts` : `'Vérifier maintenant' is disabled while checking, with its own label`.
- `apps/desktop/e2e/updates.spec.ts` : « « Vérifier maintenant » sans nouvelle version… ».

## Cas limites
- Refusé si une autre opération (vérification, téléchargement, installation) est en cours : l'état courant est rendu.

## Règles liées
- BR-UPDATE-001, BR-UPDATE-007

## Historique
- 2026-10-05 : création (HRT-16, session 2026-10-04-hearth-creation).
- 2026-10-06 : lit aussi la cible de l'agent (HRT-17, lot interface, T28).
