---
id: BR-CONSOLE-005
domaine: CONSOLE
titre: Envoyer une commande est l'acte signé ConsoleCommand : la classe est rejugée par l'agent
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-005 : Envoyer une commande est l'acte signé ConsoleCommand : la classe est rejugée par l'agent

## Règle
Envoyer une commande est l'acte `ConsoleCommand` (`0x30`), lié à la clé du service, à l'empreinte SHA-256 de la ligne NORMALISÉE et à la classe que le client a jugée (`1` libre, `2` sensible). **Mot de passe** : commande sensible, toujours, dans tous les états de confiance, jamais couvert par l'élévation ; commande libre, grille des actes de pilotage (NORMAL aucun, ALERTE sauf élévation, MODE ATTAQUE à chaque commande) ; une classe inconnue se traite comme sensible. La preuve de clé du poste est exigée à chaque commande. L'agent REJUGE la classe sur la politique de la fiche : une classe signée différente est refusée `CONSOLE_CLASS_MISMATCH` (client périmé), sans rien envoyer. Le droit de gérer les services est exigé AVANT la confirmation. Le texte de la commande n'est jamais signé en clair, seulement son empreinte.

## Application (code)
- `crates/hearth-proto/src/admin_act.rs::{ActKind::ConsoleCommand, AdminAct::console_command, password_need, covered_by_elevation}`
- `crates/hearth-proto/src/api/console.rs::ConsoleRequest::act_digest`
- `crates/hearth-agent/src/entrypoint/http/reauth.rs::Parsed::Console`
- `crates/hearth-agent/src/application/console.rs::ConsoleService::send` (rejugement de la classe)

## Vérification
- `crates/hearth-proto/src/admin_act.rs::tests::a_console_command_follows_the_grid_when_free_and_always_asks_when_sensitive`
- `crates/hearth-agent/tests/console_https.rs::{a_free_command_goes_without_password_to_the_private_address_with_the_secret_of_the_fiche, a_sensitive_command_asks_for_the_password_every_time_even_under_the_elevation, a_wrong_signed_class_is_refused_and_nothing_is_sent}`
- `crates/hearth-agent/tests/admin_reauth.rs` (matrice des actes : `ActKind::ConsoleCommand`)

## Règles liées
- BR-CONSOLE-003, BR-CONSOLE-004, BR-TRUST-036, BR-TRUST-043, BR-GRANT-017

## Historique
- 2026-11-18 : création (HRT-268).
