---
id: BR-FICHE-009
domaine: FICHE
titre: L'image d'une fiche n'est jamais saisie par l'utilisateur ; elle est épinglée par version, et par empreinte pour une fiche publiée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-009), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-009 — L'image d'une fiche n'est jamais saisie par l'utilisateur ; elle est épinglée par version, et par empreinte pour une fiche publiée

## Règle
Une fiche déclare un dépôt (minuscules, chiffres, `.`, `_`, `-`, `/`), une étiquette précise (jamais `latest` ni vide) et, pour une fiche `published`, une empreinte `sha256:` suivie de 64 chiffres hexadécimaux minuscules (`PublieeSansEmpreinte` sinon). Une variante ne change que l'étiquette (et l'empreinte) du MÊME dépôt : le format n'a pas de champ pour un autre dépôt.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::{check_image, check_variants}`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i1_one_way_closed_plan_refuses_image_and_port_traps`

## Cas limites
- L'empreinte de l'image de Terraria n'est pas vérifiée : aucune n'est inventée, la fiche reste `preview` (à relever par le job `fiche-golden`, HRT-114).

## Règles liées
- BR-FICHE-007, BR-FICHE-006

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
