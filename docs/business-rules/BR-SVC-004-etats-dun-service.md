---
id: BR-SVC-004
domaine: SVC
titre: L'état d'un service est un des neuf états fermés, dérivé par une fonction pure
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-004), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-004 — L'état d'un service est un des neuf états fermés, dérivé par une fonction pure

## Règle
L'état d'un service est toujours un et un seul de neuf (liste fermée) : en marche, arrêté, démarrage, arrêt en cours, en erreur, redémarre en boucle, en pause, déploiement en cours, état non reconnu. Cette tranche produit en marche, arrêté, en erreur, redémarre en boucle, en pause, arrêt en cours (conteneur en suppression) et non reconnu ; démarrage et déploiement en cours arrivent avec les tranches 3 et 5. `derive_state` est une fonction pure.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::derive_state`
- `crates/hearth-agent/src/domain/services/state.rs::ServiceStateKind`
- `crates/hearth-proto/src/api/services.rs::ServiceState`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::derive_state_exit_codes`
- `crates/hearth-agent/src/domain/services/state.rs::tests::every_other_run_state_has_its_service_state`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
