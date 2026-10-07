---
id: ADR-0016
titre: Présence hors de la fenêtre (notifications système, icône de la zone de notification) et actions de l'interface (une commande typée par action) (une commande typée par action)
type: architecture
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0002, ADR-0007, ADR-0013, BR-RESIL-008, BR-RESIL-009, BR-RESIL-015, BR-RESIL-016, HRT-12]
---

# ADR-0016 — Présence hors de la fenêtre et actions de l'interface

## Contexte

Le serveur de la maison s'éteint et se rallume (jeux, veille, réveil réseau) : le client passe une bonne partie de sa vie fenêtre cachée, dans la zone de notification. Le lien doit s'y faire voir (notification système, icône) sans jamais déranger (une alerte par minute au plus), et l'interface doit pouvoir lancer des actions sur un serveur avec la garantie « jamais rejouée » de la bibliothèque (BR-RESIL-009).

## Décision

1. **Règles pures dans la coquille, sans dépendance nouvelle** : `presence.rs` (aucune E/S, aucun type Tauri, temps en paramètre) porte le limiteur de notifications (`NotificationGate`) et l'état de l'icône (`LinkPresence`, `TrayStatus`) ; le dessin de la pastille (`badge.rs`, couleurs des jetons de `tokens.css`, un test relit le fichier) n'est pas une règle. Les greffons déjà présents font le reste : `tauri-plugin-notification` pour les notifications, l'icône du greffon tray (`TrayIcon::set_icon`, `set_tooltip`). Aucune nouvelle dépendance, aucune image supplémentaire : la pastille verte, orange ou rouge est dessinée sur la flamme existante. *(HRT-19, ADR-0027 : la pastille est remplacée par une image par état, `tray_icons.rs` ; la logique de présence ne change pas.)*
2. **Observateur d'états** : `link::StateObserver` (un seul, branché par `LinkRuntime::set_observer`, qui lui donne l'état courant) reçoit chaque état du relais d'événements. `alerts::Alerts` l'implémente ; les ports `Notifier` et `TrayPort` isolent le système (espions dans les tests).
3. **Limite des notifications** (spec : « une notification par minute et par serveur ») : UNE fenêtre d'une minute par serveur, toutes natures confondues ; une situation déjà annoncée n'est jamais répétée ; ce que la limite retient part à l'échéance (minuteur de 5 s), une fois, avec le nombre de changements absorbés en plus. Notifient seulement deux CHANGEMENTS d'état : « Hors ligne » et le retour « Connecté » qui le suit ; une panne qui dure ne produit rien de plus (le nombre de notifications système est borné, indépendant de la durée : un serveur éteint la nuit est le cas normal). Les échecs répétés ne notifient JAMAIS le système : leur compteur (« Reconnexion échouée {n} fois. », un palier de 5) vit dans les notifications discrètes de l'application. La reconnexion, la session expirée et l'accès révoqué ne notifient pas. Réglage « Notifier quand un serveur devient hors ligne ou revient », activé par défaut (la spec dit « désactivables »), dans `settings.json`.
4. **Icône** : elle reflète le serveur affiché dans la fenêtre (l'interface le dit par `set_displayed_server`, qui ne retient qu'un identifiant présent au carnet), sinon le pire état de tous (une panne n'est jamais cachée derrière la page des réglages), sinon neutre. Elle n'est redessinée que si l'état ou l'infobulle change. Un état qui arrive après la suppression du serveur ne le réinscrit pas.
5. **Actions : aucune commande générique.** La coquille n'expose PAS de commande « envoie cette requête » : une WebView compromise pourrait appeler n'importe quelle route de l'agent avec la session de l'utilisateur (ADR-0013 : « un nouveau besoin = une commande ou un événement typé »). Chaque action réelle (créer un compte, mettre à jour l'agent…) arrive avec son ticket sous la forme d'une commande typée, validée côté Rust, la méthode et le chemin construits côté Rust, qui appelle `LinkManager::execute` (clé d'opération, résultat inconnu à la coupure, jamais rejouée). L'interface passe par `useServerAction.run(perform)` où `perform` appelle cette commande ; le bouton porte `needs-link` (BR-RESIL-008). L'action d'essai du navigateur de développement (`DevActionPanel`) est une méthode du pont SIMULÉ (`runDevAction`) : elle n'existe ni dans le pont réel, ni dans les capacités, ni dans le binaire livré. Test : `tests/capabilities.rs` (aucune permission générique) ; une permission pour une commande absente fait échouer la compilation.

## Alternatives écartées

- Un module de notifications dans l'interface (Web Notifications) : l'application est cachée, la fenêtre peut être suspendue ; la coquille voit tous les états même fenêtre fermée.
- Quatre fichiers d'icône : une pastille dessinée évite de maintenir des images et suit les jetons de couleur.
- Une commande générique bornée (`run_action` avec liste de tailles) : écartée en review, la taille et `..` ne sont pas une liste de routes ; seule une commande typée par action ferme la menace.

## Conséquences

- Ajout de commandes : `get_notify_on_link_change`, `set_notify_on_link_change`, `set_displayed_server` (les 5 endroits de l'ADR-0010). Les futures actions suivent la même règle.
- À vérifier sur une vraie machine Windows : rendu de la pastille à 16 px (thème clair et sombre), notification affichée par le centre de notifications (le greffon ne dit pas si Windows l'a affichée).
