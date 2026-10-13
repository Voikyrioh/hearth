---
id: BR-ENGINE-001
domaine: ENGINE
titre: L'état du moteur est le premier élément de la section Services
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-moteur-de-conteneurs.md (BR-ENGINE-001), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-ENGINE-001 — L'état du moteur est le premier élément de la section Services

## Règle
L'état du moteur de conteneurs est le premier élément visible de la section Services, au-dessus de tout le reste ; la section n'est jamais un écran vide sans explication. L'agent fournit l'état dans tout instantané (`GET /services`, `services_snapshot`) : il n'y a jamais d'instantané sans état du moteur. Côté client (affichage, textes, boutons) : porté par HRT-62, hors de cette tranche.

## Application (code)
- `crates/hearth-proto/src/api/services.rs::ServicesState (le champ `engine` est obligatoire)`
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::snapshot`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::both_roles_get_the_same_engine_and_services_sorted_by_name_stopped_ones_included`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
