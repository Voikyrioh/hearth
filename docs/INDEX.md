# Docs — Hearth

Maj : 2026-10-04. Point d'entrée obligatoire des agents (recherche, dev, conception). `ARCHITECTURE.md` = carte du code.

| Dossier | Contenu | Quand le consulter |
|---|---|---|
| [adr/](./adr/INDEX.md) | 9 décisions : monorepo, client Tauri, agent statique, protocole HTTP/WebSocket, TLS épinglage, SQLite, machine à états, mises à jour signées, dépendances de l'agent | avant tout choix technique / nouvelle lib |
| [business-rules/](./business-rules/INDEX.md) | Règles domaines : INSTALL, CLIENT, CONN, DASH, RESIL, ACCT, AUDIT, UPDATE (25 fiches : BR-CONN-001, 006, 007, 013, 014 ; BR-RESIL-010, 012, 014 ; BR-INSTALL-004 ; BR-ACCT-001 à 016) | avant tout dev/fix sur une histoire |
| [open-api/](./open-api/INDEX.md) | 23 endpoints : session, compte, mesures, audit, mise à jour | avant de toucher une route / un client |
| [components/](./components/INDEX.md) | 21 composants Vue : atoms, molecules, organisms, pages | avant de créer un composant / une page |
| [bugs/](./bugs/INDEX.md) | Index vide, fiches FIX:ULID à créer au besoin | avant de modifier une zone marquée `FIX:` |
| [runbooks/](./runbooks/INDEX.md) | Procédures opérationnelles : récupérer l'accès administrateur | accès perdu, diagnostics |

## Globales (orga-global)

- `docs/adr/INDEX.md` du hub `orga-global` (privé) — ADR globales (règles de code/archi communes)
- `product-descriptions/hearth.md` du hub `orga-global` (privé) — Fiche produit écosystème
