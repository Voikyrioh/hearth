---
id: BR-ENGINE-014
domaine: ENGINE
titre: Le pilotage n'est possible que si le moteur est prêt
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-014), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-014 — Le pilotage n'est possible que si le moteur est prêt

## Règle
Les actions de pilotage (démarrer, arrêter, redémarrer, déployer) ne sont possibles que si le moteur est prêt ; dans tout autre état elles sont désactivées avec la raison. Cette tranche est en lecture seule et n'a aucune action : l'agent fournit l'état du moteur qui la décidera, et le client le lit pour désactiver ses boutons (tranches suivantes). Les refus d'une action quand le moteur n'est pas prêt (`ENGINE_UNAVAILABLE`) arrivent avec la tranche 3.

## Application (code)
- `crates/hearth-agent/src/domain/services/engine.rs::EngineState`
- `crates/hearth-proto/src/api/services.rs::EngineState`

## Vérification
- `crates/hearth-agent/src/domain/services/engine.rs::tests::classify_probe_covers_every_state`

## Cas limites
- Aucune route d'action n'existe : `GET /services` et `POST /engine/recheck` sont les seules routes de la tranche.

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
