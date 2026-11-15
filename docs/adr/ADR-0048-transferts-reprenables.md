---
id: ADR-0048
titre: Transferts de fichiers entre le client et l'agent : un WebSocket par transfert, morceaux de 4 Mio vérifiés un par un, un morceau en mémoire, fichier jamais visible sous son nom définitif tant qu'il est incomplet, reprise sur demande pendant 24 heures, zone d'attente hors de tout service
type: securite
statut: acceptée
date: 2026-11-15
portee: projet
remplace: — (pose le brouillon du 2026-10-13, tranche N7a de l'agent ; l'envoi vers la zone d'attente d'abord, le téléchargement et l'envoi vers un dossier de service viennent avec leurs tranches)
liens: [ADR-0004, ADR-0005, ADR-0038, ADR-0045, ADR-0047, ADR-0055, HRT-95, BR-FILES-005, BR-FILES-013, BR-FILES-019, BR-FILES-023, BR-MOD-015, BR-BACKUP-026, SECURITY.md]
---

# ADR-0048 : Transferts reprenables

## Contexte

Envoyer un fichier à l'agent, jusqu'à plusieurs dizaines de Gio (un pack de mods, l'archive d'un monde), sur un lien qui coupe. Contraintes lues dans le code : la requête de la liaison expire à 10 s ; un message du client sur `/stream` est borné à 4 096 octets ; un abonné lent de `/stream` fait fermer tout le flux. Le seul gros fichier que le produit transportait était la mise à jour de l'agent, téléchargée par l'agent en mémoire et plafonnée à 128 Mio : ce n'est pas un précédent. L'agent est root et le fichier reçu est un contenu hostile (il sera lu par ADR-0055) : rien ne doit le rendre visible à un service avant qu'il soit complet et contrôlé.

## Décision

1. **Un WebSocket par transfert**, `GET /transfers/{id}`, authentifié par son premier message comme `/stream` et les journaux. Le transfert est ouvert avant par une route ordinaire (`POST /deposits`, acte `FileWrite` `0x16`, droit de gérer les services, couvert par l'élévation) qui fait toutes les vérifications et rend l'identifiant. Le socket revérifie la session et le droit toutes les 5 s.
2. **Morceaux de 4 Mio**, trame binaire `[rang u64][SHA-256 du morceau][données]`. L'agent vérifie le rang (le PROCHAIN, sans trou, sans doublon), la taille exacte et l'empreinte AVANT d'écrire, rend le morceau durable (`fdatasync`), range son empreinte, PUIS accuse.
3. **Empreinte de l'ensemble** : `tree_digest`, SHA-256 de la suite des empreintes de morceaux (`hearth_proto::api::transfers::chunk_digest`), comparée à la fin. L'agent garde la liste des empreintes à côté de la partie (`<id>.dig`, 32 octets par morceau) : la reprise ne relit pas le fichier.
4. **Un morceau en mémoire par transfert.** L'agent ne lit une trame qu'après avoir écrit la précédente : la contre-pression est celle de TCP, la fenêtre de 8 morceaux non accusés est une discipline de l'émetteur (rien ne s'accumule côté agent). Aucun descripteur n'est gardé entre deux morceaux : un client lent ou arrêté ne retient ni fichier ni mémoire. Soixante secondes sans morceau reçu (un `ping` ne compte pas) ferment le socket ; l'envoi reste reprenable. Un morceau de 4 Mio doit arriver en entier dans ce délai : plancher d'environ 0,56 Mbit/s de montée.
5. **Jamais visible sous son nom définitif tant qu'il est incomplet** : la partie s'appelle `<id>.part` ; à la fin, après vérification de sa taille exacte, elle est renommée `<id>.bin` sans jamais remplacer un fichier (`renameat2` avec `RENAME_NOREPLACE`, repli par lien dur) puis le dossier est rendu durable.
6. **La zone d'attente** est `<données>/transfers` (`0700`, à l'utilisateur de l'agent, jugée avant chaque usage), **hors de tout dossier de service et de tout montage** (les dossiers gérés sont sous `<données>/services/<clé>/`). Aucun nom de fichier du client n'y est gardé : un fichier s'appelle comme son identifiant (ULID).
7. **Propriété prouvée par un registre**, comme les sauvegardes (ADR-0047) : table `transfers` (migration `0018`), un fichier de la zone n'est à Hearth que s'il y est inscrit avec son inode (la partie) ou son inode, sa taille et sa date de modification (le fichier complet) ; la zone n'est jamais listée ; un retrait revérifie l'inode sur le fichier ouvert (`NotOurs` : rien n'est touché). Un dépôt est à son compte : un autre compte n'en voit pas l'existence.
8. **Reprise sur demande seulement** (jamais seule, BR-RESIL-009) : le client rouvre un socket ; l'agent relit le DISQUE (liste d'empreintes, longueur de la partie), ne compte un morceau que si ses octets y sont réellement, annonce l'offset durable, et tronque la partie à cet offset avant le prochain morceau (un morceau à moitié écrit par un arrêt est écrasé). Un socket plus récent prend la place d'un ancien resté ouvert (verrou par transfert : le morceau en cours finit d'abord).
9. **Vingt-quatre heures** : sans activité, la partie reçue (ou le fichier complet jamais consommé) est retirée, avec sa ligne, au démarrage puis toutes les 10 minutes. C'est le seul fichier que l'agent retire de la zone d'attente (ADR-0045).
10. **Limites** : quatre transferts en cours pour l'agent, deux par compte (`TRANSFER_LIMIT`) ; huit dépôts non terminés par compte (la zone n'est pas un entrepôt). Espace : le plus grand de 1 Gio et de 5 % du disque doit rester libre après l'écriture, vérifié pour TOUTE la taille à l'ouverture, puis avant chaque morceau (BR-FILES-013).
11. **Le consommateur** d'un dépôt complet (pack de mods, archive de mods, archive d'un monde) passe par le port `DepositZone` : `take(id, compte)` rend un lecteur `Read + Seek` du fichier complet, seulement pour le compte qui l'a déposé et seulement si le fichier est encore celui que Hearth a posé ; `release(id, compte)` le retire (même compte). Jamais un chemin.
12. **Journal** : l'ouverture d'un dépôt puis la fin d'un envoi sont consignées sous `service.file.write` (compte, origine), jamais un nom de fichier, une taille ni un chemin.

## Comment l'appliquer

- Tout flux volumineux ou durable entre client et agent prend son propre socket, jamais `/stream` ni une requête unique.
- Le contenu n'est jamais signé ni journalisé ; l'acte signe le geste, le dernier nom et la taille.
- Un nouveau consommateur d'un dépôt prend `Arc<dyn DepositZone>` ; il ne lit jamais la zone d'attente par un chemin.

## Quand NE PAS l'appliquer / limites

- Un fichier de texte édité (2 Mio au plus) passe par une requête ordinaire.
- Deux connexions TLS épinglées de plus par poste pendant un transfert.
- Avant `auth`, une trame d'un morceau est lue en mémoire par axum (au plus douze connexions non authentifiées, un délai d'authentification court) : borne assumée, pas de trame plus petite à l'ouverture avec la bibliothèque actuelle.
- Un processus root peut remplacer un fichier entre la vérification de l'inode et le retrait : la garde protège d'une erreur, pas de root (même limite que les sauvegardes).
- Un numéro d'inode libéré peut être réattribué tout de suite par certains systèmes de fichiers : une partie remplacée à la main PAR UN FICHIER NEUF au même numéro n'est pas distinguée (le fichier complet publié, lui, se prouve aussi par sa taille et sa date). Le dossier est `0700` à l'agent : seul root peut y poser quoi que ce soit.
- Cette tranche n'envoie que vers la zone d'attente : l'envoi vers un dossier de service (« Remplacer », verrou du service, propriétaire du fichier), le téléchargement avec reprise et « fichier modifié pendant le téléchargement », et le téléchargement d'une sauvegarde viennent avec leurs tranches ; la trame et le socket sont faits pour le sens inverse.
- Sous Windows (poste de développement), la zone d'attente répond « non disponible » : pas d'inode.

## Alternatives rejetées

- **Requêtes HTTP par plages**, une par morceau : dix mille requêtes authentifiées et suivies pour 40 Gio, chacune sous le délai de 10 s.
- **Corps de requête en flux** : pas de reprise, pas d'accusé.
- **Une seule empreinte du fichier entier** : oblige l'agent à relire toute la partie reçue à chaque reprise.
- **Passer par `/stream`** : un transfert lent ferait tomber le lien.
- **Reprise automatique** : contraire à BR-RESIL-009.
- **Garder le nom du fichier du client** : inutile (ce qui décide du genre d'un fichier est sa lecture) et sensible.
- **Une zone d'attente dans un dossier de service** : le programme du service verrait une partie ; hors de tout montage, jamais.

## Conséquences

- Table `transfers` (migration `0018`) ; liste d'empreintes à côté de la partie.
- Capacité `service-transfers` dans `/hello`, acte `FileWrite` `0x16`, exception nommée `DELETE /transfers/{id}`, action de journal `service.file.write`.
- Borne de trame propre à ce socket (4 Mio et 64 octets).
- Un test d'agent sans moteur de fichiers réel pose un faux `TransferFiles` en mémoire (`tests/support/fake_transfer_files.rs`) ; la zone réelle (inodes, liens, renommage sans remplacement) ne se joue que sous Linux.

## Références

- `crates/hearth-agent/src/application/transfers.rs`, `domain/transfers/`, `entrypoint/ws/transfers.rs`, `entrypoint/http/transfers.rs`, `infrastructure/transfer_files.rs`, `infrastructure/sqlite/transfers_repo.rs`, `crates/hearth-proto/src/api/transfers.rs`.
- `docs/open-api/transfers.md`.
