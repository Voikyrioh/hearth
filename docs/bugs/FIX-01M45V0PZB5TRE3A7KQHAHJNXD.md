---
id: FIX-01M45V0PZB5TRE3A7KQHAHJNXD
titre: Une erreur de lecture de la base était comptée comme « 0 administrateur »
date_découverte: 2026-10-05
date_correction: 2026-10-05
---

# FIX-01M45V0PZB5TRE3A7KQHAHJNXD : lecture des comptes de l'installation

## Symptôme
`hearth-agent install` sur une machine déjà installée dont la base est illisible (verrouillée, corrompue, droits) croit qu'il n'y a aucun administrateur : il demande, ou crée, un premier compte.

## Cause root
`infrastructure/sqlite/mod.rs::count_admins_read_only` faisait `.unwrap_or(0)` sur toute erreur de la requête `SELECT COUNT(*)`, pas seulement sur « table absente ».

## Impacté
Installation (réinstallation, mise à niveau, réparation) depuis HRT-15.

## Workaround
Aucun.

## Correction
Seule l'erreur SQLite « no such table » donne 0 ; toute autre erreur remonte en `DatabaseError::Open` et l'installation s'arrête sans rien modifier. Test `counting_admins_tells_a_missing_table_from_an_unreadable_database`.

## Références
- Ticket : HRT-17 (suivi de la review de HRT-15, PR #11)
- BR : BR-INSTALL-002
