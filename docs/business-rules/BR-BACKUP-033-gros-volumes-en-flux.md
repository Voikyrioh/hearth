---
id: BR-BACKUP-033
domaine: BACKUP
titre: Les volumes d'un serveur de jeu se traitent en flux, sans charger un fichier en mémoire, avec progression ; un fichier de plus de 1 Gio est signalé
statut: active
invariant: false
source: contexts/hearth/conceptions/2026-10-13-fonctionnelle-sauvegardes-service.md (BR-BACKUP-033), contexts/hearth/conceptions/2026-10-13-technique-deploiements-lots-suivants.md (sections 6, 7 et 9.4), HRT-97
maj: 2026-11-14
---

# BR-BACKUP-033 : Les volumes d'un serveur de jeu se traitent en flux, sans charger un fichier en mémoire, avec progression ; un fichier de plus de 1 Gio est signalé

## Règle
La copie lit chaque fichier par blocs vers l'encodeur gzip, hache au passage ; la mémoire ne dépend pas de la taille d'un fichier (le manifeste, une ligne par entrée, est en mémoire : un monde de millions de fichiers pèse des centaines de Mio). La fenêtre nomme les fichiers de 1 Gio ou plus (20 au plus, les plus gros d'abord) ; le compte peut les ignorer pour cette sauvegarde (`ignore_once`) sans changer la liste du service. La vérification relit en flux.

## Application (code)
- `crates/hearth-agent/src/application/backups.rs::estimate_blocking, ::Walk::file`
- `crates/hearth-agent/src/infrastructure/backup_archive.rs`

## Vérification
- `crates/hearth-agent/tests/service_backups_https.rs::a_very_large_file_is_named_in_the_preview_so_the_account_can_skip_it_for_this_backup`

## Cas limites
- Le seuil est injecté dans les tests (7 octets) ; la mesure de 10 Gio et de 1 million de fichiers est hors de chaque PR.

## Règles liées
- BR-BACKUP-009

## Historique
- 2026-11-14 : création (HRT-97, côté agent).
