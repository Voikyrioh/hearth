---
id: BR-FICHE-029
domaine: FICHE
titre: La description du service garde la fiche, sa version, son empreinte, la variante, les réponses (les secrètes chiffrées), les questions figées et les contrats acceptés
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-029), HRT-101
maj: 2026-10-24
---

# BR-FICHE-029 — Lien fiche-service

## Règle
Le service créé depuis une fiche est un service ordinaire. Sa description (`SpecBody.fiche`) garde la fiche, sa version, son empreinte, la variante choisie, les réponses (valeur en clair seulement si la question n'est pas secrète ; une réponse secrète est scellée dans `SpecSecrets.answers`, ADR-0044), les questions figées à la création du monde (BR-EDIT-035) et les contrats acceptés (qui, quand). Ce lien est conservé tel quel : une fiche qui change ou disparaît ne modifie pas le service. Les champs ajoutés sont facultatifs : une révision plus ancienne reste lisible. Stockage dans `spec_json` (aucune migration).

## Application (code)
- `crates/hearth-agent/src/domain/services/spec.rs::{SpecFiche, SpecAnswer, SpecAgreement}`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::body_and_secrets_from`

## Vérification
- `crates/hearth-agent/src/domain/services/spec.rs::a_fiche_description_keeps_fiche_version_digest_answers_agreements_and_frozen_questions`
- `crates/hearth-agent/tests/service_spec_secrets.rs::a_fiche_description_survives_the_database_with_its_answers_and_agreements`

## Cas limites
- Une réponse secrète avec une valeur en clair est refusée à l'écriture (`SpecError::Fiche`).

## Règles liées
- BR-EDIT-035, BR-FICHE-058

## Historique
- 2026-10-24 — création (HRT-101, étape S2).
