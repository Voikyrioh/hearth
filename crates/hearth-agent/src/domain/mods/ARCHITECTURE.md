# domain/mods

Mods et packs de jeu : domaine pur, aucune E/S (HRT-124, ADR-0054, HRT-254 à 257, HRT-293, ADR-0062).

```
mods/
├── game.rs         → `is_game_version`, ce qu'un pack serveur dit du jeu et du moteur (HRT-225, BR-MOD-019)
├── pack.rs         → jugement d'un pack déposé (serveur, Modrinth), `PackReport` dont `server_mods` (nom, taille, CRC-32 des `.jar` de `mods/`) — BR-MOD-007, 008, 016
├── jar.rs          → la carte d'identité d'un `.jar` (`identify`, `JarIdentity`) : cinq fichiers lus au plus, toute valeur passe une forme — BR-MOD-022
├── version_req.rs  → `GameVersion`, `VersionReq` (prédicats Fabric/Quilt, plages Maven), `Containment` — BR-MOD-021
├── compat.rs       → `judge_installed` / `judge_batch` : verdict d'un mod pour un serveur (moteur, version, dépendances, doublons) — BR-MOD-021
├── listing.rs      → noms de fichiers : alphabet sûr, `.jar` / `.jar.disabled`, nom de pose choisi par Hearth, noms dans « Mods retirés » — BR-MOD-020, 032
├── game_info.rs    → ce que Hearth lit du jeu : version dans `logs/latest.log`, manifeste Modrinth, origine d'un fichier (`resolve_origin`) — BR-MOD-021, 033
├── deposit.rs      → jugement d'un lot de jars déposés : ajout, remplacement ou refus avec son code (`judge_deposits`) — BR-MOD-023
└── backup_list.rs  → liste des mods d'une sauvegarde recoupée avec le manifeste (`reconcile`) et comparaison avec les mods actuels (`compare`) — BR-MOD-034, 036
```

Règles communes : un nom, une version ou un identifiant lu dans un jar n'est gardé que s'il a la forme attendue, jamais recopié ; aucune panique sur un texte hostile (comparaisons par octets, jamais de découpe d'index) ; aucun `std::fs`, aucune archive : l'application lit (`SafeFs`, `domain::archive`) et donne des octets.
