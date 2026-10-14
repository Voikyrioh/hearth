---
id: BR-OPS-006
domaine: OPS
titre: Aucun secret, aucune valeur, aucun chemin du serveur ni nom d'objet dans la vue d'une opération, ses erreurs, le flux, le journal d'activité ou les traces
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 0 bis et 7), contexts/hearth/tickets/hrt/HRT-144.md
maj: 2026-10-14
---

# BR-OPS-006 — Aucun secret, aucune valeur, aucun chemin du serveur ni nom d'objet dans la vue d'une opération, ses erreurs, le flux, le journal d'activité ou les traces

## Règle
La vue d'une opération (`OperationView`) ne porte que des codes (genre, statut, étape, cause, choix), des compteurs et des dates : jamais le plan en JSON, jamais le journal de défaire (`undo`) qui nomme les objets créés, jamais un chemin, une valeur ou un nom de variable, une commande. Les erreurs des trois routes ont un texte fixe : elles ne répètent ni l'identifiant reçu, ni un nom, ni un chemin. L'entrée du journal d'activité d'une annulation ou d'une décision ne porte que le nom du service (nettoyé et borné) et un geste de liste fermée (« opération annulée », « décision « garder » »). Les traces ne disent que l'identifiant de l'opération et le genre d'un objet laissé en place.

## Application (code)
- `crates/hearth-agent/src/entrypoint/service_ops_wire.rs::view`
- `crates/hearth-agent/src/entrypoint/http/service_ops.rs::access_error`
- `crates/hearth-agent/src/domain/audit/event.rs::{Target::ServiceOperation, OperationGesture}`

## Vérification
- `crates/hearth-agent/tests/service_ops_https.rs::no_path_no_secret_and_no_object_name_reaches_a_response_the_stream_the_journal_or_a_trace`

## Cas limites
- Le test sentinelle cherche le texte brut (aucun effacement avant recherche) dans les réponses, le flux, la table du journal d'activité et les traces capturées ; il ne couvre pas les octets du fichier de base (BR-EDIT-034, `service_spec_secrets.rs`).

## Règles liées
- BR-EDIT-034, BR-SLOG-019, BR-OPS-001

## Historique
- 2026-10-14 — création (HRT-144 : le socle des opérations branché sur des routes et sur le flux).
