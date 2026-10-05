---
id: BR-UPDATE-016
domaine: UPDATE
titre: Une mise à jour annulée n'est pas relancée automatiquement
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-016), HRT-17
maj: 2026-10-05
---

# BR-UPDATE-016 : Une mise à jour annulée n'est pas relancée automatiquement

## Règle
Une mise à jour annulée (retour en arrière) ou échouée est un résultat, pas une intention : rien n'est mémorisé pour la relancer. L'agent qui revient annonce le résultat une fois (journal et flux) puis n'en fait rien ; seule une nouvelle demande d'un administrateur lance une nouvelle mise à jour.

## Application (code)
- `crates/hearth-agent/src/application/update.rs::UpdateService::{resume, report_pending, report}` : lit, annonce, marque « annoncé » ; ne lance jamais.

## Vérification
- `tests/update_use_cases.rs::a_rolled_back_update_is_announced_with_its_reason_and_journaled_as_failed` (aucun superviseur lancé après).

## Cas limites
- Le client ne relance pas non plus à la reconnexion (`hearth-link`).

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
