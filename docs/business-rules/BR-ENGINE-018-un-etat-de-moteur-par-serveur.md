---
id: BR-ENGINE-018
domaine: ENGINE
titre: Un état de moteur par serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-018), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-018 — Un état de moteur par serveur

## Règle
Chaque serveur du carnet a son propre état du moteur, indépendant des autres : l'état est tenu par l'agent de ce serveur (un registre par agent, en mémoire, reconstruit au démarrage). Le client le range par serveur (HRT-62).

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::rev_grows_with_every_change_and_starts_again_with_the_agent`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
