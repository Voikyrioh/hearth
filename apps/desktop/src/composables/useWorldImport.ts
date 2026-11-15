import { computed, onBeforeUnmount, ref, watch } from "vue";
import { logUiError } from "@/errors/report";
import {
  failureOf,
  getLinkBridge,
  type WorldInspection,
  type WorldRef,
  type WorldRefusal,
  type WorldsView,
} from "@/link";

/** Ce que la lecture des mondes a donné. */
export type WorldsState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; view: WorldsView }
  /** La fiche ne dit pas ce qu'est un monde, ou l'agent ne sait pas reprendre un monde : la carte n'existe pas. */
  | { kind: "unsupported" }
  | { kind: "failed" };

/** Ce que l'inspection du monde choisi a donné. */
export type InspectionState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; inspection: WorldInspection }
  | { kind: "refused"; reason: WorldRefusal }
  | { kind: "failed" };

/**
 * La lecture des mondes d'une fiche et l'inspection du monde choisi (HRT-106, `BR-MONDE-002`, `003`, `006`, `039`) : deux
 * LECTURES, ni actes ni journal. La liste est lue une fois, dès que le formulaire peut parler au serveur et que la
 * fiche sait reprendre un monde ; elle n'est ni gardée au-delà de la page ni écrite dans une trace. Une réponse en vol d'un
 * autre serveur, d'une autre fiche ou d'un autre monde n'atterrit nulle part.
 */
export function useWorldImport(
  serverId: () => string,
  ficheId: () => string,
  enabled: () => boolean,
  supported: () => boolean,
) {
  const worlds = ref<WorldsState>({ kind: "idle" });
  const inspection = ref<InspectionState>({ kind: "idle" });
  let reading = 0;
  let inspecting = 0;

  async function load(): Promise<void> {
    const mine = ++reading;
    worlds.value = { kind: "loading" };
    try {
      const read = await getLinkBridge().readWorlds(serverId(), ficheId());
      if (mine !== reading) return;
      worlds.value =
        read.kind === "ready" ? { kind: "ready", view: read.view } : { kind: "unsupported" };
    } catch (error) {
      if (mine !== reading) return;
      // Un agent qui ne connaît pas la route, ou un compte sans le droit : la carte n'est pas proposée, l'installation
      // d'un monde neuf continue (la lecture ne bloque jamais la page).
      const failure = failureOf(error);
      if (failure === null) logUiError(error, "worldImport:list");
      worlds.value =
        failure?.kind === "not_connected" || failure === null
          ? { kind: "failed" }
          : { kind: "unsupported" };
    }
  }

  watch(
    [serverId, ficheId],
    () => {
      reading += 1;
      inspecting += 1;
      worlds.value = { kind: "idle" };
      inspection.value = { kind: "idle" };
    },
    { flush: "sync" },
  );
  watch(
    [serverId, ficheId, enabled, supported],
    ([, , on, can]) => {
      if (on && can && worlds.value.kind === "idle") void load();
    },
    { immediate: true },
  );

  /** Inspecte le monde choisi ; une inspection en vol d'un monde abandonné est ignorée. */
  async function inspect(world: WorldRef): Promise<void> {
    const mine = ++inspecting;
    inspection.value = { kind: "loading" };
    try {
      const read = await getLinkBridge().inspectWorld(serverId(), ficheId(), world);
      if (mine !== inspecting) return;
      inspection.value =
        read.kind === "ready"
          ? { kind: "ready", inspection: read.inspection }
          : { kind: "refused", reason: read.reason };
    } catch (error) {
      if (mine !== inspecting) return;
      if (failureOf(error) === null) logUiError(error, "worldImport:inspect");
      inspection.value = { kind: "failed" };
    }
  }

  /** Oublie l'inspection (un autre monde, un retour au monde neuf). */
  function clear(): void {
    inspecting += 1;
    inspection.value = { kind: "idle" };
  }

  const available = computed(() => worlds.value.kind === "ready");

  onBeforeUnmount(() => {
    reading += 1;
    inspecting += 1;
  });

  return { worlds, inspection, available, load, inspect, clear };
}

export type WorldImportState = ReturnType<typeof useWorldImport>;
