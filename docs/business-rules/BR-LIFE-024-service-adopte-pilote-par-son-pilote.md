---
id: BR-LIFE-024
domaine: LIFE
titre: Pour un service adopté, le pilotage passe par son pilote (le système s'il a une unité, le moteur sinon) et exige le droit comme pour tout service ; un conteneur non adopté n'est pas pilotable
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-cycle-de-vie-service.md (BR-LIFE-024), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md, HRT-64
maj: 2026-10-17
---

# BR-LIFE-024 — Pour un service adopté, le pilotage passe par son pilote (le système s'il a une unité, le moteur sinon) et exige le droit comme pour tout service ; un conteneur non adopté n'est pas pilotable

## Règle
Seuls les services PUBLIÉS se pilotent (`ServicesRegistry::lifecycle_target`) : créés par Hearth ou adoptés (BR-ADOPT-012). Une demande sur un conteneur non adopté reçoit « Ce service n'existe plus. » (BR-ADOPT-004), identique à celle d'une clé inconnue. Le pilote est fixé par la clé : `u_<unité>` → le système, tout autre → le moteur. Le coupe-circuit (pilotage désactivé) prime : aucune cible, le moteur n'est jamais ouvert (BR-GRANT-015).

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::lifecycle_target`

## Vérification
- `crates/hearth-agent/src/application/services.rs::tests::only_a_published_service_has_a_lifecycle_target_and_the_pilot_follows_the_key`
- `crates/hearth-agent/tests/service_lifecycle_https.rs::an_unknown_a_malformed_and_a_hidden_service_are_all_answered_the_same_way`

## Cas limites
- Aucun cas limite propre à cette règle au-delà de ceux de la règle.

## Règles liées
- BR-ADOPT-004, BR-ADOPT-012, BR-GRANT-015, BR-LIFE-002

## Historique
- 2026-10-17 — création (HRT-64 : démarrer, arrêter, redémarrer un service, côté agent).
