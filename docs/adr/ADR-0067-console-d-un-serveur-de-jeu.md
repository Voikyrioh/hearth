---
id: ADR-0067
titre: Console d'un serveur de jeu : client RCON dans l'agent vers l'adresse privée du conteneur, commandes classées par la fiche, acte ConsoleCommand, joueurs connectés par la commande de la fiche
type: securite
statut: acceptée
date: 2026-11-18
portee: repo
remplace: le point 8 de l'ADR-0052 (« Hearth ne se connecte pas à la console dans ce format ») ; amende son point 4
liens: [us-ecrire-dans-la-console-d-un-serveur-minecraft, 2026-11-17-fonctionnelle-console-et-joueurs-minecraft, 2026-11-17-technique-console-et-joueurs-minecraft, HRT-267, HRT-268, HRT-269, HRT-291, HRT-292, ADR-0031, ADR-0032, ADR-0045, ADR-0046, ADR-0050, ADR-0052, BR-JEU-016, BR-JEU-040, BR-FICHE-051]
---

# ADR-0067 — Console d'un serveur de jeu

## Contexte
Voiky (2026-11-17) : écrire dans la console du serveur Minecraft depuis Hearth et voir les joueurs connectés, dès la 0.2.2. Aujourd'hui Hearth n'envoie au jeu que des littéraux de fiche par l'entrée standard (BR-JEU-016, ADR-0052 point 4) ; la console distante est déclarée, jamais publiée, son mot de passe généré et scellé (BR-JEU-040, ADR-0052 point 7) ; l'ADR-0052 point 8 a écarté la connexion « dans ce format » tout en décrivant la suite (adresse du conteneur sur le réseau du moteur, sans publication, avec ce secret), et son point 9 interdit toute commande exécutée dans un conteneur.

## Décision
1. **Canal** : un client RCON (`source_rcon`) écrit dans l'agent, derrière un port `GameConsole`, qui se connecte à l'adresse IPv4 PRIVÉE du conteneur du service sur son réseau `bridge` (lue par `ContainerEngine::private_address`, fonction de lecture ; jamais en `NetworkMode = host`), au port conteneur du `[[port]]` de rôle `remote_console`. Jamais `docker exec` (`rcon-cli`, `mc-send-to-console`), jamais un port publié, jamais l'entrée standard pour un texte d'utilisateur. Une connexion par échange.
2. **Secret** : le mot de passe généré à l'installation est descellé au moment de l'échange dans un tampon effacé, jamais en argument, variable d'environnement de l'agent, trace, erreur, journal, flux ou réponse.
3. **Déclaration** : `[remote_console.use]` dans la fiche (avenant à l'ADR-0050, format 2 inchangé) autorise la connexion et porte ce qui est propre au jeu : verbes refusés (avec une raison d'une liste fermée), verbes sensibles (titre et phrase de confirmation, argument « joueur »), commande et lecteur des joueurs (liste fermée), commandes rapides (au plus 6, littéraux de fiche, `send` seulement pour une commande libre), enregistrement avant l'arrêt. Rien de propre à un jeu n'est écrit dans le client ni dans l'agent.
4. **Forme et classe d'une commande** : fonction pure partagée `hearth_proto::console::{normalize, classify}` ; une ligne, 1 à 256 points de code, sans contrôle ni caractère à risque, une barre oblique de tête retirée, texte envoyé tel quel ; classe sur le premier mot (minuscules, `minecraft:` retiré) : refusée, sensible, libre. Le client classe pour l'écran, l'agent reclasse pour décider.
5. **Acte `ConsoleCommand` (`0x30`)** : clé du service, SHA-256 de la ligne, classe. Sensible : mot de passe toujours, jamais couvert par l'élévation. Libre : grille des actes de pilotage de l'ADR-0032 (aucun en NORMAL, sauf élévation en ALERTE, toujours en MODE ATTAQUE). Droit de gérer les services. Classe signée différente de la classe rejugée : refus.
6. **Quand** : service installé par Hearth, en marche, signe « prêt » vu ; verrou du service pris en mode partagé sans attendre (refus si une opération exclusive le tient) ; une commande à la fois par service ; 20 par minute et par service.
7. **Réponse hostile** : longueur annoncée 10 à 4 110 octets, identifiant et type vérifiés, 16 Kio au total, fin de réponse après 250 ms de silence, délai total de la fiche (10 s) puis « pas de réponse » (jamais renvoyée), UTF-8 avec remplacement, codes `§x` retirés, contrôle remplacé ; affichée en texte brut, jamais gardée par l'agent ni écrite dans une trace ou le journal.
8. **Journal** : action `service.console` : premier mot (forme fermée, sinon `other`), classe, joueur visé d'un verbe sensible (forme de pseudo seulement), nombre de caractères, issue ; jamais le reste du texte ni la réponse.
9. **Joueurs connectés** : commande de la fiche (`list`) envoyée par l'agent hors débit, lecteur pur d'une liste fermée (`minecraft_list`), cache de 10 s par service, noms hors forme comptés sans être rendus, jamais un chiffre deviné ; lecture sans acte ni journal, droit de lire le service.
10. **Avant un arrêt (bonus)** : étape `console_save` facultative (commande et signe de la fiche, délai borné), jamais bloquante, jamais rejouée par `recover` ; réalise sous une forme étroite l'étape `console_command` annoncée par l'ADR-0052 point 8.

## Comment l'appliquer
- `hearth-proto` : `console.rs` (pur), `api/console.rs`, `api/players.rs`, `ActKind::ConsoleCommand = 0x30` dans `ROUTES`, `password_need`, action `service.console`, capacité `game-console` dans `/hello`.
- Agent : `domain/catalog` (I-CON), `domain/console/players.rs`, `application/ports/game_console.rs`, `infrastructure/console/source_rcon.rs`, `ContainerEngine::private_address`, `application/console.rs`, `entrypoint/http/console.rs` (`GET /services/{key}/console`, `POST /services/{key}/console`, `GET /services/{key}/players`).
- Gardes d'architecture : i4 (pas d'`exec` dans le port du moteur) et « aucune route vers `send_stdin_line` » inchangées ; nouvelle garde : `GameConsole` utilisé seulement par `application/console.rs` (et l'étape `console_save`), `ConsoleLine` construit seulement par `normalize`.

## Quand NE PAS l'appliquer / limites
- Pas pour un service adopté (BR-JEU-001), ni pour les invités du portail.
- Pas pour appliquer un réglage sans redémarrer (chaque réglage devra être prouvé sur la vraie image).
- Pas d'autre protocole que `source_rcon` sans un avenant (`telnet_line` reste déclaré, non parlé).
- La classe n'est pas une barrière complète : une commande ajoutée par un mod qui donne des droits est « libre ». Le droit de gérer les services reste la barrière.
- Moteur sans privilèges : l'hôte ne joint pas l'adresse du conteneur ; la console y est injoignable jusqu'à une décision (ADR-0042 brouillon).

## Alternatives rejetées
- `docker exec rcon-cli` ou `mc-send-to-console` : commande exécutée dans le conteneur (ADR-0052 point 9), secret ou texte en argument de processus.
- Publier le port de console sur la boucle locale : lisible de tout processus du serveur (ADR-0052).
- Entrée standard pour le texte de l'utilisateur : casse l'invariant BR-JEU-016 et ne rend aucune réponse.
- Liste d'autorisation de commandes : contredit l'usage (commandes des mods).
- Joueurs par le ping de la liste des serveurs du jeu : 12 noms au plus, masquable, port public, second protocole à durcir.
- Crate RCON externe : bornes et décodage choisis ailleurs ; acceptable si Stephen la préfère, bornes revérifiées autour.
- Garder l'historique des commandes dans l'agent : accumulerait des messages privés.

## Conséquences
- BR-JEU-016 (invariant) et BR-JEU-040 (invariant) amendées ; BR-FICHE-051 amendée ; BR-CONSOLE-001 à 012 créées avec le code.
- ADR-0052 : point 8 remplacé, point 4 précisé (« par l'entrée standard ») ; ADR-0032 : la grille du pilotage couvre `ConsoleCommand` de classe libre ; ADR-0050 : avenant `[remote_console.use]`.
- Fiche Minecraft : bloc `[remote_console.use]` (version 5, même montée que le lot mods, aucune migration de réponses).
- Docs : `docs/open-api/game-console.md`, `docs/components/Console*.md`, `PlayersOnlineCard.md`, `ARCHITECTURE.md` (domaine `console`, port `GameConsole`), `SECURITY.md` (nouvelle connexion de l'agent).

## Références
- Documentation de l'image : https://docker-minecraft-server.readthedocs.io/en/latest/configuration/server-properties/ (RCON activé par défaut, port 25575, mot de passe aléatoire à chaque démarrage s'il n'est pas donné), https://docker-minecraft-server.readthedocs.io/en/latest/sending-commands/commands/ (`rcon-cli`, `mc-send-to-console`).
- Protocole : https://developer.valvesoftware.com/wiki/Source_RCON_Protocol ; particularités de Minecraft (réponses découpées, longueur de requête) : https://minecraft.wiki/w/RCON
