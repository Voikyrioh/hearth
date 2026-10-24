---
id: BR-DEL-022
domaine: DEL
titre: Le moteur doit être prêt pour supprimer
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-supprimer-service.md (BR-DEL-022), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md §5 et §9, HRT-85
maj: 2026-10-24
---

# BR-DEL-022 : Le moteur doit être prêt pour supprimer

## Règle
Tant que le moteur n'est pas « Prêt », la fenêtre et l'acte sont refusés (`409 ENGINE_UNAVAILABLE` avec l'état du moteur, « Le service n'a pas été supprimé. ») : sans le moteur, l'agent ne peut pas prouver qu'un conteneur est absent. Si le moteur devient inaccessible après l'acceptation, le résultat est inconnu (BR-DEL-015).

## Application (code)
- `crates/hearth-agent/src/application/delete.rs::ServiceDeletion::subject`

## Vérification
- `crates/hearth-agent/src/application/delete.rs` (l'état du moteur est jugé en premier)

## Cas limites
- Aucun.

## Règles liées
- BR-DEL-015

## Historique
- 2026-10-24 : création (HRT-85, côté agent).
