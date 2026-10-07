# Docs — Hearth

Maj : 2026-10-07. Point d'entrée obligatoire des agents (recherche, dev, conception). `ARCHITECTURE.md` = carte du code.

| Dossier | Contenu | Quand le consulter |
|---|---|---|
| [adr/](./adr/INDEX.md) | 25 décisions : monorepo, client Tauri, agent statique, protocole HTTP/WebSocket, TLS épinglage, SQLite, machine à états, mises à jour signées, dépendances de l'agent, du client et de la liaison, service système, pont de liaison et coffre Windows, mise à jour de l'agent à distance, tableau de bord (SVG maison, seuils côté Rust), présence hors de la fenêtre et actions typées, mise à jour du client (greffon Tauri, GitHub Releases, clé embarquée), gestion des comptes depuis le client (règles de format à source unique, une commande typée par action), journal d'activité dans l'app (filtre typé, export, liste virtualisée), identité d'appareil (clé Ed25519 par `ring`, défi sans état, inscription par mot de passe seulement), mode attaque (global, essai unique, session seule refusée sans être détruite, activation par administrateur + mot de passe + clé prouvée, fenêtre de redémarrage sur `boot_id`), case de démarrage de Windows dans l'installateur | avant tout choix technique / nouvelle lib
| [business-rules/](./business-rules/INDEX.md) | Règles domaines : INSTALL, CLIENT, CONN, DASH, RESIL, ACCT, AUDIT, UPDATE, TRUST (177 fiches : BR-CONN-001 à 017 ; BR-RESIL-001 à 020 ; BR-INSTALL-001 à 012 ; BR-CLIENT-001 à 011, 013, 014 ; BR-ACCT-001 à 016 ; BR-DASH-001 à 015 ; BR-AUDIT-001 à 021 ; BR-UPDATE-001 à 019, 024 à 029 ; BR-TRUST-001, 002, 003 à 005, 006, 007, 008, 011 à 021, 022 à 026, 027, 028, 030 à 032, 034, 035) | avant tout dev/fix sur une histoire |
| [open-api/](./open-api/INDEX.md) | 29 endpoints : session (dont le défi de la clé d'appareil), postes de confiance, sécurité (dont le mode attaque), compte, mesures (machine, historique, flux WebSocket), audit, mise à jour de l'agent | avant de toucher une route / un client |
| [components/](./components/INDEX.md) | 78 fiches : atomes, molécules, organismes (dont les cartes du tableau de bord), gabarit, pages, règle `needsLink` | avant de créer un composant / une page |
| [bugs/](./bugs/INDEX.md) | 15 fiches FIX:ULID (installation de l'agent : sqlite, wget, activation au démarrage, CAP_MKNOD, purge, dossier de données ; liaison : adresses MAC, `logout` concurrent, « se souvenir » sans secret, identifiant du carnet, fin de session devançant la réponse) | avant de modifier une zone marquée `FIX:` |
| [runbooks/](./runbooks/INDEX.md) | Procédures opérationnelles : installer l'agent, le mettre à jour à distance, publier une version du client, récupérer l'accès administrateur, sortir du mode attaque | accès perdu, diagnostics |

## Globales (orga-global)

- `docs/adr/INDEX.md` du hub `orga-global` (privé) — ADR globales (règles de code/archi communes)
- `product-descriptions/hearth.md` du hub `orga-global` (privé) — Fiche produit écosystème
