---
id: ADR-0047
titre: Sauvegardes d'un service : archive tar compressée en gzip avec manifeste voisin, propriété prouvée par un registre (inode et empreinte du manifeste), racine jugée avant chaque opération, copie en flux, jamais de suppression automatique
type: securite
statut: acceptée
date: 2026-11-14
portee: projet
remplace: — (pose le brouillon du 2026-10-13, tranche N8a de l'agent ; la restauration, le téléchargement et le dossier d'attente viennent avec leurs tranches)
liens: [ADR-0003, ADR-0009, ADR-0044, ADR-0045, ADR-0046, ADR-0055, HRT-97, BR-BACKUP-001 à BR-BACKUP-016, BR-BACKUP-025 à BR-BACKUP-029, BR-BACKUP-032, BR-BACKUP-033, SECURITY.md]
---

# ADR-0047 : Sauvegardes d'un service

## Contexte

Voiky a une règle absolue sur ses serveurs de jeu : ne jamais toucher une sauvegarde faite hors de Hearth. La spec la fondait sur des noms (« backup », « sauvegarde », « .bak »). Un nom se rate (`saves_old/`, `Forge.wld.2026-09-02-120000`) et s'imite. Les volumes sont ceux d'un monde de jeu : dizaines de Gio, millions de fichiers, service en marche. L'agent est root.

## Décision

1. **Format.** `<id>.tar.gz` (tar GNU, gzip niveau 1 par `flate2`, moteur Rust pur) et `<id>.manifest.json` à côté. Le manifeste dit, par entrée : chemin, type, taille, SHA-256, droits, propriétaire, date, `changed` ; les totaux ; l'image et le mode ; et, à côté de l'archive seulement, la taille et le SHA-256 de l'archive (calculé AU FIL de l'écriture, sans relecture). Une copie du manifeste, sans l'empreinte de l'archive, est la **dernière** entrée de l'archive (`.hearth-manifest.json`). **Écart avec le brouillon** : « en tête » est abandonné, parce que l'écrire en tête exigerait de lire deux fois des dizaines de Gio ; Hearth ne lit jamais le manifeste de l'archive pour décider, seulement celui d'à côté.
2. **La propriété est prouvée, jamais devinée.** Hearth ne lit comme sauvegarde, ne renomme et ne supprime qu'un fichier inscrit dans la table `backups` avec son périphérique et son inode, dont le manifeste voisin a l'empreinte de la ligne. Le port `BackupFiles` n'a ni listage de la racine ni recherche par nom ; chaque geste vise un nom dérivé d'un identifiant de sauvegarde (ULID jugé), dans le sous-dossier d'un service ; `remove` revérifie l'inode sur le fichier ouvert et rend `NotOurs` sans rien toucher. Une archive remplacée à la main n'a plus le bon inode : « Corrompue », jamais supprimée ; « Supprimer » retire seulement la ligne. **Écart avec le brouillon** : l'empreinte du manifeste est un SHA-256, pas un HMAC sous `services.key` : la base et la clé ont le même niveau de confiance (root), un HMAC n'apporterait rien de plus et lierait toutes les sauvegardes à la clé ; seule la note l'est (point 5).
3. **La racine est jugée avant chaque opération.** `backups.root` (`/var/lib/hearth-backups` par défaut, `0700`, à l'utilisateur de l'agent, ni lien ni autre propriétaire, jamais modifiable depuis Hearth) ne chevauche aucun dossier de données : chemins canoniques ET identités (périphérique, inode) comparés dans les deux sens, au démarrage et avant chaque geste ; un chemin illisible n'est jamais un feu vert. Une racine qui chevauche refuse la fonction.
4. **La copie est un geste de fond, annulable, borné.** Plan `stop`? / `copy` / `verify` / `start`?, aucune étape destructive : l'archive en cours (`.part`) est un objet CRÉÉ par l'opération, retiré au redémarrage après vérification de son inode (`BR-BACKUP-025`), jamais repris. Lecture en flux par `SafeFs` (`list_dir`, `open_read`, `read_link`, `stat` : jamais un lien suivi, jamais un montage franchi, tubes et sockets ni lus ni archivés), `fstat` avant et après chaque fichier sur le même descripteur. Une seule opération lourde à la fois par serveur (`heavy_io`). Espace : refus sous la réserve `max(5 Gio, 5 %)` sans case pour passer outre, relevé tous les 256 Mio lus, arrêt et suppression de SA partie écrite sous la moitié de la réserve ; six heures au plus.
5. **La note est scellée** (ChaCha20-Poly1305, `services.key`, données associées : service, révision 0, champ `backup-note:<id>`), jamais au manifeste ni au journal d'activité. Le journal d'activité ne porte que des nombres (`Target::ServiceBackup`).
6. **Hearth ne supprime jamais une sauvegarde de lui-même** : ni rotation ni limite ; ses seules suppressions sont celles de ses propres fichiers partiels et d'une archive dont la vérification de création a échoué.
7. **Vérification.** Chaque entrée lue doit avoir sa ligne du manifeste (type, taille, SHA-256), aucune entrée de plus ni de moins, le nombre d'entrées et le total d'octets bornés par le manifeste, aucune fonction d'extraction de la crate `tar` (`entries()` seulement, contenu haché sans rien écrire), l'empreinte de l'archive entière comparée au registre.

## Version

`tar` 0.4 (déjà dans `Cargo.lock`), `flate2` (déjà direct, ADR-0055), `sha2` et `ring` (déjà directs).

## Installation

`tar = { version = "0.4", default-features = false }` : sans `xattr` (aucun attribut étendu n'est lu ni écrit).

## Usage

Écriture : `tar::Builder` sur un `GzEncoder` (`infrastructure/backup_archive.rs::ArchiveWriter`), un fichier à la fois lu en flux par `SafeFs`. Lecture : `tar::Archive::entries()` jugé par `domain::backups::read::ArchiveIndex`. Fichiers de la racine : `infrastructure/backup_files.rs::StdBackupFiles` (`std::fs`, `O_NOFOLLOW`, création exclusive).

## Périmètre

`domain/backups/` (règles pures), `application/backups.rs`, `application/ports/backup_{repo,files}.rs`, `infrastructure/{backup_archive,backup_files}.rs`, `infrastructure/sqlite/backups_repo.rs`, migration `0017`. Livré : créer (à chaud, avec arrêt), lister, estimer, vérifier, supprimer, éléments ignorés. Hors tranche : téléchargement (HRT-95), restauration, copie de sécurité et dossier d'attente (HRT-98), dossiers « ouverts aux services » et services adoptés (HRT-70), ordre d'enregistrement d'un jeu avant la copie (N11).

## Comment l'appliquer

- Test de contrat de la règle absolue : racine semée de fichiers étrangers dont un nommé comme une archive de Hearth ; créer, lister, estimer, vérifier, supprimer, régler : contenu et inode des sentinelles identiques (`tests/service_backups_https.rs`, contre le faux ; `infrastructure/backup_files.rs::tests`, contre le vrai sous Linux).
- Tout geste sur la racine passe par `BackupFiles` ; aucun `std::fs` sur la racine ailleurs que dans `infrastructure/backup_files.rs`.

## Quand NE PAS l'appliquer / limites

- Une archive téléchargée puis redéposée à la main n'est pas à Hearth (pas de ligne) : elle reste lisible par `tar`, jamais vérifiée ni restaurée par Hearth.
- Liens physiques archivés comme fichiers indépendants ; attributs étendus et bits setuid non conservés ; propriétaire et droits relevés mais restaurés seulement par HRT-98.
- Aucune cohérence garantie à chaud ; pas d'instantané du système de fichiers.
- Sauvegardes non chiffrées (`0600`).
- La priorité d'entrée-sortie basse n'est pas posée (`ioprio_set` exige `unsafe`) : la copie tourne sur un fil bloquant ; l'effet est à mesurer sur forge.
- La vérification à la demande est synchrone (une archive de plusieurs dizaines de Gio peut dépasser le délai d'une requête) ; le manifeste, une ligne par entrée, est en mémoire (centaines de Mio pour des millions de fichiers).
- L'unité systemd générée (`ReadWritePaths`) n'est pas modifiée : sous `ProtectSystem=full`, `/var` est inscriptible ; une racine sous `/home` demandera de l'ajouter.

## Alternatives rejetées

- **Règle de noms seule** : se rate, s'imite.
- **HMAC du manifeste sous `services.key`** : voir point 2.
- **Manifeste en tête de l'archive** : double lecture, voir point 1.
- **zstd** : bibliothèque C dans le binaire statique, moins universel à la main. **zip** : droits POSIX, flux. **Format dédupliqué maison, `restic`, `borg`** : illisibles sans outil.
- **Lister la racine et reconnaître une archive à son nom ou à sa marque** : un fichier étranger peut porter un nom « de Hearth ».

## Conséquences

- Migration `0017` : `backups` (registre), `backup_settings` (éléments ignorés). Capacité `service-backups`, actes `0x19`, `0x1B`, `0x1C`, causes d'opération `no_space`, `disk_almost_full`, `backup_corrupt`, `not_restarted`.
- `SafeFs` gagne la lecture (`list_dir`, `stat`, `open_read`, `read_link`).
- La suppression d'un service (HRT-85) refuse encore la case « sauvegardes » : son câblage est un suivi.

## Références

- `contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md` sections 6, 7 et 9.4.
- `forge-nixos/jeux/terraria.nix` (forme `Forge.wld.<date>` des archives faites à la main).
