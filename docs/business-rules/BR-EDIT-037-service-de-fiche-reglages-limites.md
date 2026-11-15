---
id: BR-EDIT-037
domaine: EDIT
titre: Pour un service de fiche, seule l'adresse des joueurs se change
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-modifier-un-service.md (BR-EDIT-037), HRT-87
maj: 2026-11-14
---

# BR-EDIT-037 — Pour un service de fiche, seule l'adresse des joueurs se change

## Règle
Pour un service de fiche, seule l'adresse des joueurs se change ; une réponse figée est refusée `PLAN_SETTING_REFUSED` (`frozen`), tout autre changement `fiche`. Les réponses non figées attendent la traduction de la fiche (suivi).

## Application (code)
- `crates/hearth-agent/src/domain/services/update.rs::apply_to_fiche`

## Vérification
- `crates/hearth-agent/src/domain/services/update.rs::tests::a_game_service_only_takes_the_public_address`
- `crates/hearth-agent/src/domain/services/update.rs::tests::a_frozen_answer_is_refused_with_its_own_reason`

## Cas limites
- Les textes affichés et les boutons sont ceux du client (HRT-88).

## Règles liées
- BR-EDIT-035, BR-EDIT-024

## Historique
- 2026-11-14 — création (HRT-87, modifier un service par recréation, côté agent).
