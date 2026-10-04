# Hearth — guide pour Claude

Hub et pipeline : `CLAUDE.md` du hub `orga-global` (dépôt privé, prime sur tout). Contexte produit : `contexts/hearth/` du hub (épics, stories, specs, conception technique, tickets `HRT-*`, sessions).

## Avant de coder

1. `ARCHITECTURE.md` puis `docs/INDEX.md`.
2. `docs/adr/` (décisions) et `docs/business-rules/` (règles du domaine touché).
3. Conception : `contexts/hearth/conceptions/2026-10-04-technique-socle.md` du hub (privé) ; les décisions reprises ici sont dans `docs/adr/`.

## Commandes

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Les trois doivent passer avant tout commit.

## Règles

- Architecture hexagonale dans `hearth-agent` et `hearth-link` : `domain/` sans E/S ni dépendance vers axum, SQLx, système. Ports dans `application/ports/`.
- Une règle métier vit dans `domain/` et a sa fiche `docs/business-rules/BR-*.md`, modifiée dans le même commit.
- `unwrap` et `expect` interdits hors tests (lint en erreur). Erreurs : `thiserror` par couche, pas d'`anyhow` ; l'erreur fatale est journalisée une seule fois dans `main`.
- `hearth-proto` : aucun type de framework, aucune E/S.
- Le client ne parle à l'agent que par `hearth-link`. L'interface web n'ouvre aucune connexion réseau.
- Textes d'interface : français, tutoiement, pas de tiret cadratin.
- Git : une branche par ticket (`feat/HRT-nn-slug`), PR, pas de push direct sur `main`. Gel des commits en heures ouvrées (voir hub).
