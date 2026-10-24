---
id: BR-DEL-003
domaine: DEL
titre: Un service se supprime en marche, arrêté, en erreur ou en redémarrage en boucle
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-003), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-003 : Un service se supprime en marche, arrêté, en erreur ou en redémarrage en boucle

## Règle
Les états « En marche », « Arrêté », « En erreur » et « Redémarre en boucle » permettent la suppression. Dans tout autre état (démarrage, arrêt en cours, déploiement en cours, en pause, état non reconnu) l'agent répond `409 SERVICE_NOT_DRIVABLE` avec l'état en détail et rien n'est touché. Une autre action en cours sur le service (BR-DEL-021) est jugée avant l'état.

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::{deletable, ServiceDeletion::delete}`

## Vérification
- `crates/hearth-agent/tests/service_delete_https.rs::a_paused_service_is_refused_with_its_reason_and_nothing_is_touched`

## Cas limites
- Un service dont le conteneur a disparu n'a pas d'état : il reste supprimable (BR-DEL-018).

## Règles liées
- BR-DEL-021
- BR-DEL-018

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
