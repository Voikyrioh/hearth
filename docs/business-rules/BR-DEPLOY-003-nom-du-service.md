---
id: BR-DEPLOY-003
domaine: DEPLOY
titre: Le nom est fait de lettres sans accent, de chiffres, de tirets et de points, commence par une lettre ou un chiffre, et fait de 2 à 63 caractères
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-deployer-image.md (BR-DEPLOY-003), HRT-68
maj: 2026-10-24
---

# BR-DEPLOY-003 — Le nom est fait de lettres sans accent, de chiffres, de tirets et de points, commence par une lettre ou un chiffre, et fait de 2 à 63 caractères

## Règle
Tout autre nom est refusé (`name`, `bad_format`) avant toute écriture.

Le minimum est 2 et non 1 : le nom du service est celui du conteneur, et le moteur refuse un nom de conteneur d'un seul caractère (`[a-zA-Z0-9][a-zA-Z0-9_.-]+`). Accepter `a` ferait télécharger l'image pour échouer à la création.

## Application (code)
- `crates/hearth-agent/src/domain/services/deploy.rs::validate_name`
- `apps/desktop/src/deploy/rules.ts::nameIssue` (client : même règle en saisie)

## Vérification
- `crates/hearth-agent/src/domain/services/deploy.rs::tests::the_name_is_letters_digits_dashes_and_dots_from_two_to_sixty_three`
- `apps/desktop/src/deploy/deploy.test.ts (le nom)`

## Cas limites
- Aucun cas particulier connu.

## Règles liées
BR-DEPLOY-004

## Historique
- 2026-10-17 — création (HRT-68, déployer une image, côté agent).
- 2026-10-19 — minimum porté à 2 caractères (refus du moteur), review de la PR #107.
- 2026-10-24 — côté client (HRT-69, déployer une image).
