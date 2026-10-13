---
id: ADR-0055
titre: Lire une archive apportée par l'utilisateur (.zip, .tar, .tar.gz) : l'archive est hostile jusqu'à preuve du contraire ; tout est borné avant d'être alloué, rien n'est écrit avant que toute l'archive soit jugée, et un doute refuse l'archive entière
type: securite
statut: acceptée
date: 2026-10-13
portee: projet
remplace: — (complète la section 13.2 de la conception technique des fiches, qui reportait le `.zip`)
liens: [ADR-0009, ADR-0045, ADR-0047 (brouillon), ADR-0048 (brouillon), ADR-0050, ADR-0054 (brouillon), HRT-130, BR-ARCH-001 à BR-ARCH-011, SECURITY.md]
---

# ADR-0055 : Lire une archive apportée par l'utilisateur

## Contexte

Voiky, le 2026-10-13 vers 18:30, sur les archives `.zip` : « perso je serais d'avis de gérer, rien que pour les modpack minecraft où c'est prévalent ». Trois usages lisent maintenant une archive que Hearth n'a pas écrite : reprendre un monde depuis une sauvegarde existante (HRT-110), déposer une archive de mods (règle MOD-005 du hub), regarder un pack Minecraft (règle MOD-007 ; un `.mrpack` est un ZIP). L'agent est root. Un ZIP est plus piégeux qu'un tar : sa table des matières est **à la fin** du fichier et peut contredire les en-têtes placés devant chaque fichier ; deux lecteurs peuvent donc voir deux contenus différents dans le même fichier.

## Décision

1. **Un seul module**, `domain/archive/`, en Rust pur : il reçoit une source `Read + Seek` et un port d'écriture (`ExtractSink`), et rend une liste d'entrées jugées ou un refus typé. Les trois usages passent par lui. Aucun autre code de l'agent n'ouvre une archive. L'adaptateur fichiers est `infrastructure/archive_fs.rs`.
2. **Deux passes, liées par l'empreinte du fichier** : `inspect` juge toute l'archive et n'écrit rien ; `extract` recalcule le SHA-256 du fichier d'archive et refuse s'il a changé. **Rien n'est écrit tant que toute l'archive n'est pas jugée.**
3. **Bornes vérifiées avant toute allocation**, sur des nombres lus dans le fichier donc faux par hypothèse : fichier d'archive 8 Gio (lu par la position de fin) ; 200 000 entrées, et le nombre annoncé par l'index de fin doit tenir dans sa taille (46 octets au moins par entrée) avant toute réservation ; index de fin 64 Mio ; nom 1 024 octets (255 par composant) ; 32 niveaux ; 1 000 000 de noms distincts ; taille déballée totale sous l'espace libre moins la réserve et 64 Gio au plus. L'index entre en mémoire par ce que le fichier fournit réellement, jamais par sa taille annoncée.
4. **Taux de compression.** Refus à l'inspection si une entrée annonce plus de 200 fois sa taille compressée (au-delà de 1 Mio déballé) ou si l'archive entière annonce plus de 100 fois sa taille. Un `.tar.gz` est surveillé pendant la lecture. À l'inspection, chaque entrée d'un zip est relue (taille réelle, CRC-32). À l'extraction, l'écriture s'arrête au premier octet au-delà de la taille annoncée.
5. **L'index de fin peut mentir : on le confronte.** Pour chaque entrée d'un ZIP, l'en-tête local relu doit dire la même chose que l'index (nom, méthode, drapeaux, tailles, CRC, ou descripteur de données qui les confirme). Les entrées pavent le fichier sans trou ni chevauchement de l'octet 0 à l'index. Refus de toute l'archive : en-tête local différent, chevauchement, entrée hors du fichier, plus d'un enregistrement de fin, octets non couverts (données préfixées, archive concaténée), plusieurs volumes. Les octets que le pavage ne voit pas sont refusés eux aussi : champ « extra » local de plus de 512 octets ou qui n'est pas une suite exacte d'enregistrements, octets après la fin d'un flux deflate dans la zone compressée, corps d'une entrée de dossier autre qu'un flux deflate vide (`03 00`). ZIP64 partiel : un champ de l'enregistrement de fin qui n'est pas saturé doit dire la même chose que l'enregistrement ZIP64. Une archive **stockée dans** l'archive n'est pas un second enregistrement de fin : seules les signatures à partir du début de l'index comptent.
6. **Chemins.** Refus de l'archive pour un nom absolu, avec lettre de lecteur ou « : », barre oblique inverse, composant vide, `.` ou `..`, octet nul, caractère de contrôle (catégories Unicode `Cc` : C0, DEL, C1) ou de format (`Cf` : inversion bidirectionnelle, largeur nulle, marques de langue, U+00AD, U+FEFF…, plus les séparateurs de ligne et de paragraphe U+2028 et U+2029 : voir `BR-ARCH-003`), point ou espace final, nom réservé de Windows, trop long ou trop profond. Un nom est un `ArchivePath` (type qu'on ne fabrique que par ce jugement) ; l'écriture se fait par le port, sans jamais suivre un lien.
7. **Liens et fichiers spéciaux.** Seuls un fichier ordinaire et un dossier sont acceptés. Lien symbolique, lien physique, périphérique, tube, prise : refus de toute l'archive. Le type d'une entrée de zip est lu dans les attributs externes QUEL QUE SOIT l'hôte déclaré par `made_by`. Les droits, le propriétaire et les dates de l'archive ne sont jamais repris : l'APPELANT demande, par une `WriteSpec`, un propriétaire (identifiants numériques) et des modes fixes (fichiers sans bit d'exécution ni spécial, dossiers sans bit spécial), posés par descripteur à la création (`fchown`, `fchmod`), jamais d'après le masque ambiant.
8. **Noms en double.** Même chemin exact, ou chemin qui ne diffère que par la casse ou la forme Unicode (NFC puis minuscules, chemin entier et dossiers parents), fichier et dossier du même nom : refus. Aucune entrée « ne gagne ».
9. **Encodage des noms.** UTF-8 valide ou refus, avec le message « Ce fichier contient des noms que Hearth ne sait pas lire. Recrée l'archive avec un outil récent. ». Aucun encodage n'est deviné ; le drapeau UTF-8 de l'entrée et le champ de nom Unicode en « extra » sont ignorés.
10. **Méthodes.** `stored` et `deflate`. Chiffrement (dont AES), autres compressions, ZIP64 hors bornes : refus. Tar : `ustar` et `pax`, noms longs GNU et pax bornés comme un nom, entrées creuses refusées ; `gzip` à un seul membre.
11. **Jamais d'archive dans l'archive.** Un fichier intérieur qui est lui-même une archive est déposé tel quel, jamais ouvert.
12. **Lire un seul fichier** : 1 Mio au plus, sur une archive déjà inspectée.
13. **Ressources.** Annulation à chaque entrée et à chaque tampon de 64 Kio. Un `.tar.gz` ou un tar ne peut traîner plus de 1 Mio de zéros après son dernier bloc.
14. **Écriture : tout par descripteur depuis le dossier PARENT de la destination (Unix seulement).** L'appelant donne une racine de confiance absolue et un chemin relatif ; le parent est ouvert une fois (`O_DIRECTORY | O_NOFOLLOW`, composant par composant). **Précondition vérifiée sur ce descripteur (`fstat`) et refusée sinon (`UNSAFE_DESTINATION`) : le parent appartient à l'utilisateur effectif de l'agent et n'est inscriptible ni par le groupe ni par les autres.** Ensuite, plus aucun appel par chemin : dossier d'attente (`mkdirat`, nom imprévisible, `0700`), ouverture, écritures, `fsync` des fichiers, des dossiers puis du dossier d'attente, bascule `renameat2(RENAME_NOREPLACE)` des deux noms relatifs au même descripteur, `fsync` du parent. Si le noyau ou le système de fichiers ne prend pas en charge l'interdiction de remplacer : refus (`ATOMIC_SWITCH_UNAVAILABLE`), le dossier d'attente est retiré, jamais un `rename` qui remplace. Le nettoyage (refus, panne, annulation) supprime par descripteur, sans suivre un lien, et seulement le dossier dont le périphérique et l'inode sont ceux du dossier d'attente créé par l'opération. L'archive elle-même est ouverte sans suivre de lien ni bloquer sur un tube (`O_NOFOLLOW | O_NONBLOCK`) et doit être un fichier ordinaire (`fstat`). Hors Unix, l'écriture est refusée : il n'existe aucun repli par chemin.
15. **Erreurs.** Un code par cause (`ArchiveRefusal::code`), jamais un nom d'entrée, un chemin du serveur ni un contenu.

## Choix de la lecture : à la main, sur `flate2`

Les crates `zip` (4.6.1) et `tar` (0.4.46) sont dans `Cargo.lock` (client et outils), mais **aucune n'est utilisée par l'agent**. Les griefs ci-dessous viennent de l'examen du brouillon et de leur API publique ; leur code n'a pas été relu ligne à ligne ici (non vérifié) :
- `zip` lit l'index de fin et ne le confronte pas aux en-têtes locaux, ne refuse pas les données préfixées ni les chevauchements, et son extraction suit des règles de nom qui ne sont pas les nôtres ;
- `tar` a `unpack`/`unpack_in` (extraction toute faite), des en-têtes pax et GNU et une gestion des liens que nous voulons refuser, pas contourner.
Les deux formats sont donc lus **à la main** (quelques centaines de lignes chacun, testées octet par octet), sur la seule décompression `deflate` de `flate2` (backend `miniz_oxide`, Rust pur). Aucune fonction d'extraction d'une crate n'est appelée : le code lit les entrées et écrit lui-même par le port.

Dépendances ajoutées (justifiées dans l'ADR-0009) : `flate2` 1.1 (`default-features = false`, `rust_backend`), `unicode-normalization` 0.1 (`std`), `rustix` 1 (`std`, `fs`, `process` pour `geteuid`, Unix seulement). Toutes déjà dans `Cargo.lock`.

## Comment l'appliquer

- Un test par piège, chacun avec une archive fabriquée par le test (aucun binaire versionné) : voir `docs/business-rules/BR-ARCH-*.md`.
- Test par génération aléatoire à graine fixe (`tests_extract.rs`) : mutations d'archives valides et octets aléatoires, aucune panique.
- Tout ce qui écrit (liens, parent, bascule, propriétaire, modes, nettoyage) exige Unix et ne tourne qu'en CI Linux (`tests/archive_fs.rs`, module `unix`, et `infrastructure/archive_fs/tests_unix.rs`) ; sur le poste Windows, la compilation Linux est prouvée par une copie isolée vérifiée pour la cible `x86_64-unknown-linux-gnu`.
- Le motif de refus est typé et dit à l'utilisateur en une phrase ; il ne cite jamais un nom d'entrée dans le journal d'activité.

## Quand NE PAS l'appliquer / limites

- Les sauvegardes écrites par Hearth (ADR-0047, brouillon) gardent leur lecteur et leur manifeste ; la reprise d'un monde depuis l'une d'elles passe quand même par les deux passes.
- Un pack Minecraft n'est pas déballé par l'agent : le programme du jeu le fait dans son conteneur.
- Ce module ne dit rien de ce que les fichiers contiennent.
- `.7z` et `.rar` ne sont pas lus.
- La borne de mémoire est tenue par construction, **pas mesurée** (un allocateur de comptage exige `unsafe`, interdit par le lint `forbid`).
- La racine de confiance de la destination est celle de l'appelant : ses ancêtres ne sont pas vérifiés (seul le parent final l'est, sur son descripteur).
- Un arrêt brutal (coupure) laisse un dossier d'attente `.nom.staging-…` que personne ne ramasse encore (ticket de suivi) ; `fsync` rend l'écriture durable autant que le système de fichiers le permet, ce que seul un essai sur le matériel dirait.
- Les caractères de format sont une table écrite à la main (Unicode 16.0), à relire à chaque montée d'Unicode.
- `read_one` recalcule l'empreinte de tout le fichier à chaque appel ; lire plusieurs fichiers en une passe est un ticket de suivi.
- Pas de campagne de fuzzing réelle (mutations à graine fixe seulement).
- Hors Unix (poste de développement), lire reste possible, écrire est refusé.

## Alternatives rejetées

- **Reporter le `.zip`** : écarté par la réponse 8 de Voiky.
- **Faire confiance à `zip` ou `tar` telles quelles** : voir ci-dessus.
- **Lire le ZIP en flux par ses en-têtes locaux, sans l'index** : c'est l'autre moitié du désaccord ; les outils de l'utilisateur lisent l'index.
- **Deviner l'encodage d'un nom** : un nom mal deviné est un chemin que personne n'a choisi.
- **Écarter les entrées douteuses et garder les autres** : une archive à moitié reprise est un monde ou un pack faux, sans que l'utilisateur le sache.
- **Déballer dans le conteneur du jeu pour tous les usages** : la reprise d'un monde se fait avant que le conteneur existe.

## Conséquences

- Code d'erreur `ARCHIVE_REFUSED { reason }` commun aux trois usages (à brancher par HRT-110 et HRT-124 : ce ticket ne pose aucune route).
- HRT-130 (le module, domaine pur, testable sur le poste) précède HRT-110 et HRT-124.

## Références

- Spécification du format ZIP de PKWARE (`APPNOTE.TXT`) : les noms de structures (enregistrement de fin d'index, en-tête local, descripteur de données, ZIP64) en viennent ; relue de mémoire, non relue sur le texte pendant ce travail.
- Conception technique des fiches (hub), section 13.2 ; vérifications sur les jetons, mods et packs (hub), section 3.
