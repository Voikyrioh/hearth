---
id: BR-CONSOLE-008
domaine: CONSOLE
titre: 20 commandes par minute et une à la fois par serveur
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-008 : 20 commandes par minute et une à la fois par serveur

## Règle
Au plus 20 commandes par minute et par serveur, tous comptes confondus (seau à jetons en mémoire : rafale de 5, un jeton toutes les 3 secondes), et une seule à la fois (la suivante attend la réponse de la précédente). Au-delà : `429 CONSOLE_RATE_LIMITED` avec `details.retry_after_s`, sans envoi. Les lectures des joueurs connectés ne comptent pas. Le seau est oublié au redémarrage de l'agent.

## Application (code)
- `crates/hearth-agent/src/domain/console/bucket.rs::TokenBucket`
- `crates/hearth-agent/src/application/console.rs::{PerService, ConsoleService::send}`

## Vérification
- `crates/hearth-agent/src/domain/console/bucket.rs::tests`
- `crates/hearth-agent/tests/console_https.rs::the_commands_are_limited_per_minute_and_the_wait_is_told`

## Règles liées
- BR-CONSOLE-006, BR-CONSOLE-011

## Historique
- 2026-11-18 : création (HRT-268). Écart avec la spec « 21e commande de la minute » : la rafale de 5 de la conception technique (D7) s'applique, le test envoie donc une sixième commande immédiate.
