---
id: BR-SVC-022
domaine: SVC
titre: Image sans nom : identifiant court plutôt qu'une version fausse
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-voir-les-services.md (BR-SVC-022), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lot-1.md §5, §7, §9, HRT-61
maj: 2026-10-13
---

# BR-SVC-022 — Image sans nom : identifiant court plutôt qu'une version fausse

## Règle
Quand l'image d'un service n'a plus de nom connu (le conteneur la désigne par son identifiant), `image.ref` est absent et `image.short_id` donne les 12 premiers caractères de l'identifiant : l'interface écrit « image sans nom {identifiant} » plutôt qu'une version fausse. Limite connue : le cas « dont le nom a été repris par une autre version » demande de comparer l'identifiant de l'image avec celui de sa référence et n'est pas traité par cette tranche.

## Application (code)
- `crates/hearth-agent/src/domain/services/image.rs::image_name`
- `crates/hearth-proto/src/api/services.rs::ImageInfo`

## Vérification
- `crates/hearth-agent/src/domain/services/image.rs::tests::an_image_known_only_by_its_id_has_no_name`
- `crates/hearth-agent/src/infrastructure/engine/translate.rs::tests::an_image_known_only_by_its_id_has_no_name`

## Historique
- 2026-10-13 — création (HRT-61, tranche 1 des déploiements : moteur détecté et liste en lecture seule).
