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
5. **L'heure écoulée est lue à la connexion (HRT-18).** À chaque connexion, `hearth-link` lit `GET /metrics/history?window=1h` par le port `Transport::hour_history` (lecture seule, sans suivi d'opération ; `execute` reste réservé aux actions) **À CÔTÉ de la connexion, jamais dans la tentative** : le lien passe « Connecté » et le direct coule d'abord ; la lecture est une tâche séparée de la tâche du serveur, bornée par `request_timeout`, abandonnée dès que le lien retombe (rien n'est annoncé pour l'ancienne session). Une route muette ou lente ne retarde donc ni « Connecté » ni le direct et ne fait échouer aucune tentative. Elle ne garde que les échantillons STRICTEMENT plus anciens que le premier de l'instantané du flux (`domain::history::older_than_snapshot` : l'heure, à 1 échantillon par 10 s, ne remplace jamais le détail à la seconde) et les annonce par `Event::History`, après l'instantané. La coquille les retient (`DashBook`, rejoués avec la dernière vue par `get_dashboard` à une interface qui s'ouvre plus tard), les envoie en `link://history` et ne les mêle jamais à la série du processeur (qui suppose un pas de 1 s). L'interface les recolle dans l'anneau par `SampleRing::fill` : l'heure ne comble que ce que l'anneau n'a pas (plus ancien que son premier échantillon, trous de PLUS DE 3 s : un point de l'heure n'entre que si aucun échantillon de l'anneau n'est à 1,5 s ou moins de lui, des deux côtés ; en pratique une coupure du lien de plus de 5 minutes pendant laquelle l'agent mesurait) et ne remplace JAMAIS un échantillon reçu en direct (à une reconnexion, un pic vu à la seconde ne retombe pas à la moyenne de son pas) : la courbe d'une heure est remplie dès l'ouverture, sans « Depuis N min ». Une lecture qui échoue (agent sans la route, délai) n'empêche jamais la connexion (journalisée en `info`, sans rien de sensible) : l'instantané seul, et « Depuis N min » comme avant. L'historique d'une heure n'est PAS persisté (le disque garde les 5 minutes de l'instantané). La couverture se mesure DANS la fenêtre affichée et sur des échantillons contigus (`coverageMs`, tolérance 1,5 pas de la fenêtre, 5 s au moins), donc une vue relue du disque ou une coupure ne comptent jamais comme couvertes. L'agent rend, à côté de la moyenne de chaque pas de 10 s, le MAXIMUM de chaque mesure tracée (`peaks` : processeur, mémoire, débits, charge et mémoire de chaque carte graphique) ; `hearth-link` le met à la place de la moyenne dans l'heure annoncée (`domain::history::with_peaks`), donc un pic d'une seconde ne disparaît pas en vieillissant (FIX-01M4CY8BVM3MV9769QWNDNW7VT). Champ AJOUTÉ au contrat : un client plus ancien lit les moyennes. Si une lecture de l'heure échoue après une première réussie, l'heure retenue d'une connexion reste en place et est rejouée devant un instantané plus récent, avec un vide entre les deux (le vide est vrai).

## Écarts à la conception technique

Cette décision remplace, pour HRT-11, la section 3 (uPlot) et la phrase de la section 7 « seuils appliqués côté client sur les séries ». Le critère de la section 10 (moins de 5 % d'un cœur) n'est pas encore mesuré : à relever sur le vrai poste, fenêtre visible puis masquée (les abonnements restent actifs, voulu).

## Dépendance ajoutée

`time` (déjà au workspace, fonctions `parsing`) passe en dépendance du client : lecture de l'instant RFC 3339 d'un échantillon.

## Conséquences

- Aucun seuil dans `src/` : changer un seuil, c'est changer `hearth-proto` (et sa fiche BR-DASH-003), le client suit au prochain build.
- Une interface plus lente que le flux (onglet masqué) perd les plus anciens messages (canal borné de la liaison) : l'instantané suivant, ou la lecture de la dernière vue, recolle.
