# Docs — Hearth

Maj : 2026-10-05. Point d'entrée obligatoire des agents (recherche, dev, conception). `ARCHITECTURE.md` = carte du code.

| Dossier | Contenu | Quand le consulter |
|---|---|---|
| [adr/](./adr/INDEX.md) | 14 décisions : monorepo, client Tauri, agent statique, protocole HTTP/WebSocket, TLS épinglage, SQLite, machine à états, mises à jour signées, dépendances de l'agent, du client et de la liaison, service système, pont de liaison et coffre Windows, mise à jour de l'agent à distance | avant tout choix technique / nouvelle lib |
| [business-rules/](./business-rules/INDEX.md) | Règles domaines : INSTALL, CLIENT, CONN, DASH, RESIL, ACCT, AUDIT, UPDATE (127 fiches : BR-CONN-001 à 017 ; BR-RESIL-001 à 020 ; BR-INSTALL-001 à 012 ; BR-CLIENT-001 à 011, 013, 014 ; BR-ACCT-001 à 016 ; BR-DASH-001 à 015 ; BR-AUDIT-001 à 021 ; BR-UPDATE-011 à 019, 024, 027, 028, 029) | avant tout dev/fix sur une histoire |
| [open-api/](./open-api/INDEX.md) | 24 endpoints : session, compte, mesures (machine, historique, flux WebSocket), audit, mise à jour de l'agent | avant de toucher une route / un client |
| [components/](./components/INDEX.md) | 45 fiches : atomes, molécules, organismes, gabarit, pages, règle `needsLink` | avant de créer un composant / une page |
| [bugs/](./bugs/INDEX.md) | 6 fiches FIX:ULID (installation de l'agent : sqlite, wget, activation au démarrage, CAP_MKNOD, purge, dossier de données) | avant de modifier une zone marquée `FIX:` |
| [runbooks/](./runbooks/INDEX.md) | Procédures opérationnelles : installer l'agent, le mettre à jour à distance, récupérer l'accès administrateur | accès perdu, diagnostics |

## Globales (orga-global)

- `docs/adr/INDEX.md` du hub `orga-global` (privé) — ADR globales (règles de code/archi communes)
- `product-descriptions/hearth.md` du hub `orga-global` (privé) — Fiche produit écosystème
