---
id: BR-SVC-005
domaine: SVC
titre: « En erreur » : code de sortie non nul sans arrêt demandé par Hearth
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-005), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-005 — « En erreur » : code de sortie non nul sans arrêt demandé par Hearth

## Règle
Un service est « en erreur » quand il est arrêté avec un code de sortie non nul sans que l'arrêt ait été demandé depuis Hearth ; arrêté avec le code zéro, ou arrêté par une demande Hearth, il est « arrêté ». Le code du dernier arrêt est fourni dans les deux cas. Dans cette tranche (aucune action), l'arrêt n'est jamais « demandé » : `derive_state` reçoit `stop_requested = false` ; l'argument sert aux tranches suivantes.

## Application (code)
- `crates/hearth-agent/src/domain/services/state.rs::derive_state`

## Vérification
- `crates/hearth-agent/src/domain/services/state.rs::tests::derive_state_exit_codes`

## Cas limites
- 137 sans arrêt demandé : « en erreur » (arrêt forcé ou manque de mémoire).

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
