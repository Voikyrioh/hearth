import { enableAutoUnmount } from "@vue/test-utils";
import { afterEach } from "vitest";
import { dropAllKeptBatches } from "@/composables/useServiceMods";

// Les bulles sont téléportées dans `body` : sans démontage, celles d'un test resteraient dans le suivant.
enableAutoUnmount(afterEach);
// Les lots de mods non appliqués vivent au niveau du service (en mémoire) : aucun ne passe d'un test au suivant.
afterEach(() => dropAllKeptBatches());
