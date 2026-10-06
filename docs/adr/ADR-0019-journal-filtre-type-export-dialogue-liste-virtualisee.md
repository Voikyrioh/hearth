---
id: ADR-0019
titre: Journal d'activité dans l'app : filtre typé, export par la boîte d'enregistrement du système, liste virtualisée
type: architecture
statut: acceptée
date: 2026-10-06
portee: projet
remplace: —
liens: [ADR-0002, ADR-0013, ADR-0016, BR-AUDIT-001, BR-AUDIT-010, BR-AUDIT-013, BR-AUDIT-014, BR-AUDIT-015, BR-AUDIT-017, HRT-14]
---

# ADR-0019 — Journal d'activité dans l'app

## Contexte

HRT-14 affiche le journal de l'agent (HRT-05) : plusieurs dizaines de milliers d'entrées, du texte saisi par des tiers (identifiants tentés, noms de poste), un flux en direct, un export. Trois décisions structurantes : comment l'interface lit le journal sans commande générique (ADR-0013, 0016), comment l'export atteint un fichier sans que la page ne désigne un chemin, comment tenir des milliers de lignes.

## Décision

1. **Une lecture = une commande typée.** `read_audit(server_id, filter, before)` et `export_audit(server_id, filter)`. Le filtre (`AuditFilterDto`) est un objet métier : identifiants de comptes, types d'action de la liste fermée de la spec, résultats, bornes de dates (secondes), texte de recherche. Il est validé dans `hearth-link` (`domain/audit_query.rs` : 50 comptes au plus, identifiants sans virgule ni caractère de contrôle, texte de 200 caractères au plus, période ordonnée et plausible, curseur strictement positif) ; la route, la méthode (toujours `GET`) et la requête encodée sont construites là et nulle part ailleurs. Aucun paramètre de la page n'est un chemin, une méthode, une adresse, une requête brute, un corps ou des en-têtes (`tests/capabilities.rs` reste vert sans exception).
2. **Les types d'action de la spec couplent action et résultat** (« Connexion réussie » = connexion et réussi) : l'agent combine ses filtres par ET, pas par paires. `AuditFilter::plan` produit une requête par rectangle (actions × résultats), fusionnées quand elles partagent les mêmes résultats ou les mêmes actions ; `merge_pages` recolle les pages sans doublon ni perte (les `limit` plus récentes de l'union sont parmi les `limit` plus récentes de l'une des requêtes). Un type qui contredit le filtre de résultat ne retient rien et n'interroge pas l'agent.
3. **Export : le fichier est choisi par l'UTILISATEUR.** La coquille ouvre la boîte d'enregistrement du système (`rfd`, déjà embarquée pour la boîte d'erreur de démarrage : AUCUNE dépendance nouvelle, ni aws-lc ni OpenSSL) rattachée à la fenêtre, avec un nom suggéré ; le système demande lui-même confirmation avant d'écraser. La page ne fournit ni chemin ni nom ; annuler n'écrit rien. Lecture d'abord, boîte ensuite : une erreur de lecture n'ouvre pas de boîte. Pas de greffon de dialogue Tauri, donc aucune permission supplémentaire côté web.
4. **Neutralisation des formules : UNE implémentation, `hearth_proto::api::audit_csv::field`** (`=`, `+`, `-`, `@`, tabulation, retour chariot : apostrophe en tête, puis guillemets si besoin). L'agent (`GET /audit/export`) et la liaison cliente (export d'un résultat qui exige plusieurs requêtes : `render_items`) passent par elle. Le client n'ajoute rien au fichier de l'agent : une seconde neutralisation doublerait l'apostrophe.
5. **Regroupement des rafales : une source côté interface** (`audit/grouping.ts`), parce que la règle (BR-AUDIT-013 : au moins 5 refus de la même adresse en 2 minutes) porte sur ce qui est affiché. Ce n'est pas la condensation de l'agent (`repeat_count` : événements IDENTIQUES d'une même minute), que l'interface compte pour `1 + repeat_count` et n'essaie pas de refaire. Un regroupement ne contient que des refus de connexion de la même adresse qui se suivent dans la liste : une autre entrée coupe la rafale, rien n'est masqué ni déplacé.
6. **Liste : fenêtre bornée et virtualisée, sans bibliothèque.** Hauteur de ligne fixe (jeton `--audit-row-height`), seules les lignes visibles et quelques voisines existent dans le DOM (`AuditTable`), dimensions posées par propriétés CSS. En mémoire : 3 000 lignes au plus (le bas est coupé quand des entrées arrivent en tête, la suite se relit par curseur), 500 entrées au plus derrière « N nouvelles entrées » (au-delà, on garde les plus anciennes et on relit la tête au clic). Une bibliothèque de virtualisation aurait ajouté une dépendance pour 60 lignes de code et une grille ARIA qu'il faut de toute façon écrire.
7. **Justesse.** La liste est une suite contiguë du journal filtré sans doublon (par `id`), du plus récent au plus ancien ; une entrée en direct s'ajoute en tête seulement si l'utilisateur y est. Au retour du lien : relecture de la tête jusqu'à retrouver ce qui est déjà là, ou repartir de la tête si le trou dépasse 10 pages. Un filtre actif vaut aussi pour le direct : l'entrée reçue ne sert que de signal, l'agent filtre (une relecture de la tête après 300 ms de calme) ; les règles de recherche de l'agent (accents, début de mot) ne sont jamais recopiées côté interface.
8. **Fuseau.** L'heure s'affiche dans le fuseau du PC (`Intl.DateTimeFormat`) ; la source UTC (`at`) est conservée dans l'entrée, visible dans l'infobulle et le détail, et c'est elle que contient l'export.
9. **Texte non fiable.** Toute valeur du journal passe par `audit/text.ts` : caractères de contrôle, séparateurs de ligne et inversions de sens de lecture remplacés par une marque, troncature par points de code, texte complet dans l'infobulle et la boîte de détail. Aucun `v-html`.

## Comment l'appliquer

- Nouvelle lecture du journal : une commande typée de plus (les 5 endroits de l'ADR-0010), jamais un paramètre libre.
- Tout nouveau chemin d'export réutilise `audit_csv::field`.
- Vérification : `cargo tree -p hearth-desktop -i aws-lc-rs` et `-i openssl` ne trouvent rien.

## Alternatives écartées

- **Greffon `tauri-plugin-dialog`** : dépendance et permissions de plus pour ce que `rfd` fait déjà.
- **Chemin d'export choisi côté page** : une page compromise écrirait où elle veut.
- **Filtrer le direct côté interface** : recopie des règles de recherche de l'agent, qui divergeraient.
- **Exporter toujours depuis les pages JSON** : jusqu'à 100 requêtes pour 10 000 entrées ; l'export de l'agent reste la voie normale.

## Conséquences

- `GET /audit/export` de l'agent reste la source du fichier dans le cas courant ; la liaison ne le réécrit que pour une union de types d'action.
- Limite connue : le choix de compte du filtre propose les comptes que le journal a montrés (la liste des comptes viendra avec HRT-13).
- À vérifier sur une vraie machine Windows : la boîte d'enregistrement (position, confirmation d'écrasement), le rendu de la grille au clavier avec un lecteur d'écran.
