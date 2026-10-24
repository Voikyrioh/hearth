---
id: BR-FICHE-054
domaine: FICHE
titre: Seul un service créé par une fiche a des réglages de jeu ; l'onglet suit la capacité de l'agent et la lecture à l'ouverture dit le reste
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-054), HRT-109
maj: 2026-10-24
---

# BR-FICHE-054

## Règle
L'onglet « Réglages » n'existe que pour un service créé par Hearth depuis une fiche que l'agent propose. Le client le montre quand l'agent annonce la capacité `game-settings` (`GET /hello`, lue avec le droit « gérer les services »), que le compte a ce droit et que le service est créé par Hearth, ni adopté ni en cours de déploiement ; la vue n'est lue qu'à l'OUVERTURE de l'onglet, jamais pour dessiner la liste des onglets. La liste des services ne dit pas encore qu'un service est né d'une fiche (un repère léger est suivi par HRT-197) : un service déployé à la main a donc l'onglet, qui lui dit « Ce service n'a pas de réglages de jeu. » parce que l'agent répond `404 NOT_A_GAME_SERVICE` (code distinct de `SERVICE_NOT_FOUND`, clé mal formée, et de `NOT_FOUND`, route qu'un agent plus ancien ne sert pas). Un service adopté n'a jamais l'onglet.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/game_settings.rs::not_a_game_service`, `crates/hearth-proto/src/error.rs::ErrorCode::NotAGameService`
- `crates/hearth-link/src/domain/services.rs::agent_serves_game_settings`, `manager/rights.rs::ServiceRightsRead::game_settings`
- `apps/desktop/src-tauri/src/service_ops/game_settings.rs::{read, has_no_settings}`
- `apps/desktop/src/components/organisms/ServiceDetail.vue` (la liste des onglets), `apps/desktop/src/composables/useServiceRights.ts::gameSettings`

## Vérification
- `crates/hearth-agent/tests/game_settings_https.rs::reading_needs_the_right_to_manage_services_and_a_stranger_key_is_unknown`
- `apps/desktop/src-tauri/src/service_ops/game_settings.rs::tests::only_a_service_that_is_not_a_game_server_or_an_old_agent_has_no_settings`
- `apps/desktop/src/components/organisms/gameSettingsTab.test.ts::l'onglet`

## Cas limites
- Un agent plus ancien (sans la capacité `game-settings`) : l'onglet n'apparaît pas ; s'il répondait `404 NOT_FOUND` à la route, le client le lit aussi « pas de réglages ».
- Une clé mal formée (`SERVICE_NOT_FOUND`) est une erreur, jamais « pas de réglages ».

## Règles liées
BR-FICHE-041

## Historique
- 2026-10-24 — création (HRT-109).
