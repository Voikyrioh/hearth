---
id: FIX-01M7Q1M71TKSBWVV0C0GTXWWXZ
titre: Un serveur Minecraft limité en mémoire était tué par le noyau : la JVM ne voyait pas la limite du conteneur (HRT-295)
date_découverte: 2026-11-17
date_correction: 2026-11-18
---

# FIX-01M7Q1M71TKSBWVV0C0GTXWWXZ : la JVM reçoit la limite de mémoire du service

## Symptôme
Sur la forge, le serveur Minecraft (image itzg, Temurin 17.0.15) a été tué trois fois en 24 h par le noyau (`journalctl -k` : OOM kill du groupe de contrôle mémoire). Ce n'est pas une fuite : tas de 3,7 Gio à froid, hors tas 560 Mio.

## Reproduction
Noyau 6.18, JDK 17 : la JVM écrit « controller memory is not enabled / One or more required controllers disabled at kernel level ». `-XX:+PrintFlagsFinal` donne `MaxHeapSize` à 75 % de la mémoire de l'HÔTE (32 Gio, soit 23,5 Gio) alors que `memory.max` du conteneur vaut 12 Gio.

## Cause root
La fiche passait `JVM_XX_OPTS=-XX:MaxRAMPercentage=75` et `MEMORY=""` : elle comptait sur la détection de conteneur de la JVM (`UseContainerSupport`) pour que les 75 % s'appliquent à la limite du service. Sur ce noyau la détection échoue, la JVM prend la mémoire de l'hôte, le tas grandit au-delà de la limite.

## Impacté
Tout serveur Minecraft installé par fiche avec une limite de mémoire inférieure à la mémoire de l'hôte, sur un noyau où la JVM ne lit pas les groupes de contrôle.

## Workaround
Aucun côté Hearth ; à la main, ajouter `-XX:MaxRAM=<octets>` aux options de la JVM.

## Correction
- La fiche déclare quelle variable porte les options de la JVM : `jvm_max_ram = true` sur la ligne `[[env]]` de `JVM_XX_OPTS` (`schema/fiche.rs::EnvVar`, invariant : exige une valeur fixe). `Fiche::jvm_options_env()` la restitue.
- `domain/services/plan.rs::plan_from_spec` (constructeur unique du plan, ADR-0041) reçoit ce nom par `PlanContext::jvm_options_env` et, pour cette variable seulement, ajoute ` -XX:MaxRAM=<memory_bytes>` (limite obligatoire du service, BR-DEPLOY-038). `MaxRAMPercentage=75` garde son sens : 75 % de cette limite. Une option `MaxRAM` déjà présente n'est pas dupliquée. Aucun champ nouveau dans `ContainerPlan`, aucune autre option ni variable touchée.
- Les deux appelants (`application/deployments.rs::plan_of`, `application/service_update.rs::plan_of`) lisent la déclaration dans la fiche EMBARQUÉE de l'agent (`Deployments::jvm_options_env`), pas dans la description stockée : un serveur déjà installé reçoit le correctif à sa prochaine recréation (enregistrer un réglage suffit), sans montée de version de fiche. La description stockée garde `-XX:MaxRAMPercentage=75` : aucune dérive n'apparaît.
- Un service d'une autre fiche, ou sans fiche, n'a aucune option ajoutée.
- La variable déclarée mais ABSENTE de la description (un réglage l'a retirée) est ajoutée avec la seule limite (`-XX:MaxRAM=<octets>`) : le correctif ne se perd pas en silence (revue du 23/11, test `an_absent_jvm_variable_is_added_with_the_memory_limit_alone`).
Non-régression : `domain::services::plan::tests::a_declared_jvm_variable_receives_the_memory_limit_and_nothing_else_changes`, `..::another_variable_is_left_alone_and_an_existing_max_ram_is_not_duplicated`, `domain::catalog::tests_minecraft` (la fiche déclare `JVM_XX_OPTS`), `tests/minecraft_install_https.rs` (valeur `-XX:MaxRAMPercentage=75 -XX:MaxRAM=10737418240` dans le plan créé).
Banc réel (manuel, forge) : `-XX:+PrintFlagsFinal` doit montrer un `MaxHeapSize` d'environ 9 Gio pour une limite de 12 Gio.

## Références
- Ticket : HRT-295 (lot 0.2.2)
- Règles : BR-DEPLOY-035 (plan fermé), BR-DEPLOY-038 (limites obligatoires), BR-FICHE-009
- Code : `crates/hearth-agent/src/domain/services/plan.rs` (marqueur `FIX:01M7Q1M71TKSBWVV0C0GTXWWXZ`)
