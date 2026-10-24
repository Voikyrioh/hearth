---
id: BR-FICHE-058
domaine: FICHE
titre: Hearth n'accepte jamais un contrat à la place de l'utilisateur ; l'agent refuse l'installation sans acceptation et consigne qui a accepté et quand
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-058), HRT-101
maj: 2026-10-24
---

# BR-FICHE-058 — Contrat de licence

## Règle
Une fiche qui exige un contrat le déclare ; aucune fiche, variante, valeur proposée ni réglage ne pose la valeur d'acceptation (invariant I8). La traduction refuse (`AgreementRequired`) toute demande qui n'a pas accepté chaque contrat, même venue d'un client modifié, et la description garde pour chaque contrat accepté son identifiant, le compte qui l'a accepté et l'instant (`SpecAgreement`).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/translate.rs::translate`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::body_and_secrets_from`
- `crates/hearth-agent/src/domain/services/spec.rs::SpecAgreement`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i8_a_contract_is_accepted_only_by_a_gesture`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_translated_fiche_becomes_the_description_of_an_ordinary_deployment`

## Cas limites
- Le refus HTTP `AGREEMENT_REQUIRED` (liste des identifiants) arrive avec l'étape d'installation de HRT-101.

## Règles liées
- BR-FICHE-007, BR-FICHE-029

## Historique
- 2026-10-24 — création (HRT-101, étapes S2 et S3).
