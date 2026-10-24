import { ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import { type FicheDetail, getLinkBridge } from "@/link";

/**
 * Le détail d'une fiche (HRT-102, BR-FICHE-004) : une lecture, ouverte à tout compte, relue quand le lien revient.
 * `phase` : `loading` (première lecture), `found`, `unknown` (l'agent ne propose pas cette fiche : ni une panne ni une
 * erreur), `failed` (la lecture a échoué ; la fiche déjà lue reste affichée). Une réponse en retard sur une plus récente
 * est écartée.
 */
export function useFicheDetail(
  serverId: () => string,
  ficheId: () => string,
  connected: () => boolean,
) {
  const detail = ref<FicheDetail | null>(null);
  const phase = ref<"loading" | "found" | "unknown" | "failed">("loading");
  let ticket = 0;

  async function load(): Promise<void> {
    const mine = ++ticket;
    try {
      const read = await getLinkBridge().readFiche(serverId(), ficheId());
      if (mine !== ticket) return;
      if (read.kind === "found") {
        detail.value = read.fiche;
        phase.value = "found";
      } else {
        detail.value = null;
        phase.value = "unknown";
      }
    } catch (error) {
      if (mine !== ticket) return;
      phase.value = "failed";
      logUiError(error, "fiche:read");
    }
  }

  watch([serverId, ficheId], () => {
    ticket += 1;
    detail.value = null;
    phase.value = "loading";
  });
  watch(
    [serverId, ficheId, connected],
    ([, , on]) => {
      if (on) void load();
    },
    { immediate: true },
  );

  return { detail, phase, reload: load };
}
