# Architecture — domain/files

Ce que le domaine sait d'un chemin sous la racine d'un service (HRT-145, ADR-0046). Aucune E/S.

```
├── mod.rs       → exports et bornes
└── rel_path.rs  → `RelPath` : composants validés (ni vide, ni absolu, ni `.` ni `..`, sans octet nul ; 255 octets par composant, 4 096 au total, 64 niveaux), `RelPath::root()` pour la racine elle-même. Seule `parse` en fabrique : posséder un `RelPath`, c'est que le chemin a passé tous les refus AVANT tout appel au système (`BR-FILES-003` d)
```

L'adaptateur est `infrastructure/fs/` ; le port, `application/ports/safe_fs.rs`.
