---
id: BR-ADOPT-020
domaine: ADOPT
titre: Le serveur peut désactiver l'adoption dans la configuration de l'agent : aucun candidat, l'acte refusé ; les adoptés restent gérés et « ne plus gérer » reste possible
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-adopter-un-service.md (BR-ADOPT-020), docs/adr/ADR-0043-par-defaut-hearth-ne-gere-que-ses-services-un-conteneur-hors-hearth-s-adopte.md, HRT-80
maj: 2026-10-13
---

# BR-ADOPT-020 — Le serveur peut désactiver l'adoption dans la configuration de l'agent : aucun candidat, l'acte refusé ; les adoptés restent gérés et « ne plus gérer » reste possible

## Règle
`[services] adoption = false` dans `agent.toml` (ou `HEARTH_SERVICES_ADOPTION=off`, la variable l'emporte) ; permise par défaut. Désactivée : `GET /services/external` et `POST /services/adopt` répondent `409 CONFLICT` avec `details.reason = adoption_disabled` et le texte de la spec (« L'adoption de services est désactivée sur ce serveur. Un administrateur du serveur peut la réactiver dans la configuration de l'agent Hearth. ») ; les services déjà adoptés restent publiés et gérés ; `POST /services/{key}/release` reste possible. La ligne des conteneurs non gérés reste (le compte est inchangé). Hearth ne change jamais ce réglage lui-même.

## Application (code)
- `crates/hearth-agent/src/infrastructure/config/mod.rs::ServicesConfig`
- `crates/hearth-agent/src/application/service_adoption.rs::ServiceAdoption`
- `crates/hearth-agent/src/entrypoint/http/services.rs::adoption_disabled`

## Vérification
- `crates/hearth-agent/tests/service_adoption_https.rs::with_adoption_switched_off_nothing_is_offered_nothing_is_adopted_and_an_adopted_service_can_be_released`
- `crates/hearth-agent/src/infrastructure/config/mod.rs::tests::adoption_is_permitted_by_default_and_the_file_or_the_environment_can_turn_it_off`

## Règles liées
- BR-ADOPT-013, BR-GRANT-015

## Historique
- 2026-10-13 — création (HRT-80, ADR-0043 : l'acte d'adopter et de relâcher, moitié agent et protocole).
