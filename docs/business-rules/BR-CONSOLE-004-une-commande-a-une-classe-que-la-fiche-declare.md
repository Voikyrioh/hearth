---
id: BR-CONSOLE-004
domaine: CONSOLE
titre: Chaque commande a une classe (refusée, sensible ou libre) que la fiche déclare, jugée sur son premier mot
statut: active
invariant: true
source: contexts/hearth/conceptions/2026-11-17-fonctionnelle-console-et-joueurs-minecraft.md
maj: 2026-11-24
---

# BR-CONSOLE-004 : Classe d'une commande, déclarée par la fiche

## Règle
Chaque commande a une classe, jugée sur son premier mot (en minuscules, préfixe `minecraft:` retiré), selon la liste que la FICHE déclare (`[remote_console.use]`). La liste ne vit ni dans le client ni dans l'agent pour un jeu donné.
- **Refusée** : jamais envoyée ; la phrase dit quoi faire à la place. Minecraft : `stop` (« Pour arrêter le serveur, utilisez le bouton « Arrêter » : il enregistre le monde, et le serveur ne repart pas tout seul. »), `restart` (« Pour redémarrer le serveur, utilisez le bouton « Redémarrer » de Hearth. »), `reload` (« Cette commande recharge le serveur à chaud et peut le casser : redémarrez-le plutôt. »).
- **Sensible** : confirmation qui dit ce que fait la commande, mot de passe à chaque fois (BR-CONSOLE-005). Minecraft : `op`, `deop`, `ban`, `ban-ip`, `pardon`, `pardon-ip`, `kick`, `whitelist`, `save-off`, `execute` (une commande `execute … run` peut en contenir une autre).
- **Libre** : tout le reste, y compris les commandes ajoutées par des mods.

Pourquoi `stop` est refusée : l'image relance un conteneur dont le jeu s'arrête de lui-même, et Hearth ne saurait pas que l'arrêt est voulu. La classe n'est pas une barrière de sécurité complète (un mod peut ajouter une commande qui donne des droits, Hearth ne la connaît pas) : c'est un garde-fou contre l'erreur et un mot de passe sur les gestes connus qui donnent des droits. Le droit de gérer les services reste la vraie barrière.

## Application (code)
- `crates/hearth-proto/src/console.rs::classify` (classe d'une ligne ; `ConsolePolicy::refusal` pour les refusées, `ConsolePolicy::is_sensitive` pour les sensibles, tout le reste est libre). Un verbe d'un autre espace de noms (`mod:commande`) est traité comme sensible (ADR-0067).
- `crates/hearth-proto/src/console.rs::ConsoleLine::verb` (premier mot en minuscules ASCII, préfixe `minecraft:` retiré)
- `crates/hearth-agent/catalog/minecraft/fiche.toml` (`[remote_console.use]` : la liste déclarée, données de la fiche)
- `crates/hearth-agent/src/domain/catalog/loading/schema/fiche.rs` (lecture de la liste ; raisons fermées `use_hearth_stop`, `use_hearth_restart`, `breaks_server`)
- `crates/hearth-proto/src/error.rs::ConsoleCommandRefused` (erreur renvoyée pour un verbe refusé, `details.reason`)
- Interface : `apps/desktop/src/services/console.ts::refusedSentence` (phrase de la raison refusée ; la phrase de `stop` suit la manière d'arrêter de la fiche)

## Vérification
- `crates/hearth-agent/tests/console_https.rs::a_refused_verb_is_refused_even_with_a_prefix_or_capitals_and_is_journaled` (`stop`, `/STOP`, préfixe `minecraft:` : refus, rien d'envoyé)
- `crates/hearth-agent/tests/console_https.rs::a_wrong_signed_class_is_refused_and_nothing_is_sent` (classe annoncée par le client différente de celle jugée par l'agent)
- `crates/hearth-proto/src/console.rs::tests` (refus des formes invalides, round-trip des raisons de refus)
- À la main : dans la console, taper `stop` : la phrase « Pour arrêter le serveur, utilisez le bouton « Arrêter » » s'affiche et rien n'est envoyé au jeu.

## Cas limites
- `/STOP`, `Stop`, `minecraft:stop` → même classe que `stop` (refusée).
- `execute ... run stop` → sensible (premier mot `execute`), jamais refusée.
- Verbe d'un autre espace de noms (`mod:cmd`) → sensible, mot de passe exigé.
- Verbe inconnu sans espace de noms (`foo`) → libre.

## Règles liées
- BR-CONSOLE-003 (forme de la ligne et premier mot), BR-CONSOLE-005 (acte signé, classe rejugée par l'agent), BR-CONSOLE-012 (l'arrêt demandé par Hearth enregistre le monde)

## Historique
- 2026-11-24 : création (HRT-268), fiche rédigée depuis la spec fonctionnelle du 2026-11-17 et le code (`classify`, fiche Minecraft).
