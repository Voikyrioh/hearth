---
id: BR-LIFE-001
domaine: LIFE
titre: Démarrer, arrêter et redémarrer un service est réservé aux comptes qui ont le droit de gérer les services, quel que soit leur rôle
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-001 — Démarrer, arrêter et redémarrer un service est réservé aux comptes qui ont le droit de gérer les services, quel que soit leur rôle

## Règle
Les trois actes sont réservés aux comptes qui ont le droit « gérer les services » (BR-GRANT-003), administrateurs ou non. Le refus est jugé AVANT que la clé du chemin soit lue (même réponse que le service existe ou non), par la couche d'accès (niveau `Services` des trois lignes de `ENDPOINTS`) : `403 FORBIDDEN_ROLE` avec `details.missing = services`, « Tu n'as pas le droit de gérer les services de ce serveur. ». Le refus est inscrit au journal d'activité (action `service.start`, `service.stop` ou `service.restart`, résultat « Refusé », cause « droits insuffisants »). Aucun ordre n'est donné au moteur.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (lignes `/services/{key}/start|stop|restart`, `Access::Services`)`
- `crates/hearth-agent/src/entrypoint/http/auth.rs (refus de droit, `Reason::InsufficientRights`)`

## Vérification
- `crates/hearth-agent/tests/service_lifecycle_https.rs::only_an_account_with_the_right_may_drive_a_service_whatever_its_role`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::every_route_is_classified_and_every_service_route_is_at_the_services_level`

## Cas limites
- Le droit est relu à CHAQUE requête : retiré en cours d'action, l'action va à son terme, la demande suivante est refusée (BR-LIFE-025).

## Règles liées
- BR-GRANT-003, BR-GRANT-004, BR-LIFE-016, BR-LIFE-025

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
