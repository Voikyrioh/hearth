---
id: BR-ADOPT-023
domaine: ADOPT
titre: Les compteurs du moteur ne comptent que les services publiés ; leur total égale le nombre de lignes de la liste
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-023), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-79
maj: 2026-10-13
---

# BR-ADOPT-023 — Les compteurs du moteur ne comptent que les services publiés ; leur total égale le nombre de lignes de la liste

## Règle
Les compteurs du moteur (`BR-ENGINE-004`) ne comptent que les services publiés : l'agent n'envoie aucun compteur par catégorie, l'interface les calcule sur la liste, qui ne contient que des services publiés ; leur total égale donc le nombre de lignes. Les conteneurs non gérés ne sont dans aucune catégorie : ils ont leur compte propre (`engine.external_count`, `BR-ADOPT-002`).

## Application (code)
- `crates/hearth-agent/src/application/services.rs::ServicesRegistry::publish`
- `crates/hearth-agent/src/entrypoint/services_wire.rs::snapshot`

## Vérification
- `crates/hearth-agent/tests/services_https.rs::on_a_server_whose_containers_are_all_outside_hearth_the_list_is_empty_and_they_are_counted`
- `crates/hearth-agent/src/application/services.rs::tests::an_adopted_container_is_published_by_its_key_and_the_others_stay_counted`

## Cas limites
- La ligne « {n} conteneurs créés hors de Hearth ne sont pas gérés » est un écran du client (HRT-81).

## Règles liées
- BR-ADOPT-002, BR-ENGINE-004, BR-SVC-004

## Historique
- 2026-10-13 — création (HRT-79, ADR-0043 : ce que l'agent publie, moitié agent, première partie).
