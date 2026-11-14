---
id: ADR-0058
titre: CI : les jobs Windows (facturés x2) ne font leur travail que si le périmètre client Windows change ; ils restent des contrôles requis qui rapportent toujours un succès
type: convention
statut: acceptée
date: 2026-11-14
portee: projet
remplace: —
liens: [ADR-0001, ADR-0002]
---

# ADR-0058 : CI Windows seulement si le client change

## Contexte

Octobre 2026 : 14 882 minutes Windows facturées sur ce dépôt (jobs `rust` jambe `windows-latest` et `desktop`, à chaque PR et à chaque push). La plupart des PR ne touchent que l'agent (Linux, `crates/hearth-agent`). Voiky a validé de cibler ces minutes (2026-11-14). Les contrôles `desktop`, `rust (windows-latest)`, `rust (ubuntu-latest)` et `agent-install` sont **requis** par le jeu de règles `main : intégrité`.

## Décision

1. Un job `changes` (Linux, quelques secondes) liste les fichiers de la PR (`gh api .../pulls/N/files`) et sort `windows=true|false`. Hors PR (push sur `main`, `workflow_dispatch`) : toujours `true`. Liste illisible : `true`.
2. Périmètre client Windows : `apps/desktop/`, `crates/hearth-link/`, `crates/hearth-proto/`, `xtask/`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `clippy.toml`, `.cargo/`, `.github/workflows/ci.yml`, `.github/actions/`.
3. Les jobs Windows ne sont **jamais sautés par `if:` de job** : un job de matrice sauté ne rapporte pas son nom développé et la PR resterait « Expected - waiting ». Hors périmètre, ils tournent sur `ubuntu-latest` avec toutes leurs étapes ignorées (`if: env.RUN == 'true'`) et rapportent un succès en quelques secondes, sous le même nom.
4. `concurrency` par référence : une PR annule son exécution précédente, `main` ne l'est jamais (`cancel-in-progress: ${{ github.event_name == 'pull_request' }}`).
5. `main`, les lancements manuels et les publications (`publish*.yml`, inchangés) gardent la couverture complète.

## Conséquences

- Une PR agent seule ne consomme plus de minute Windows. Le risque accepté : une PR qui casse le build Windows sans toucher le périmètre (par exemple du code `hearth-agent` compilé sous Windows) n'est vue qu'au push sur `main`.
- Toute nouvelle étape de `rust` ou `desktop` doit porter `if: env.RUN == 'true'`.
- Ajouter une crate ou un dossier partagé avec le client : l'ajouter à l'expression du job `changes`.
