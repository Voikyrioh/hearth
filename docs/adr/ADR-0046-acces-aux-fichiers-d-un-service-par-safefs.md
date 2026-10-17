---
id: ADR-0046
titre: Tout accès aux fichiers d'un service passe par SafeFs : racine ouverte une fois, chaque composant ouvert depuis le descripteur de son parent sans suivre de lien, par la crate rustix
type: securite
statut: acceptée
date: 2026-10-17
portee: projet
remplace: — (étend l'ADR-0009, dépendances de l'agent ; complète l'ADR-0055 pour l'écriture)
liens: [ADR-0003, ADR-0009, ADR-0012, ADR-0041, ADR-0055, HRT-145, BR-DEPLOY-010, BR-FILES-003, BR-FILES-004, BR-DEL-007, BR-DEL-009]
---

# ADR-0046 : Accès aux fichiers d'un service

## Contexte

L'agent est root. Le contenu du dossier de données d'un service est écrit par le programme du service, hostile par hypothèse : liens symboliques et physiques, tubes, périphériques, remplacements pendant que l'agent travaille. Écrire un fichier de réglages, mesurer, supprimer, et plus tard parcourir, sauvegarder et restaurer touchent ces dossiers. Le dépôt interdit `unsafe`. HRT-145 pose la première tranche : la racine gérée par Hearth (`<données de l'agent>/services/<clé>/`). Les dossiers ouverts par le serveur (liste d'autorisation) et la lecture pour l'onglet Fichiers restent à HRT-93.

## Décision

1. **Port `SafeFs` + `ServiceRoot`** (`application/ports/safe_fs.rs`), seul chemin vers le dossier d'un service. Aucun `std::fs` avec un chemin composé dans les cas d'usage. Le port ne connaît aucun chemin absolu : `(racine, RelPath)`.
2. **`RelPath` jugé par le domaine** (`domain/files/`) avant tout appel système : ni vide, ni absolu, ni `.` ni `..`, sans octet nul, 255 octets par composant, 4 096 au total, 64 niveaux.
3. **Résolution composant par composant** : la racine est ouverte une fois ; chaque composant est ouvert par `openat(parent, nom, O_DIRECTORY | O_NOFOLLOW)` depuis le descripteur du précédent, et son périphérique (`fstat`) doit être celui de la racine. `openat2` avec `RESOLVE_BENEATH` n'est PAS utilisé à ce stade : le repli de l'avant-projet est retenu seul, parce qu'il ne dépend ni de la version du noyau ni du filtre d'appels système de l'unité, et qu'il ferme le même risque (un lien ou un remplacement en cours d'opération ne détourne rien) ; ses limites sont dites ci-dessous.
4. **Même objet vérifié et utilisé** : le type d'un fichier est lu sur le descripteur ouvert (`O_NOFOLLOW | O_NONBLOCK`, `fstat`) ; un tube, un socket, un périphérique, un lien : classés, jamais lus.
5. **Fichier à plusieurs noms** : lu, supprimable, jamais modifié.
6. **Écriture atomique** : fichier d'attente du même dossier (création exclusive, sans suivre de lien), propriétaire et droits posés sur le descripteur avant la première donnée, `fsync`, `renameat`, `fsync` du dossier. Jamais de bit setuid, setgid ni sticky (`Perms` les refuse).
7. **Suppression par descripteurs** (`unlinkat` relatif au parent ouvert) ; un lien est retiré comme lien ; aucun point de montage franchi ; la racine d'un service ne se retire que par `SafeFs::remove_service_root`.
8. **Racines de confiance** : le dossier de données, `services/` et `<clé>/` appartiennent à l'utilisateur effectif de l'agent et ne sont inscriptibles ni par le groupe ni par les autres, sinon `UnsafeRoot`.

## Version

`rustix` 1.x (déjà dans `Cargo.lock` et dans les dépendances de l'agent depuis HRT-130), fonctionnalités `std`, `fs`, `process`. Aucune dépendance ajoutée.

## Usage

```rust
let root = fs.ensure_service_root(&key)?;            // services/<clé>/, 0700
root.mkdir(&RelPath::parse("world/saves")?, dir_perms)?;
root.write_atomic(&RelPath::parse("world/server.cfg")?, bytes, file_perms)?;
let size = root.measure(&RelPath::root(), Duration::from_secs(10))?;   // size.partial : « au moins »
fs.remove_service_root(&key)?;
```

## Doc

Pages de manuel `openat(2)` (`O_NOFOLLOW`, `O_DIRECTORY`, `O_NONBLOCK`), `renameat(2)`, `unlinkat(2)`, `fstatat(2)` ; documentation de `rustix::fs`.

## Périmètre

`infrastructure/fs/` de l'agent (adaptateur), `application/ports/safe_fs.rs` (port), `domain/files/` (`RelPath`). Jamais dans `hearth-proto`. Les fichiers propres à l'agent (`/var/lib/hearth`) n'en ont pas besoin.

## Comment l'appliquer

- Tout code qui écrit, mesure ou retire sous le dossier d'un service prend un `ServiceRoot`.
- La suite de contrat (`tests/support/safe_fs_contract.rs`) tourne contre le faux et l'adaptateur réel ; les pièges (lien planté vers l'extérieur, tube, fichier à plusieurs noms, course, montage simulé) sont dans `infrastructure/fs/tests_unix.rs` (CI Linux).

## Quand NE PAS l'appliquer / limites

- `SafeFs` protège le serveur, pas les fichiers du service contre le service lui-même.
- Un montage « bind » du même système de fichiers garde le même `st_dev` : il n'est pas vu. `openat2` avec `RESOLVE_BENEATH | RESOLVE_NO_XDEV` le verrait ; à reprendre si le besoin se confirme (sonde au démarrage).
- Entre le contrôle d'un fichier existant et le renommage, le service peut encore écrire l'ancien contenu : la course est dite, pas fermée.
- Un jeu qui range ses données derrière des liens n'est pas parcourable à travers eux.
- Hors Linux, `FsError::Unsupported`.

## Alternatives rejetées

- **Canonicaliser puis ouvrir par chemin** : course entre la vérification et l'usage.
- **`cap-std`** : son repli suit les liens internes, ce que `BR-FILES-003` interdit.
- **`libc` et `unsafe`** : interdit par le dépôt.
- **Un conteneur jetable qui monte le dossier** : image de plus, moteur requis, hors du plan fermé.
- **Changer d'identité vers celle du service** : ne vaut rien quand le service est root.

## Conséquences

- L'unité générée de l'agent doit pouvoir écrire sous `<données>/services/` (déjà sous le dossier de données).
- HRT-93 (onglet Fichiers, racines ouvertes) et HRT-85 (suppression) ajoutent leurs gestes à ce port, sans nouveau chemin d'accès.

## Références

- `Cargo.toml` du workspace (`unsafe_code = "forbid"`), `infrastructure/archive_fs/unix.rs` (même technique, destination d'une extraction).
