import { watch } from "vue";
import type { RouteLocationNormalized, RouteLocationRaw } from "vue-router";
import { deployDoor, routeOpen } from "@/deploy/availability";
import { type RightsState, rightsGate } from "@/services/rights";
import { useServersStore } from "@/stores/servers";
import { useServiceRightsStore } from "@/stores/serviceRights";
import { useServicesStore } from "@/stores/services";

/** Au plus ce délai pour que la section Services ait un état à montrer avant de juger la route. */
export const DEPLOY_GUARD_WAIT_MS = 3000;

/**
 * La garde de la route du formulaire de déploiement (HRT-69, BR-DEPLOY-001, 027) : un compte sans le droit de gérer les
 * services, un agent qui ne gère pas les services et un moteur inutilisable (absent, non pris en charge, version non
 * prise en charge, pilotage désactivé) sont renvoyés vers la liste, qui explique. Un moteur qui n'est pas « prêt » mais
 * existe laisse le formulaire s'ouvrir : il explique l'état. Le serveur refuse de toute façon (BR-GRANT-004) : cette
 * garde ne fait que ne pas proposer ce qui sera refusé. Sans aucun état connu au bout du délai, la liste s'ouvre.
 */
export async function deployRouteGuard(
  to: RouteLocationNormalized,
): Promise<true | RouteLocationRaw> {
  const id = typeof to.params.id === "string" ? to.params.id : "";
  const list: RouteLocationRaw = { name: "services", params: { id } };
  const server = useServersStore().byId(id);
  if (!server) return list;
  const services = useServicesStore();
  void services.start();
  const entry = await firstState(id);
  if (!entry) return list;
  const rights = await readRights(id);
  const gate = rightsGate({
    state: rights,
    isAdmin: server.role === "admin",
    engine: entry.engine?.state ?? null,
  });
  const door = deployDoor({ engine: entry.engine?.state ?? null, gate });
  return routeOpen(door) ? true : list;
}

/** L'état des services de ce serveur, attendu un court instant s'il n'est pas encore arrivé. */
function firstState(id: string) {
  const services = useServicesStore();
  const known = services.of(id);
  if (known) return Promise.resolve(known);
  return new Promise<ReturnType<typeof services.of>>((resolve) => {
    let stop: () => void = () => undefined;
    const timer = setTimeout(() => {
      stop();
      resolve(undefined);
    }, DEPLOY_GUARD_WAIT_MS);
    stop = watch(
      () => services.of(id),
      (entry) => {
        if (!entry) return;
        clearTimeout(timer);
        stop();
        resolve(entry);
      },
    );
  });
}

/** Le droit « gérer les services » de ce serveur, lu s'il ne l'est pas encore (BR-GRANT-011). */
async function readRights(id: string): Promise<RightsState | undefined> {
  const store = useServiceRightsStore();
  const known = store.of(id);
  if (known && known.phase === "known") return known;
  await store.load(id);
  return store.of(id);
}
