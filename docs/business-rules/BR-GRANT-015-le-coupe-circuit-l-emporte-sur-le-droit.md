---
id: BR-GRANT-015
domaine: GRANT
titre: Le coupe-circuit du serveur (`engine.enabled`) l'emporte sur le droit
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-015), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-015 — Le coupe-circuit du serveur (`engine.enabled`) l'emporte sur le droit

## Règle
Si le pilotage des services est désactivé dans la configuration de l'agent (`[engine] enabled = false` ou `HEARTH_ENGINE=off`, `BR-ENGINE-021`), même un compte qui a le droit n'obtient aucune action ni aucune valeur : aucun service n'est publié, le socket du moteur n'est jamais ouvert, et `ServicesRegistry::reveal_env` refuse d'entrée sans toucher au moteur (`404`, comme pour un service inconnu). Désactiver le pilotage ne retire pas le droit du compte : le droit reste en base, il redevient utilisable dès que le pilotage est rétabli.

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::reveal_env (`!self.enabled`)`
- `crates/hearth-agent/src/infrastructure/config/mod.rs::EngineConfig`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::the_kill_switch_wins_over_the_right_with_two_values_of_the_setting`
- `crates/hearth-agent/tests/services_https.rs::a_disabled_engine_is_told_as_disabled_and_the_socket_is_never_opened`

## Cas limites
- Le test couvre la seule action de service existante (révéler). Chaque acte à venir doit lui aussi refuser d'entrée quand le pilotage est désactivé (`BR-ENGINE-014`).

## Règles liées
- BR-ENGINE-021, BR-GRANT-003

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
