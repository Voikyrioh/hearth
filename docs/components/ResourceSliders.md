# ResourceSliders

Organisme · `apps/desktop/src/components/organisms/ResourceSliders.vue`

Mémoire et processeur d'un service qu'on crée (HRT-146, BR-RES-004, BR-RES-037 à BR-RES-039). À réutiliser tel quel par le formulaire de déploiement (HRT-69), l'installation par fiche (HRT-102) et, pour un service existant, l'onglet « Ressources » (HRT-90).

Les bornes (`capacity`) viennent de `GET /engine/capacity` (`resources/capacity.ts::engineCapacityFromWire`). Sans elles (`null`), le composant dit « Hearth n'a pas pu lire les limites de ce serveur. Le service ne peut pas être envoyé sans elles. », ne montre aucun curseur. La valeur de départ est donnée par l'appelant (`resources/start.ts::startingResources`) : la mémoire recommandée d'une fiche, ou la proposition de BR-RES-039 (2 Go, ou la moitié de la mémoire libre, jamais moins de 256 Mo ; tous les processeurs). Sous le curseur de mémoire : la recommandation et sa preuve (« mesuré », « chiffre de l'éditeur »), ou « Hearth n'a pas de chiffre pour ce programme. » suivie de ce que Hearth propose ; sous la recommandation, « Sous la mémoire recommandée : le serveur peut ralentir ou s'arrêter. » sans bloquer.

Le fil reste en octets (`memoryBytes`) et en millicœurs (`cpuMillis`) ; une valeur vide ou refusée vaut `null`. Pas de crans nommés (`MemoryTiers`), de jauge du total engagé ni de consommation : ils arrivent avec HRT-122 et HRT-89.

- Props : `capacity` (`EngineCapacity | null`), `modelValue` (`Resources`), `recommended` (`{ bytes, proof: "measured" | "measured_idle" | "publisher" } | null`), `memoryNote` (un texte à la place de « Hearth n'a pas de chiffre pour ce programme » quand `recommended` est `null` : une fiche dont la mémoire n'est pas prouvée le dit avec ses mots, HRT-102), `cpuNote` (une ligne sous le curseur de processeur), `disabled`
- Événements et slots : `update:modelValue` (`Resources`)
- Envoi : le composant ne le juge pas. L'appelant, qui a `capacity` et la valeur, garde son bouton d'envoi inactif tant que `resources/ready.ts::resourcesIssue(capacity, valeur)` n'est pas `null` (la raison, `capacity`, `memory` ou `cpu`, lui dit quoi afficher).
- Notes : Tests : `components/organisms/resourceSliders.test.ts`. Règles pures : `src/resources/`.
- Revue UX du 2026-10-24 (constat 18) : une application inconnue dit une seule phrase sous le curseur de mémoire, « Hearth ne connaît pas cette application et propose {x}. Montez ce chiffre si elle en demande plus. » (plus de « Hearth n'a pas de chiffre pour ce programme. » avant).
