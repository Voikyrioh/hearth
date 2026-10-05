---
id: BR-CONN-014
domaine: CONN
titre: L'incompatibilité de version d'interface est détectée et dit qui doit se mettre à jour
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-014), ADR-0004, HRT-04
maj: 2026-10-04
---

# BR-CONN-014 — Détection de l'incompatibilité de version

## Règle
Le client annonce la version d'interface qu'il parle (`X-Hearth-Api: <n>`) ; l'agent accepte la plage `[min, max]` de `hearth_proto::version`. Hors plage : `426 INCOMPATIBLE_VERSION` avec `details.upgrade` : `"client"` si la version du client est inférieure au minimum (client trop ancien : mettre à jour le client), `"agent"` si elle est supérieure au maximum (agent trop ancien : mettre à jour l'agent). Toute réponse des routes contrôlées porte `X-Hearth-Api-Range: min-max`. `GET /hello` est exempté (il sert justement à lire la plage avant de parler). Un en-tête absent ou illisible est une erreur de validation (`422 VALIDATION_ERROR`).

## Application (code)
- `crates/hearth-agent/src/domain/compat.rs::check` — décision pure (`Incompatibility`).
- `crates/hearth-agent/src/entrypoint/http/version.rs` — couche de contrôle, appliquée à toutes les routes sauf `/hello`.

## Interface (coquille et vue)
- `link_dto::LinkFailure::{IncompatibleAgent, IncompatibleClient}`, `BlockedDto::{IncompatibleAgent, IncompatibleClient}` ; textes « L'agent de ce serveur est trop ancien. Mets à jour l'agent sur le serveur. » / « Le client est trop ancien. Mets à jour le client sur ce PC. » dans l'assistant (message de carte) et dans `OfflineBanner.vue`. Pas de bouton de mise à jour avant HRT-16/17.

## Vérification
- Tests : `domain::compat::tests` ; `tests/sessions_https.rs::an_incompatible_version_is_refused_with_who_must_upgrade`.

## Cas limites
- Les routes inconnues répondent `404` avant le contrôle de version.

## Règles liées
- BR-CONN-012 (le client reconnaît l'agent par `/hello`).

## Historique
- 2026-10-04 — création (HRT-04, session 2026-10-04-hearth-creation).
- 2026-10-05 : section Interface (HRT-10).
