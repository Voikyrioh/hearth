---
id: BR-UPDATE-019
domaine: UPDATE
titre: Serveur sans accès à Internet : l'agent actuel continue de fonctionner
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-mises-a-jour.md (BR-UPDATE-019), HRT-17
maj: 2026-10-05
---

# BR-UPDATE-019 : Serveur sans accès à Internet : l'agent actuel continue de fonctionner

## Règle
Si le serveur ne peut pas joindre l'adresse du binaire (résolution, connexion, TLS), la mise à jour se termine par `failed` avec la raison `unreachable` ; rien n'a été écrit, l'agent actuel continue. Le client affiche « Le serveur n'a pas accès à Internet pour télécharger la mise à jour de l'agent. ». Un statut d'erreur, une coupure en cours de route ou un fichier trop gros donnent `download_failed`.

## Application (code)
- `crates/hearth-agent/src/infrastructure/update/download.rs::{HttpsDownloader::fetch, map_error}` -> `FetchError::Unreachable`.
- `crates/hearth-agent/src/application/update.rs::UpdateService::execute` (traduction en `UpdateReason`).

## Vérification
- `tests/update_use_cases.rs::an_unreachable_server_or_a_failed_download_ends_with_a_reason_and_the_agent_is_unchanged`.
- `infrastructure::update::download::tests::an_unreachable_address_is_reported_as_such`, `tests/update_download.rs`.

## Cas limites
- Un certificat que le système ne reconnaît pas est une erreur de connexion : même raison `unreachable`.

## Règles liées
- ADR-0008 (mises à jour signées), ADR-0012 (service système), ADR-0014 (dépendances de la mise à jour)

## Historique
- 2026-10-05 : création (HRT-17, lot agent, session 2026-10-04-hearth-creation).
