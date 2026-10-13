---
id: BR-GRANT-003
domaine: GRANT
titre: Tout acte de service exige le droit ; le rôle seul n'en ouvre aucun
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-droit-de-gerer-les-services.md (BR-GRANT-003), contexts/hearth/conceptions/2026-10-13-adr-droit-de-gerer-les-services-brouillon.md, HRT-76
maj: 2026-10-13
---

# BR-GRANT-003 — Tout acte de service exige le droit ; le rôle seul n'en ouvre aucun

## Règle
Exigent le droit (et le rôle seul n'en ouvre aucun) : démarrer, arrêter, redémarrer, déployer, annuler un déploiement, adopter et relâcher un service, révéler les valeurs des variables, lire la liste des candidats à l'adoption, et pour les lots suivants supprimer, modifier, limiter les ressources, console, fichiers, sauvegardes. Lire la liste des services, le détail d'un service créé par Hearth et son récapitulatif de ports n'exigent pas le droit.

Mise en œuvre : toute route d'un acte de service est déclarée `Access::Services` dans `ENDPOINTS`. HRT-76 : seule `GET /services/{key}/env` existe aujourd'hui sur cette branche (elle était `Access::Admin`). Un test de table impose que toute route `/services/{key}/…` ou `/deployments…` qui modifie ou qui est consignée soit `Access::Services` : un acte de service à venir déclaré autrement fait échouer la suite.

## Application (code)
- `crates/hearth-agent/src/entrypoint/http/mod.rs::Access::Services`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::ENDPOINTS (`GET /services/{key}/env`, `Access::Services`)`
- `crates/hearth-agent/src/domain/accounts/rights.rs::ServicesRight::can_manage_services`

## Vérification
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::every_service_act_is_declared_at_the_services_level`
- `crates/hearth-agent/src/entrypoint/http/mod.rs::tests::the_services_routes_are_reads_open_to_both_roles_and_never_journaled`
- `crates/hearth-agent/tests/http_api.rs::every_services_route_refuses_an_account_without_the_right_even_an_administrator`

## Cas limites
- Ce que voit encore un compte sans le droit dans ce ticket : `GET /services`, `POST /engine/recheck` (lecture), le sujet `services` du flux, avec la liste, l'état, les ports, les noms de variables et la commande de lancement des services publiés. Le filtre de publication et la restriction du détail d'un service adopté sont des tickets suivants.

## Règles liées
- BR-GRANT-001, BR-GRANT-004, BR-SVC-009, BR-SVC-020

## Historique
- 2026-10-13 — création (HRT-76, ADR-0041 : le droit de gérer les services, moitié agent).
