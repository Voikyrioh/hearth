# Architecture — domain/transfers

Les règles d'un transfert reprenable (HRT-95, ADR-0048, `BR-FILES-005`, `013`, `019`). Aucune E/S : l'application écrit, le domaine juge.

```
transfers/
├── mod.rs    → `TransferId` (ULID en majuscules, il entre dans les noms de fichiers de la zone d'attente), `Receiver` (un morceau est accepté s'il est le PROCHAIN, de la bonne taille et de la bonne empreinte ; `finish` compare l'empreinte de la suite des empreintes), `judge_space` et `reserve` (le plus grand de 1 Gio et de 5 % reste libre), `admit` (4 transferts, 2 par compte, 8 dépôts ouverts), `judge_name` (1 à 255 octets, ni séparateur, ni octet nul, ni `.` ni `..`)
└── tests.rs  → les règles ci-dessus, dont la reprise depuis la liste d'empreintes gardée
```

L'application est `application/transfers.rs`, le socket `entrypoint/ws/transfers.rs`, la zone d'attente `infrastructure/transfer_files.rs`.
