---
id: BR-ADOPT-007
domaine: ADOPT
titre: Un acte d'adoption porte 1 à 50 clés, sans doublon ; au-delà, il est refusé avec « Tu peux adopter 50 services à la fois. »
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-007), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-007 — Un acte d'adoption porte 1 à 50 clés, sans doublon ; au-delà, il est refusé avec « Tu peux adopter 50 services à la fois. »

## Règle
`POST /services/adopt` refuse (`422 VALIDATION_ERROR`, champ `keys`) une liste vide, une liste de plus de 50 clés (`MAX_ADOPTIONS`, texte « Tu peux adopter 50 services à la fois. ») et une clé en double. Rien n'est adopté. « Tout cocher » ne coche que les candidats affichés : c'est un comportement du client ; l'agent ne connaît que la liste exacte qu'on lui envoie. Une clé mal formée ou inconnue n'est pas une erreur de validation : elle a l'issue `not_found` (BR-ADOPT-010).

## Application (code)
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption::adopt`
- `crates/hearth-agent/src/entrypoint/http/services.rs::adoption_error`
- `crates/hearth-proto/src/api/services.rs::MAX_ADOPTIONS`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::too_many_keys_duplicates_and_a_target_that_does_not_match_the_keys_adopt_nothing`

## Règles liées
- BR-ADOPT-008, BR-ADOPT-010, BR-ADOPT-021

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
