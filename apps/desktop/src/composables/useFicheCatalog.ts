import { ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { type Catalog, getLinkBridge } from "@/link";

/**
 * Le catalogue des fiches d'un serveur (HRT-102, BR-FICHE-001, 002) : une lecture, ouverte à tout compte, relue quand le
 * lien revient. Les fiches sont livrées avec l'agent : le catalogue fonctionne sans Internet. Une réponse en retard sur
 * une plus récente est écartée. `phase` : `loading` (première lecture), `ready`, `failed` (la dernière lecture a échoué ;
 * le catalogue déjà lu reste affiché).
 */
export function useFicheCatalog(serverId: () => string, connected: () => boolean) {
  const catalog = ref<Catalog | null>(null);
  const phase = ref<"loading" | "ready" | "failed">("loading");
  let ticket = 0;

  async function load(): Promise<void> {
    const mine = ++ticket;
    try {
      const read = await getLinkBridge().readCatalog(serverId());
      if (mine !== ticket) return;
      catalog.value = read;
      phase.value = "ready";
    } catch (error) {
      if (mine !== ticket) return;
      phase.value = "failed";
      logUiError(error, "catalogue:read");
    }
  }

  watch(serverId, () => {
    ticket += 1;
    catalog.value = null;
    phase.value = "loading";
  });
  watch(
    [serverId, connected],
    ([, on]) => {
      if (on) void load();
    },
    { immediate: true },
  );

  return { catalog, phase, reload: load };
}
