---
id: BR-SVC-023
domaine: SVC
titre: Code de sortie fourni ; l'explication est portée par l'interface
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-023), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-023 — Code de sortie fourni ; l'explication est portée par l'interface

## Règle
Le code de sortie est accompagné d'une explication pour 0 (terminé normalement), 137 (coupé de force) et 143 (arrêt demandé) ; tout autre code est affiché seul. L'agent fournit `exit_code` (pour un service arrêté, en erreur ou qui redémarre) ; l'explication est portée par l'interface.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::derive_state (`exit_code`)`
- `crates/hearth-proto/src/api/services.rs::ServiceItem (exit_code)`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::derive_state_exit_codes`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
