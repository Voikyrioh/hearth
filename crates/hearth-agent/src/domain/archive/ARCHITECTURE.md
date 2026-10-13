# Architecture — domain/archive

Lecture d'une archive `.zip`, `.tar` ou `.tar.gz` venue d'ailleurs (HRT-130, ADR-0055, règles `BR-ARCH-001` à `011`). L'archive est un contenu HOSTILE, lu par un agent root. **Domaine pur** : le module reçoit une source `Read + Seek` et un port d'écriture, il ne touche aucun fichier. Il n'a ni route, ni écran, ni migration ; HRT-110 (reprendre un monde) et HRT-124 (mods et packs) l'appelleront.

Deux passes liées par l'empreinte SHA-256 du fichier : `inspect` juge toute l'archive et n'écrit rien ; `extract` recalcule l'empreinte, refuse si elle a changé, puis écrit par `ExtractSink`. Un doute refuse l'archive ENTIÈRE.

```
archive/
├── mod.rs      → API : `inspect`, `extract`, `read_one` ; `Inspection`, `EntryInfo`, `ExtractReport` ; détection du format par les octets ; rejeu d'un tar
├── limits.rs   → `ArchiveLimits` : toutes les bornes (8 Gio, 200 000 entrées, 64 Mio d'index, 1 024 octets de nom, 32 niveaux, taux 200/100, 1 Mio de lecture seule)
├── refusal.rs  → `ArchiveRefusal` : un code par cause, aucun nom d'entrée ni chemin dans un message ; `from_io` reprend un refus glissé dans une erreur d'entrée-sortie
├── path.rs     → `judge` (un nom brut devient un `ArchivePath` ou un refus) et `NameRegistry` (doublons, casse, forme Unicode NFC, fichier contre dossier)
├── zip.rs      → lecteur de zip à la main : enregistrement de fin (et ZIP64), index, confrontation de chaque en-tête local, pavage du fichier sans trou
├── tar.rs      → `TarWalker` : ustar, pax, noms longs GNU, tailles, fin ; gzip.rs : flux gzip à un seul membre, en-tête borné, pied vérifié, taux surveillé
├── flow.rs     → annulation, empreinte du fichier, `pump` (taille exacte, un octet d'excès au plus, CRC-32)
├── sink.rs     → port `ExtractSink` : l'adaptateur écrit sans suivre de lien (`infrastructure/archive_fs.rs`)
└── testkit.rs  → (tests) fabrique zip, tar et gzip octet par octet, piégés ; `MemSink`
```

Tests : `tests_paths.rs` (noms), `tests_zip.rs` (zip), `tests_tar.rs` (tar et gzip), `tests_extract.rs` (extraction, annulation, génération aléatoire à graine fixe). Le test de liens contre le vrai système de fichiers est `tests/archive_fs.rs` (module `unix` : CI Linux seulement).

Règles : BR-ARCH-001 à BR-ARCH-011. Décision : ADR-0055.
