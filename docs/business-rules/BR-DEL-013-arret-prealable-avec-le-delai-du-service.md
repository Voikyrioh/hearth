---
id: BR-DEL-013
domaine: DEL
titre: Un service qui tourne est arrêté d'abord, avec son délai de grâce, jamais coupé en silence
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-013), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-013 : Un service qui tourne est arrêté d'abord, avec son délai de grâce, jamais coupé en silence

## Règle
Avant d'être supprimé, un service « En marche » ou « Redémarre en boucle » est arrêté : l'arrêt voulu est écrit AVANT l'ordre (BR-LIFE-010), l'arrêt est propre avec le délai de grâce du service, l'attente est bornée (BR-LIFE-021). Une coupure forcée est dite (cause « arrêt forcé » dans la mention du journal d'activité). La suppression du conteneur ne force jamais (`remove_keeping_volumes`, sans `force`) : un conteneur reparti est refusé, pas tué. Pour un serveur de jeu, l'enchaînement avec l'enregistrement du monde n'est pas promis par la suppression.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::stop`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::a_running_service_is_stopped_with_its_grace_before_the_container_goes`
- `crates/hearth-agent/tests/support/engine_contract.rs::deploys` (la suppression sans force refuse un conteneur qui tourne)

## Cas limites
- Hearth ne pilote que le moteur pour un service créé par Hearth : jamais d'unité systemd ici.

## Règles liées
- BR-LIFE-010
- BR-LIFE-021
- BR-DEL-014

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
