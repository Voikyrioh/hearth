---
id: BR-FICHE-038
domaine: FICHE
titre: Les questions de l'installation sont exactement les réglages essentiels, huit au plus, chacun avec une valeur proposée
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-038), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-038 — Les questions de l'installation sont exactement les réglages essentiels, huit au plus, chacun avec une valeur proposée

## Règle
Une fiche ne compte pas plus de huit réglages de niveau `essential` (`TropDEssentiels`). Un réglage imposé n'est jamais `essential`. Terraria en pose six : nom du monde, mot de passe, message d'accueil, nombre de joueurs, taille du monde, difficulté.

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::check_settings (compte des essentiels)`
- `crates/hearth-agent/src/domain/catalog/settings_state.rs::essential_questions`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::coherence_the_settings_of_a_fiche_agree_with_each_other (neuf essentiels refusés, imposé essentiel refusé)`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::the_questions_of_an_installation_are_eight_at_most_and_a_secret_shows_no_value`

## Cas limites
- La question de la variante ne compte pas parmi les huit (elle n'est pas un réglage).

## Règles liées
- BR-FICHE-037

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
