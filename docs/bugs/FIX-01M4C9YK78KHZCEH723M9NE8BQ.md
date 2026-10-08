---
id: FIX-01M4C9YK78KHZCEH723M9NE8BQ
titre: La copie de la base de la mise à jour gardait les anciennes empreintes jusqu'à sa suppression simple
date_découverte: 2026-10-07
date_correction: 2026-10-08
---

# FIX-01M4C9YK78KHZCEH723M9NE8BQ : `update/hearth.db.before` supprimée sans être écrasée

## Symptôme
La copie d'avant l'échange des binaires (`update/hearth.db.before`, `hearth.db-wal.before`, BR-UPDATE-029) est prise avant la migration `0008` : elle contient les anciennes empreintes de requêtes (corps avec mots de passe, FIX-01M4BZN31A8Z8WN0WKNTCRTFFN). À la fin du travail elle était supprimée par `remove_file` : les blocs restaient lisibles sur le disque.

## Reproduction
`infrastructure::update::host::tests::the_database_copies_are_overwritten_with_zeros_before_removal` : un lien dur pris avant `clear_staging` voit les mêmes octets ; rouge sans l'écrasement (le marqueur est encore là), vert avec (zéros).

## Cause root
`FsUpdateHost::clear_staging` retirait la copie sans l'écraser.

## Impacté
Toute mise à jour de l'agent faite avec le code de la PR #39 (jamais publié).

## Workaround
Aucun.

## Correction
`clear_staging` renomme la copie de la base et son journal en `.erasing` (un fichier de zéros ne porte jamais le nom que la remise de la base lit), les écrase de zéros en place avec `fsync`, puis les supprime (`erase_copy`) ; un `.erasing` laissé par une panne est fini au nettoyage suivant ; un lien symbolique n'est jamais suivi (le lien est retiré, sa cible intacte) ; si l'écrasement échoue, la suppression a lieu quand même. Une mise à jour abandonnée garde ses copies en clair, exprès, jusqu'au geste de l'opérateur. Meilleur effort : sur un système de fichiers à copie sur écriture ou un disque qui remappe ses blocs, l'ancien contenu peut subsister. `// FIX:01M4C9YK78KHZCEH723M9NE8BQ`. Hors périmètre, dit : un retour arrière remet la base d'avant telle quelle (anciennes empreintes comprises), puis la `0008` et son effacement physique rejouent à la mise à jour suivante ; une purge de désinstallation supprime sans écraser.

## Règles
- BR-UPDATE-029 (note), BR-RESIL-021 (précisée).

## Non-régression
- Le test ci-dessus.

## Références
- Ticket : HRT-32 (suites de la review r2 de la PR #39)
- Code : `crates/hearth-agent/src/infrastructure/update/host.rs`
