---
id: BR-FICHE-036
domaine: FICHE
titre: Hearth ne dit jamais d'un serveur de jeu installé par fiche qu'il est « isolé », « sûr », « protégé » ou « vérifié »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-036), HRT-101
maj: 2026-10-24
---

# BR-FICHE-036

## Règle
Règle de vocabulaire, portée par le client (HRT-102) : la seule phrase sur la confiance est « Cette fiche a été écrite par la personne qui publie Hearth. Elle fait tourner une image publique sur ton serveur, avec les droits d'un service ordinaire. ». Côté agent, ce que la fiche peut demander est borné par le format fermé (BR-FICHE-007) et par le plan fermé du conteneur (ADR-0041).

## Application (code)
- `crates/hearth-agent/src/domain/services/plan.rs::plan_from_spec`
- `apps/desktop/src/components/organisms/FicheSummaryDialog.vue`
- `apps/desktop/src/pages/FicheDetail.vue`
- `apps/desktop/src/fiche/copy.test.ts`

## Vérification
- `crates/hearth-agent/src/domain/services/plan.rs::the_plan_has_no_field_for_the_options_a_privileged_service_would_need`
- `apps/desktop/src/fiche/copy.test.ts`
- `apps/desktop/src/pages/FicheDetail.test.ts::dit la phrase de confiance, jamais « isolé » ni « sûr »`

## Cas limites
- Le vocabulaire est au client (HRT-102).

## Règles liées
- BR-FICHE-007, BR-GRANT-022

## Historique
- 2026-10-24 — création (HRT-101).
- 2026-10-24 — côté client (HRT-102).
