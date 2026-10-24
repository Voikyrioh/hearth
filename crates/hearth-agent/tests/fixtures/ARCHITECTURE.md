# Architecture — tests/fixtures

Données de test de l'agent, lues par `include_str!` (jamais embarquées dans le binaire, jamais chargées par `build.rs`).

```
fixtures/
└── rich/                → la fiche de fabrication « riche » (HRT-101) : `fiche.toml` et `settings.toml` exercent tout le format de fiche (variantes, trois dossiers, trois canaux, secret tiré, contrat, arrêt et signe « prêt » essayés, ressources avec preuve). UNE seule copie : `domain/catalog/test_support.rs` (`RICH_FICHE`, `RICH_SETTINGS`) et les tests d'intégration (`catalog_https.rs`, `fiche_install_https.rs`, `fiche_install_secrets.rs`) la lisent ; aucune ne la recopie.
```

Ce n'est PAS une fiche du catalogue : le dossier `crates/hearth-agent/catalog/` est le seul que `build.rs` embarque.
