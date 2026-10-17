---
id: BR-DEPLOY-003
domaine: DEPLOY
titre: Le nom est fait de lettres sans accent, de chiffres, de tirets et de points, commence par une lettre ou un chiffre, et fait de 1 à 63 caractères
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-003), HRT-68
maj: 2026-10-17
---

# BR-DEPLOY-003 — Le nom est fait de lettres sans accent, de chiffres, de tirets et de points, commence par une lettre ou un chiffre, et fait de 1 à 63 caractères

## Règle
Tout autre nom est refusé (`name`, `bad_format`) avant toute écriture.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::validate_name`

## Vérification
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::the_name_is_letters_digits_dashes_and_dots_from_one_to_sixty_three`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-004

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
