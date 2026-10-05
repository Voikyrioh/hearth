---
id: BR-CONN-002
domaine: CONN
titre: L'empreinte confirmée est mémorisée et vérifiée à chaque connexion
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-04-fonctionnelle-connecter-serveur.md (BR-CONN-002), ADR-0005
maj: 2026-10-05
---

# BR-CONN-002 — L'empreinte confirmée est mémorisée et vérifiée à chaque connexion

## Règle
Une fois confirmée par l'utilisateur, l'empreinte (32 octets) est enregistrée dans le carnet de serveurs. Chaque connexion ultérieure (requête HTTPS, flux WebSocket, reconnexion) refuse la poignée de main TLS si le certificat présenté n'a pas exactement cette empreinte, quelle que soit la chaîne de certification ou le nom du serveur. Seul `/hello` de première prise de contact accepte tout certificat (mode « sonde »), uniquement pour lire l'empreinte à confirmer.

## Application (code)
- `crates/hearth-link/src/domain/pinning.rs::decide` (`PinDecision::{FirstConnection, Match, Mismatch}`).
- `crates/hearth-link/src/adapters/tls.rs` : vérificateur rustls « épinglé » et vérificateur « sonde ».
- `crates/hearth-link/src/manager/mod.rs::LinkManager::probe`.

## Interface (coquille et vue)
- `apps/desktop/src-tauri/src/link.rs::LinkRuntime::add_server` : l'empreinte confirmée par l'utilisateur est enregistrée au carnet (`servers.json`) à « Confirmer » ; `::probe` ne l'enregistre pas.
- Tests : `apps/desktop/src-tauri/tests/link_runtime.rs::the_wizard_flow_registers_connects_remembers_forgets_and_removes`.

## Vérification
- Tests : `domain::pinning::tests`, `adapters::tls::tests` ; intégration : `tests/fault_proxy.rs::a_reinstalled_agent_is_refused_by_the_pinned_fingerprint`, `tests/pinning.rs`.

## Cas limites
- Deux certificats qui ne diffèrent que par les octets 17 à 32 de l'empreinte ont le même affichage court mais sont distincts : la comparaison porte sur les 32 octets.
- TLS 1.3 seul.

## Règles liées
- BR-CONN-001, BR-CONN-003, BR-CONN-011.

## Historique
- 2026-10-05 — création (HRT-07, session 2026-10-04-hearth-creation).
- 2026-10-05 : section Interface (HRT-10).
