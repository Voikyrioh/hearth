---
id: BR-FICHE-011
domaine: FICHE
titre: Une fiche ne demande jamais un identifiant de compte ; un jeton de serveur est permis comme réglage secret
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-installer-serveur-de-jeu.md (BR-FICHE-011), contexts/hearth/conceptions/2026-10-13-technique-fiches-de-jeu.md, HRT-113
maj: 2026-10-13
---

# BR-FICHE-011 — Une fiche ne demande jamais un identifiant de compte ; un jeton de serveur est permis comme réglage secret

## Règle
Aucun type de réglage ne représente le nom d'utilisateur ou le mot de passe d'une plateforme. Un secret saisi a un genre fermé (`game_password`, `admin_password`, `server_token`) : un jeton de serveur est révocable et ne donne pas accès au compte. Un secret n'a ni valeur proposée ni défaut non vide, n'est jamais passé en argument de commande, et sa valeur est un `Secret` (jamais dans un `Debug`).

## Application (code)
- `crates/hearth-agent/src/domain/catalog/loading/schema/settings.rs::SecretKind`
- `crates/hearth-agent/src/domain/catalog/loading/loader.rs::check_values (secret sans valeur)`
- `crates/hearth-agent/src/domain/catalog/loading/invariants.rs::check_arg_setting (jamais en argument)`

## Vérification
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::coherence_the_settings_of_a_fiche_agree_with_each_other (secret avec valeur proposée)`
- `crates/hearth-agent/src/domain/catalog/tests_fiches.rs::i3_no_text_reaches_an_interpreter_arguments_orders_and_derived_values (secret en argument)`
- `crates/hearth-agent/src/domain/catalog/tests_state.rs::a_secret_is_sealed_never_kept_in_the_state_or_in_a_debug_output`

## Cas limites
- Le scellement et l'absence de la valeur dans la base, les traces et le journal d'activité sont posés par HRT-101 (test sentinelle).

## Règles liées
- BR-FICHE-007

## Historique
- 2026-10-13 — création (HRT-113, format de fiche version 2, domaine pur).
