---
id: ADR-0016
titre: Présence hors de la fenêtre (notifications système, icône de la zone de notification) et actions de l'interface
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

1. **Règles pures dans la coquille, sans dépendance nouvelle** : `presence.rs` (aucune E/S, aucun type Tauri, temps en paramètre) porte le limiteur de notifications (`NotificationGate`), l'état de l'icône (`LinkPresence`, `TrayStatus`) et la pastille (`paint_badge`). Les greffons déjà présents font le reste : `tauri-plugin-notification` pour les notifications, l'icône du greffon tray (`TrayIcon::set_icon`, `set_tooltip`). Aucune nouvelle dépendance, aucune image supplémentaire : la pastille verte, orange ou rouge est dessinée sur la flamme existante.
2. **Observateur d'états** : `link::StateObserver` (un seul, branché par `LinkRuntime::set_observer`, qui lui donne l'état courant) reçoit chaque état du relais d'événements. `alerts::Alerts` l'implémente ; les ports `Notifier` et `TrayPort` isolent le système (espions dans les tests).
3. **Limite des notifications** : « Hors ligne » et « de retour » ont chacun leur fenêtre d'une minute par serveur ; une situation déjà annoncée n'est jamais répétée ; ce que la limite retient part à l'échéance (minuteur de 5 s), une fois, avec le nombre de changements absorbés ; seuls « Hors ligne » et le retour « Connecté » qui le suit notifient. Réglage « Notifier quand un serveur devient hors ligne ou revient », activé par défaut, dans `settings.json`.
4. **Icône** : elle reflète le serveur affiché dans la fenêtre (l'interface le dit par `set_displayed_server`), sinon le pire état de tous (une panne n'est jamais cachée derrière la page des réglages), sinon neutre. Elle n'est redessinée que si l'état ou l'infobulle change.
5. **Actions** : une commande générique `run_action(serverId, { method, path, body })` appelle `LinkManager::execute` ; réponse `completed { status, body }` ou `unknown { opId }` (lien tombé avant la réponse, jamais rejouée, issue par `link://operation`). La coquille borne le chemin (absolu, sans `..`, sans caractère de contrôle, 512 caractères) et le corps (JSON, 64 Kio) ; le corps n'est jamais journalisé. L'interface passe par `useServerAction`, et le bouton porte `needs-link` (BR-RESIL-008).

## Alternatives écartées

- Un module de notifications dans l'interface (Web Notifications) : l'application est cachée, la fenêtre peut être suspendue ; la coquille voit tous les états même fenêtre fermée.
- Quatre fichiers d'icône : une pastille dessinée évite de maintenir des images et suit les jetons de couleur.
- Une commande par action métier dès maintenant : les actions d'administration (comptes, mise à jour) arrivent avec leurs tickets ; la commande générique, bornée, suffit au socle et ne ferme rien.

## Conséquences

- Ajout de commandes : `get_notify_on_link_change`, `set_notify_on_link_change`, `set_displayed_server`, `run_action` (les 5 endroits de l'ADR-0010).
- À vérifier sur une vraie machine Windows : rendu de la pastille à 16 px (thème clair et sombre), notification affichée par le centre de notifications (le greffon ne dit pas si Windows l'a affichée).
