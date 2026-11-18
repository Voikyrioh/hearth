---
id: BR-CONSOLE-007
domaine: CONSOLE
titre: La réponse du jeu est bornée, brute, jamais gardée ; une commande sans réponse n'est jamais renvoyée
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-18
---

# BR-CONSOLE-007 : La réponse du jeu est bornée, brute, jamais gardée ; une commande sans réponse n'est jamais renvoyée

## Règle
La réponse du jeu (conteneur hostile) est lue par le client RCON de l'agent avec des bornes (longueur annoncée, identifiant, type, 16 Kio au total, 250 ms de silence pour la fin, délai de la fiche) ; elle va seulement dans la réponse HTTP (`reply`, texte décodé et nettoyé, `truncated` si coupée), jamais dans une trace, une erreur, le journal d'activité ni le disque. Réponse vide : `200 outcome = empty`. Délai dépassé : `200 outcome = no_answer` (la commande a peut-être été exécutée, Hearth ne la renvoie JAMAIS). Console injoignable (pas d'adresse privée, connexion refusée, mot de passe refusé, réponse hors protocole) : `502 CONSOLE_UNREACHABLE`, sans détail technique, inscrit au journal.

## Application (code)
- `crates/hearth-agent/src/infrastructure/console/source_rcon.rs` (bornes)
- `crates/hearth-agent/src/application/console.rs::ConsoleService::{send, talk}`

## Vérification
- `crates/hearth-agent/tests/console_rcon.rs` (faux serveur hostile)
- `crates/hearth-agent/tests/console_https.rs::{a_late_answer_is_a_success_without_text_and_the_command_is_never_sent_again, an_empty_answer_and_a_cut_answer_are_told, an_unreachable_console_is_a_bad_gateway_and_is_journaled}`

## Règles liées
- BR-CONSOLE-001, BR-CONSOLE-009

## Historique
- 2026-11-18 : création (HRT-268).
