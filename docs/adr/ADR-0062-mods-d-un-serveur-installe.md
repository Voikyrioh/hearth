---
id: ADR-0062
titre: Mods d'un serveur installé : voie drop_jars, lecture bornée des fiches d'identité, opération mods par renommages sans remplacement et retour automatique, état activé dans le nom du fichier, origine et liste des mods de chaque sauvegarde dans la base de l'agent
type: architecture
statut: acceptée
date: 2026-11-16 (révisée le 2026-11-17, puis le 2026-11-17 au soir : point 10)
portee: repo
remplace: —
liens: [us-gerer-les-mods-d-un-serveur-minecraft, 2026-11-16-fonctionnelle-gerer-les-mods-d-un-serveur-minecraft, 2026-11-16-technique-gerer-les-mods-d-un-serveur-minecraft, HRT-253 à HRT-259, HRT-293, ADR-0047, ADR-0045, ADR-0046, ADR-0050, ADR-0054, ADR-0055, ADR-0058]
---


# ADR-0062 — Mods d'un serveur installé

## Contexte
Voiky veut voir, ajouter, désactiver et retirer les mods d'un serveur Minecraft déjà installé (0.2.2, décision du 2026-11-17), avec leur origine, avec des fichiers `.jar` venus du PC seulement, pour de petits mods cosmétiques ou d'optimisation. Aucune route de fichiers n'existe (HRT-93/94 au backlog), la fiche ne déclare que `drop_pack`, et l'image `itzg/minecraft-server` a ses propres règles sur `/data/mods` (pack serveur réappliqué seulement s'il change, pack Modrinth local retraité à chaque démarrage, `REMOVE_OLD_MODS` faux par défaut ; code lu le 2026-11-16).

## Décision
1. **Voie `drop_jars`** dans la liste fermée `[mods] ways`, et bloc `[mods.jars]` : `folder` (dossier de rôle `data`, sauvegardé, inscriptible), `dir` (`mods`), `retired_dir` (`mods-retires`), `variants` (variantes qui chargent des mods). Format de fiche inchangé (`2`), fiche Minecraft en version 5.
2. **Lecture d'un jar = lecture d'archive hostile** (ADR-0055) : `inspect` entier avec des bornes de jar (256 Mio, 1 Gio déballé, 65 536 entrées), puis au plus cinq fichiers d'identité de 1 Mio (`fabric.mod.json`, `quilt.mod.json`, `META-INF/neoforge.mods.toml`, `META-INF/mods.toml`, `META-INF/MANIFEST.MF`). Aucune archive contenue ouverte, aucune classe chargée, rien exécuté. Toute valeur passe une forme avant d'être gardée.
3. **Une opération de service `mods`** (journal ADR-0045, verrou exclusif, place des opérations lourdes), acte `ModsApply` `0x20` jamais couvert par l'élévation. Étapes : `stop` (si en marche), `copy`, `verify` (cœur de la sauvegarde réutilisé, note fixe « Avant mods, {date} »), `place` (point de non-retour, destructive), `start` et `watch` (si en marche).
4. **Écrire = poser sans remplacer ou renommer sans remplacer, jamais supprimer.** Dépôt posé en flux à empreinte vérifiée sous un nom choisi par Hearth (`{id}-{version}.jar`) ; retrait et remplacement = renommage `renameat2(RENAME_NOREPLACE)` vers `mods-retires` ; remise = renommage inverse. Chaque mouvement est inscrit au journal (inode) AVANT d'être fait. Seulement dans `data/mods` et `data/mods-retires`, seulement conteneur arrêté, par `SafeFs` (ADR-0046).
5. **Retour automatique** si le jeu ne dit pas « prêt » ou s'arrête : mouvements rejoués à l'envers (inode vérifié), relance, queue du journal du jeu gardée 24 h en mémoire (comme ADR-0058). Coupure de l'agent : `recover` classe, « Revenir à l'état d'avant » rejoue le journal, rien n'est repris seul.
6. **Version de Minecraft** : réponse `version` si elle n'est pas `LATEST`, sinon `logs/latest.log` (64 Kio, ligne « Starting minecraft server version »), sinon manifeste Modrinth, sinon inconnue (jugement de version suspendu, dit).
7. **Mods d'un pack Modrinth** (listés dans `/data/.modrinth-modpack-manifest.json`) : ni retirés, ni remplacés, ni désactivés (l'image les remet à chaque démarrage). Aucune fiche n'écrit `REMOVE_OLD_MODS`.
8. **Activer / désactiver = renommer dans le même dossier** (`x.jar` ↔ `x.jar.disabled`, convention des lanceurs, les moteurs ne chargent que les `*.jar`), par `rename_noreplace`, dans la même opération `mods` que le reste, défait par le même retour. L'état n'est tenu nulle part ailleurs que dans le nom du fichier : une copie mentirait après un renommage à la main ou une restauration.
9. **Origine dans la base de l'agent, jamais dans le volume** (table `service_mod_origins`) : `hearth` (fichier posé par une application réussie : nom, taille, SHA-256), `server_pack` (entrées `mods/` du pack serveur inspecté à l'installation, ou relu une fois pour un serveur plus ancien : nom, taille, CRC-32) ; `modrinth_pack` lu dans le manifeste à chaque fois ; sinon `manual`. Le volume est inscriptible par le jeu et ses mods (ADR-0046) : une étiquette que le code d'un mod peut réécrire ne vaut rien, et un fichier JSON tiers serait une surface d'analyse de plus. Correspondance par identité de fichier : après une restauration, un fichier non reconnu est « ajouté à la main », jamais deviné. Origine en base et état dans le nom validés par Voiky le 2026-11-17 à 21:10.
10. **Liste des mods de chaque sauvegarde dans la base de l'agent** (tables `backup_mod_sets`, `backup_mod_entries`, même migration `0019_mods.sql` ; demande de Voiky du 2026-11-17 : « versionner les sauvegardes avec la liste de mods actifs »). Prise par le cœur commun de la sauvegarde pour toute sauvegarde d'un service à `[mods.jars]` (lecture avant la copie et avant l'arrêt, par un port `ModsSnapshot`), recoupée avec le manifeste de la sauvegarde (même chemin et même SHA-256, sinon « non reconnu dans cette sauvegarde »), écrite dans la transaction qui publie la ligne `ready`, retirée avec la sauvegarde. Jamais dans le manifeste (format d'ADR-0047 inchangé, lisible hors de Hearth) ni dans l'archive. Une lecture de mods ratée ne fait jamais échouer une sauvegarde. Aucune liste reconstituée pour une sauvegarde plus ancienne. Pour la restauration à venir (HRT-99/100), le dossier `mods` d'un tel service est remis à l'identique, les fichiers en trop déplacés dans `mods-retires` (proposé, Q-V5 de la spec).

## Comment l'appliquer
- Domaine pur : `domain/mods/{jar,version_req,compat,listing,backup_list}.rs`, `domain/ops/mods.rs`. Route de lecture `GET /services/{key}/backups/{id}/mods` (point 10), champ `mods` de `BackupView`. Application : `application/mods.rs`. Entrée : `entrypoint/http/mods.rs`, trois routes (`GET /services/{key}/mods`, `POST …/mods/check` en lecture seule, `POST …/mods/apply`). Fil : `hearth_proto::api::mods`.
- Port `SafeFs` : `ServiceRoot::rename_noreplace` (même racine, parents ouverts sans suivre de lien, fichier ordinaire seulement). Port des dépôts : `place` reçoit les droits (`Perms`) du fichier posé.
- Invariants du chargeur : `drop_jars` si et seulement si `[mods.jars]` ; `folder` de rôle `data` sauvegardé ; `dir` et `retired_dir` un composant `[a-z0-9-]{1,32}` distincts ; `stop.world_saved = "by_image_on_signal"` ; jamais `REMOVE_OLD_MODS`.

## Quand NE PAS l'appliquer / limites
- Pas un fichier d'état de Hearth dans le dossier du jeu (suggestion d'un JSON écartée le 2026-11-17, point 9).
- Pas pour les plugins Paper/Purpur (autre dossier, autre format), ni pour une archive `.zip` de mods ou un remplacement de pack (HRT-217), ni pour Modrinth par Hearth (HRT-126).
- Pas une route de fichiers générique : un autre jeu à mods passe par une voie de fiche à lui, ou par HRT-93/94.
- Un mod qui plante APRÈS le signe « prêt » n'est pas vu par la surveillance.
- Les dépendances embarquées dans des jars contenus ne sont pas lues : une dépendance absente avertit, ne bloque pas.

## Alternatives rejetées
- Enchaîner côté client arrêt, sauvegarde, écriture : verrou relâché entre trois opérations, pas de retour cohérent.
- Monter un dossier `/mods` synchronisé par l'image (`COPY_MODS_SRC`) : recréer le conteneur pour changer un montage, et l'image écrase par nom.
- Route de fichiers générique : surface d'écriture bien plus large que deux dossiers.
- Décision « revenir / garder » après un échec : serveur arrêté pour tous jusqu'à la réponse ; écarté au profit du retour automatique (ADR-0058).
- Fichier JSON tenu par Hearth dans le volume pour l'origine et l'état (suggestion de Voiky) : inscriptible par le jeu et ses mods, divergent du nom réel des fichiers après une modification à la main ; remplacé par les points 8 et 9.
- Lire `mcmod.info` et les jars contenus : versions du jeu hors des variantes de la fiche ; ouvrir une archive dans l'archive contredit BR-MOD-012.

## Conséquences
- Migration `0019_mods.sql` (`mods` dans le `CHECK` du genre d'opération, tables `service_mod_origins`, `backup_mod_sets`, `backup_mod_entries`) ; `OpKind::Mods`, `OperationKind::Mods`, `AuditAction::ServiceMods` (`service.mods`), acte `0x20`.
- BR-MOD-001, 011, 012 amendées ; BR-MOD-020 à 033 créées avec le code (032 activer/désactiver, 033 origine, ajoutées le 2026-11-17) ; BR-MOD-034 à 036 (liste des mods d'une sauvegarde, HRT-293 et HRT-294) ; BR-MOD-037 (restauration des mods) écrite avec HRT-99 ; HRT-217 redécoupé (la voie `drop_files` « Tes mods » n'est plus nécessaire pour Minecraft).
- Docs : `docs/open-api/game-mods.md`, `docs/components/Mods*.md`, `ARCHITECTURE.md` (domaine `mods`, opération `mods`).

## Références
- Code de l'image : https://github.com/itzg/docker-minecraft-server/blob/master/scripts/start-setupModpack (`/data/.generic_pack.sum`, `REMOVE_OLD_MODS` par défaut `false`), `scripts/start-utils` (`checkSum`).
- https://github.com/itzg/mc-image-helper : `modrinth/FilePackFetcher.java` (pack local retraité à chaque démarrage), `modrinth/InstallModrinthModpackCommand.java`, `files/Manifests.java` (`.modrinth-modpack-manifest.json`, nettoyage par différence de manifestes).
- Formats : https://fabricmc.net/wiki/documentation:fabric_mod_json , https://github.com/QuiltMC/rfcs/blob/main/specification/0002-quilt.mod.json.md , https://docs.minecraftforge.net/en/latest/gettingstarted/modfiles/ , https://docs.neoforged.net/docs/gettingstarted/modfiles/
