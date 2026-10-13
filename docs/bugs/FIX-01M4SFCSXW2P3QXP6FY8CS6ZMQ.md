---
id: FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ
titre: Textes de conteneur : séparateurs de ligne Unicode non retirés, ports et montages omis non comptés
date_découverte: 2026-10-13
date_correction: 2026-10-13
---

# FIX-01M4SFCSXW2P3QXP6FY8CS6ZMQ : bornes des textes et des listes d'un conteneur (HRT-72)

Ticket : HRT-72. Sévérité : mineur. Zones : `domain/services/limits.rs::is_unsafe_char`, `infrastructure/engine/translate.rs` (`ports_of`, `observation_from`), `entrypoint/services_wire.rs::service_item`.

## Symptôme
- U+2028, U+2029 (séparateurs de ligne et de paragraphe, hors catégorie « contrôle ») et U+00AD passaient : un nom de conteneur pouvait encore casser une ligne d'affichage.
- Au-delà de 128 ports ou 64 montages, les suivants disparaissaient sans trace (BR-SVC-021) : rien n'autorisait « et N autres ».

## Reproduction
`limits::tests::bidirectional_and_invisible_formatting_characters_are_removed` (cas U+2028, U+2029, U+00AD) et `translate::tests::everything_a_container_says_is_cleaned_and_bounded` (compteurs 2872 et 436). Rouges avant (les caractères restaient ; les champs n'existaient pas).

## Cause root
`is_unsafe_char` ne listait que les contrôles et les formatages invisibles connus ; le plafond `take(MAX_*)` jetait le surplus sans le compter.

## Impacté
Le fil `services`, jamais lu par un client publié.

## Workaround
Aucun.

## Correction
Trois caractères ajoutés au filtre. `ports_omitted` et `mounts_omitted` (u32, `serde(default)`, absents quand 0) portent le surplus de `ServiceItem`. Rien renommé ni retiré.

## Règles
- BR-SVC-021 (mise à jour).
