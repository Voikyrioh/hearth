# Architecture — domain/catalog

Fiches de serveur de jeu, format version 2 (HRT-113, ADR-0050, ADR-0051, ADR-0052). **Domaine pur** : ni E/S, ni processus, ni réseau, ni `async`, ni tirage aléatoire (garde de test : `architecture_guard.rs`). Les octets des fiches viennent de l'appelant : `build.rs` à la compilation, plus tard un catalogue distant signé. Aucune route, aucun acte, aucune migration, aucun appel au moteur dans ce dossier : HRT-101 les ajoute autour (`application/catalog.rs`, `application/fiche_install.rs`, `entrypoint/http/catalog.rs`), jamais dedans.

Une fiche = deux fichiers TOML par jeu dans `crates/hearth-agent/catalog/<id>/` : `fiche.toml` (la surface de sécurité : image, ports, dossiers, canaux, secrets, console, contrat, arrêt, mondes) et `settings.toml` (le contenu : thèmes et catalogue de réglages), plus `ids.lock` (registre des identifiants de réglage, vérifié par `cargo xtask catalog-check`). Le format est FERMÉ : tout champ inconnu est refusé, et neuf invariants (I1 à I9) refusent le reste avec un motif typé (BR-FICHE-007).

```
catalog/
├── loading/          → AUTONOME (`super::` seulement) : `build.rs` et `xtask` l'incluent par `#[path]` pour appliquer le chargeur réel.
│   ├── limits.rs     → bornes de l'invariant I9 (256 Kio, 400 réglages, 16 ports, 8 dossiers, 6 canaux, 6 étapes, 20 minutes, libellés)
│   ├── error.rs      → `FicheRefusee { fichier, chemin, ligne, motif }`, `Motif` (un motif par invariant, `invariant()` le nomme), `message_pour(dossier)`
│   ├── value.rs      → alphabets fermés (`Charset`, `first_refused_char`), `ChannelPath` (chemin relatif validé), adresses `https`, ordres, drapeaux, versions pointées, `extract_game_version`, `forbidden_in_file`
│   ├── schema/       → `fiche.rs` et `settings.rs` : structures serde, `deny_unknown_fields` à tous les niveaux
│   ├── memory.rs     → mémoire recommandée et crans (HRT-122) : `TierId`, `memory_tiers` (crans servis, jamais un chiffre calculé d'un autre), `recommended_gap` (règle de publication)
│   ├── model.rs      → `Fiche`, `Setting`, `ValueSpec`, `validate_value` (une valeur contre son type), `fiche_digest` (SHA-256 des deux fichiers avec leurs longueurs)
│   ├── loader.rs     → `load(origine, octets)` : taille, UTF-8, analyse fermée, bornes, invariants, préréglages résolus, cohérence des réglages
│   ├── invariants.rs → I1 à I9 : `check_bounds`, `check_fiche_file`, `check_cross`, `check_url`, `check_derive`
│   └── ids_lock.rs   → `ids.lock` : un identifiant ne disparaît pas par oubli, ne change pas de clé sans `rename`, n'est pas réutilisé
├── formats/          → `kv_lines` et `xml_properties`, écrits à la main : lecture et retouche SANS PERTE d'un fichier de réglages (identité, localité, refus plutôt que perte). `contract.rs` : la suite de contrat unique passée par les deux formats
├── settings_state.rs → trois états d'un réglage (valeur, proposée, non écrit), validation d'un changement, effet DÉDUIT du canal, écart avec le fichier, réglages imposés défaits
├── stop_plan.rs      → plan d'arrêt borné ; un élément `untested` n'est jamais exécuté ni cru
├── translate.rs      → fiche + variante + réponses + secrets + contrats donnent un `ServiceSpecDraft` ; interface du validateur du déploiement ordinaire (`DeploymentValidator`, implémentée par `domain::services::deploy::OrdinaryDeployment`, branchée par `domain::services::from_fiche`, HRT-101)
├── generate.rs       → un secret tiré par l'agent (HRT-101) : `secret_from_bytes` / `draw_secret` transforment des octets (le port `SecretRandom`) en secret `alnum` de la longueur déclarée, octets biaisés rejetés, abandon plutôt que boucle ; aucun tirage ici
├── readiness.rs      → le signe « le jeu est prêt » (sous-chaîne LITTÉRALE, lignes postérieures au lancement seulement, jamais un signe `untested`) et les phases d'un serveur de jeu (`phase_of`, `ReadyRule`) — HRT-101, BR-FICHE-028, 056
└── embedded.rs       → la table des fiches embarquées, engendrée par `build.rs` (`include!` de `OUT_DIR`) ; `keep_offered(fiches, include_preview)` (la seule porte, appelée par `CatalogService`) : les fiches `published`, plus les `preview` dans une construction d'essai (`HEARTH_PREVIEW_FICHES`, HRT-101)
```

Tests : `test_support.rs` (une fiche minimale et une fiche riche, dont les fiches piégées sont dérivées d'UNE modification), `tests_fiches.rs` (format fermé, I1 à I9, cohérence, Terraria, `ids.lock`), `tests_state.rs` (réglages, écart, arrêt), `tests_translate.rs` (traduction et test de propriété à graine fixe), `architecture_guard.rs`.

Règles : BR-FICHE-005 à 011, 028, 037, 038, 046 à 049, 056 ; BR-RES-031 à 033, 038 ; BR-JEU-002, 038, 039, 040.
