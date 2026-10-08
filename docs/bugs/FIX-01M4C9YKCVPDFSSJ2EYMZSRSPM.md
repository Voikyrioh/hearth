---
id: FIX-01M4C9YKCVPDFSSJ2EYMZSRSPM
titre: Le point de contrôle occupé de l'effacement des anciennes empreintes n'était pas lu avant de poser la marque
date_découverte: 2026-10-08
date_correction: 2026-10-08
---

# FIX-01M4C9YKCVPDFSSJ2EYMZSRSPM : marque d'effacement posée sur un point de contrôle partiel

## Symptôme
Après la migration `0008`, l'ouverture de la base efface les octets des anciennes empreintes (`VACUUM`, puis `PRAGMA wal_checkpoint(TRUNCATE)`), puis pose `user_version = 1`. Si un autre processus lisait la base à cet instant, le point de contrôle ne tronquait pas le journal, sans erreur SQL, et la marque était posée quand même : l'effacement n'était plus jamais rejoué et le journal gardait ses anciennes trames.

## Reproduction
`infrastructure::sqlite::tests::the_mark_is_not_set_while_another_reader_keeps_the_journal` : un lecteur garde un instantané, une écriture suit, l'effacement est lancé. Rouge sans la lecture du résultat : la marque était posée.

## Cause root
`PRAGMA wal_checkpoint(TRUNCATE)` rend une ligne `(busy, journal, déplacées)` ; `busy = 1` signifie « pas fini ». Le code n'en lisait pas le résultat.

## Impacté
Code de la PR #39 (HRT-32) seulement, jamais publié. Condition : une sous-commande ou un lecteur ouvert au même instant que le démarrage qui suit la mise à jour.

## Workaround
Aucun nécessaire.

## Correction
`truncate_journal` lit `busy` ; occupé : `ScrubError::JournalBusy`, pas de marque. L'agent **démarre quand même** (`scrub_or_warn` : avertissement au journal), l'effacement est repris au démarrage suivant. `// FIX:01M4C9YKCVPDFSSJ2EYMZSRSPM` dans `infrastructure/sqlite/mod.rs`. La constante de la marque est l'unique source de la valeur écrite.

## Règles
- BR-RESIL-021 (précisée).

## Non-régression
- Le test ci-dessus (rouge sans la lecture, vert avec ; la marque se pose une fois le lecteur parti).

## Références
- Ticket : HRT-32 (suites de la review r2 de la PR #39)
- Code : `crates/hearth-agent/src/infrastructure/sqlite/mod.rs`
