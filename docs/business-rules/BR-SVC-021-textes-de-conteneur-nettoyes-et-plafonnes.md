---
id: BR-SVC-021
domaine: SVC
titre: Tout ce qui vient d'un conteneur est nettoyé et plafonné avant le fil
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-021), revue de la PR 77 (HRT-61)
maj: 2026-10-13
---

# BR-SVC-021 — Tout ce qui vient d'un conteneur est nettoyé et plafonné avant le fil

## Règle
Un nom, une image ou un chemin allant jusqu'à 255 caractères ne casse jamais la mise en page (la coupure avec « … » et le survol sont portés par l'interface). Côté agent, pour que ni un conteneur hostile ni un conteneur étrange ne gonfle les instantanés de tous les abonnés ni ne glisse un retour à la ligne chez le client : tout texte venu d'un conteneur ou du moteur est débarrassé des caractères de contrôle et de formatage invisibles (séparateurs de ligne et de paragraphe U+2028 et U+2029 et trait d'union conditionnel U+00AD compris) puis coupé (« … » à la fin) à une longueur plafonnée (nom 255, référence d'image 255, version d'image 128, chemin de montage 1024, projet Compose 255, nom et version du moteur 64) ; un service garde au plus 128 ports publiés et 64 montages (les premiers, dans l'ordre trié) ; le surplus est compté et porté par `ports_omitted` et `mounts_omitted` (champs facultatifs du service, absents quand 0). Les bornes sont des constantes du domaine.

## Application (code)
- `crates/hearth-agent/src/domain/services/limits.rs::bounded`
- `crates/hearth-agent/src/domain/services/limits.rs::MAX_PORTS`
- `crates/hearth-agent/src/domain/services/limits.rs::is_unsafe_char`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::observation_from`

## Vérification
- `crates/hearth-agent/src/domain/services/limits.rs::tests::a_long_text_is_cut_to_the_limit_with_a_mark`
- `crates/hearth-agent/src/domain/services/limits.rs::tests::control_characters_are_removed`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::everything_a_container_says_is_cleaned_and_bounded`
- `crates/hearth-agent/src/domain/services/limits.rs::tests::bidirectional_and_invisible_formatting_characters_are_removed`
- `crates/hearth-proto/src/api/services.rs::tests::the_omitted_counters_are_optional_both_ways`

## Cas limites
- Au-delà de 128 ports ou 64 montages, les suivants ne sont pas listés mais comptés : `ports_omitted`, `mounts_omitted` (« et N autres »).
- Les plages de ports contiguës ne sont pas regroupées.

## Historique
- 2026-10-13 — création (revue de la PR 77, HRT-61).
- 2026-10-13 — HRT-72 : compteurs des ports et montages omis (FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ) ; U+2028, U+2029, U+00AD retirés (FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ).
