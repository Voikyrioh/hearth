---
id: BR-FICHE-013
domaine: FICHE
titre: Une installation par fiche est un déploiement ordinaire : mêmes règles, mêmes gardes, mêmes limites obligatoires
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-013), HRT-101
maj: 2026-10-24
---

# BR-FICHE-013 — Une installation par fiche est un déploiement ordinaire

## Règle
La fiche, traduite en brouillon (`translate`), est jugée par le validateur du déploiement ordinaire (`OrdinaryDeployment`, les mêmes fonctions que le formulaire d'une image) avant de devenir la même description durable (`SpecBody`, `SpecSecrets`) et le même plan fermé. Les limites de mémoire et de processeur sont obligatoires comme pour une image à la main (`check_limits`, BR-DEPLOY-038). Les secrets que l'agent tire (mot de passe d'une console, par exemple) sont tirés par le port `SecretRandom` : alphabet `alnum`, longueur déclarée, octets biaisés rejetés, jamais de repli sur un hasard faible ; ils sont scellés sous l'identifiant `gen-<id>` des réponses (ADR-0044). Un déploiement par fiche compte dans la limite de deux déploiements simultanés.

## Application (code)
- `crates/hearth-agent/src/domain/services/from_fiche.rs::body_and_secrets_from`
- `crates/hearth-agent/src/domain/services/deploy.rs::OrdinaryDeployment`
- `crates/hearth-agent/src/domain/catalog/generate.rs::{secret_from_bytes, draw_secret}`

## Vérification
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_translated_fiche_becomes_the_description_of_an_ordinary_deployment`
- `crates/hearth-agent/src/domain/services/from_fiche.rs::a_request_without_memory_limit_is_refused`
- `crates/hearth-agent/src/domain/catalog/tests_translate.rs::property_every_accepted_fiche_and_valid_answers_give_a_closed_description`
- `crates/hearth-agent/src/domain/catalog/generate.rs::generated_secret_has_the_declared_length_and_alphabet_without_bias`
- `crates/hearth-agent/tests/fiche_install_https.rs::a_fiche_install_creates_a_service_with_the_fiche_digest_the_pinned_image_and_a_0600_root_config_file`
- `crates/hearth-agent/tests/fiche_install_secrets.rs::no_answer_value_nor_generated_secret_reaches_the_database_the_journal_the_traces_or_a_response`

## Cas limites
- Le chemin HTTP (droit, acte confirmé, limites, ports, description durable, création) est celui de `POST /deployments` : voir `tests/fiche_install_https.rs`. Le disque jugé est celui des images ET celui de la racine de données (BR-FICHE-023).

## Règles liées
- BR-FICHE-007, BR-FICHE-024, BR-DEPLOY-035, BR-DEPLOY-038

## Historique
- 2026-10-24 — création (HRT-101, étapes S3 et S5).
