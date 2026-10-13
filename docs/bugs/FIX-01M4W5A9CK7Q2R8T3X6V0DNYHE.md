---
id: FIX-01M4W5A9CK7Q2R8T3X6V0DNYHE
titre: Le décodage d'un segment de chemin acceptait une forme que le routeur ne lit pas (« %+f ») pour la clé d'un service relâché
date_découverte: 2026-10-13
date_correction: 2026-10-14
---

# FIX-01M4W5A9CK7Q2R8T3X6V0DNYHE : `decode_segment` accepte une forme malformée (HRT-80, revue finale de la PR 84, suivi 5)

## Symptôme
`hearth_proto::admin_act::decode_segment("a%+fb")` rendait `a\u{f}b` : la couche de confirmation de l'agent décodait `%+f` en l'octet `0x0F`, alors que le routeur laisse la séquence telle quelle. « Décode comme le routeur » n'était donc pas strictement vrai, et la liaison cliente (même fonction) acceptait de construire un acte pour un tel segment.

## Reproduction
`hearth-proto` : `admin_act::tests::a_path_segment_is_decoded_and_a_key_has_its_shape` (cases `%+f`, `%-1`, `% 5`, `%%41`...), `hearth-link` : `domain::act::tests` (chemins `u_a%+f.service`, `u_a%-1.service`, `u_a%%41.service` : requête libre), `hearth-agent` : `service_adoption_https::a_unit_with_an_escaped_name_is_adopted_and_released_through_its_encoded_key`.

## Cause root
La paire de chiffres était lue par `u8::from_str_radix(pair, 16)`, qui accepte un signe (`+f`, et `+` seul puis un chiffre).

## Impacté
Aucune conséquence observée : les deux lectures donnent une clé invalide, donc rien ne se passe. Défaut d'accord entre la couche et le routeur, fermé avant qu'un écran n'appelle l'acte.

## Workaround
Aucun.

## Correction
`decode_segment` lit chaque chiffre par `hex_digit` (ASCII `0-9 a-f A-F`) et exige deux chiffres après chaque `%` ; tout le reste rend `None`. Source unique de l'agent et de la liaison : un seul correctif. Le banc de test de l'agent signe le texte tel quel quand l'encodage est illisible.

## Références
- Ticket : HRT-80 (suivi de la revue finale de la PR 84)
- BR : BR-ADOPT-013
