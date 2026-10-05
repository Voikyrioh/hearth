---
id: ADR-0015
titre: Tableau de bord : courbes en SVG maison, seuils appliqués côté Rust, historique client borné
type: architecture
statut: acceptée
date: 2026-10-05
portee: projet
remplace: —
liens: [ADR-0010, ADR-0013, conception technique 2026-10-04 sections 3, 7, 8, 10, HRT-11]
---

# ADR-0015 — Tableau de bord : courbes en SVG maison, seuils côté Rust, historique client borné

## Contexte

HRT-11 montre la machine en direct : un échantillon par seconde et par serveur, des jauges, des courbes de 1 min, 5 min et 1 h, des niveaux d'alerte (85/95 %, 80/90 °C, processeur tenu 30 s). La conception technique (section 3) prévoyait uPlot pour les courbes et disait que les seuils sont « appliqués côté client sur les séries ».

## Décision

1. **Courbes en SVG maison, pas d'uPlot.** `HAreaChart` trace au plus 360 points par série (1 min : 60, 5 min : 300, 1 h : 360), trait 2 px, remplissage dégradé, point d'extrémité, pas de grille, trous aux pas sans mesure : c'est tout ce que dit le design (`design-system-web.md`). Une dépendance de plus (et son CSS, difficile à plier aux jetons Braise et à la CSP stricte) n'apporte rien à cette échelle. À rouvrir si une courbe dépasse quelques milliers de points ou demande zoom et curseur.
2. **Les seuils ne sont jamais écrits côté TypeScript.** La coquille (`src-tauri/src/dashboard.rs`) applique les fonctions pures de `hearth_proto::thresholds` (`usage_level`, `temperature_level`, `cpu_level`) à chaque échantillon et envoie à l'interface un niveau (`normal`, `attention`, `critical`) par mesure, aux mêmes positions que les listes de l'échantillon. Le niveau du processeur « tenu 30 s » (BR-DASH-004) se calcule sur une série au pas de 1 s tenue par serveur (`DashBook`, 64 points) ; les instants sont ceux de RÉCEPTION (l'horloge murale de l'agent peut reculer ou différer de celle du poste ; un instantané est ramené à « maintenant » en gardant ses écarts). Cela écarte la copie des seuils (ils ont une source unique) et reste exact : le niveau décidé suit le flux, pas la fenêtre affichée. Les pas des courbes d'une heure (10 s) ne servent donc jamais à alerter. Le pont simulé ne calcule aucun niveau : un test ou le panneau de développement « pousse » une mesure dans un niveau (`machine.pin`).
3. **Événements et commande.** `link://snapshot` (identité + historique de 5 minutes + niveaux du dernier échantillon, à la connexion et quand une carte apparaît), `link://metrics` (un échantillon et ses niveaux, chaque seconde), commande `get_dashboard(server_id)` (dernière vue connue : en mémoire, sinon celle du disque : le tableau s'affiche hors ligne et avant la première connexion de la session, BR-DASH-009). Mêmes règles que l'ADR-0013 : écoute posée d'abord, lecture ensuite ; un événement arrivé pendant la lecture l'emporte. Les nombres sont des `f64` à une décimale, affichés tronqués (le niveau est décidé sur la valeur exacte, donc « 85 % » ne s'affiche que si le seuil est atteint) ; les quantités des octets en `f64` (exacts jusqu'à 9 Po).
4. **Historique client borné.** L'interface garde au plus 3 600 échantillons par serveur (une heure à 1 Hz, `SampleRing`), non réactifs ; les courbes se redessinent sur un compteur incrémenté à chaque échantillon accepté, pas à chaque objet. Un échantillon pas plus récent que le dernier est ignoré ; un instantané de reconnexion REMPLACE ce qu'il recouvre et garde ce qui est plus ancien et plus récent (BR-DASH-011). Le rééchantillonnage (1 s, 1 s, 10 s, moyenne par pas) est fait côté client, sur ce qui est en mémoire.
5. **Limite assumée : la fenêtre d'une heure se remplit depuis l'ouverture de l'application.** `hearth-link` ne sait pas lire `GET /metrics/history` (`execute` est réservé aux actions, avec suivi sur disque) ; l'instantané du flux ne porte que 5 minutes. La courbe dit « Depuis N min » tant qu'elle ne couvre pas la fenêtre. Suivi : **HRT-18** (premier lot : « historique d'une heure dès l'ouverture »), une méthode de lecture dans `hearth-link` (sans suivi d'opération) pour remplir l'heure à l'ouverture. La mention est honnête : la couverture se mesure DANS la fenêtre affichée et sur des échantillons contigus (`coverageMs`), donc une vue relue du disque (session précédente) ou une coupure ne comptent jamais comme couvertes, et ne sont jamais reliées par un trait.

## Écarts à la conception technique

Cette décision remplace, pour HRT-11, la section 3 (uPlot) et la phrase de la section 7 « seuils appliqués côté client sur les séries ». Le critère de la section 10 (moins de 5 % d'un cœur) n'est pas encore mesuré : à relever sur le vrai poste, fenêtre visible puis masquée (les abonnements restent actifs, voulu).

## Dépendance ajoutée

`time` (déjà au workspace, fonctions `parsing`) passe en dépendance du client : lecture de l'instant RFC 3339 d'un échantillon.

## Conséquences

- Aucun seuil dans `src/` : changer un seuil, c'est changer `hearth-proto` (et sa fiche BR-DASH-003), le client suit au prochain build.
- Une interface plus lente que le flux (onglet masqué) perd les plus anciens messages (canal borné de la liaison) : l'instantané suivant, ou la lecture de la dernière vue, recolle.
