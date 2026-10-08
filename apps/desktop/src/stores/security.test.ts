import { flushPromises } from "@vue/test-utils";
import { afterEach, describe, expect, it } from "vitest";
import { mountContext } from "@/test/mount";
import { useSecurityStore } from "./security";

// HRT-18 (suite) : l'effacement en attente vient de la LECTURE `GET /security`, que le flux ne porte pas. Quand
// un événement du flux plus récent que la lecture garde la main et la lecture n'apporte
// que le poste, la clé et l'effacement : la branche de fusion du magasin. (Le pont simulé fait avancer le
// numéro à chaque lecture : sans événement concurrent, une lecture passe toujours par l'autre branche, celle
// que le test de page `SecurityMode.test.ts` exerce.) Ce test rougit si la ligne qui recopie
// `erasurePending` dans la fusion est retirée.
afterEach(() => {
  document.body.innerHTML = "";
});

describe("security store : the pending erasure", () => {
  it("keeps the stream event's alert and takes the pending erasure from a read that it overtook", async () => {
    const { bridge } = await mountContext();
    const store = useSecurityStore();
    await store.start();
    await store.load("forge");
    await flushPromises();
    expect(store.of("forge")?.state?.erasurePending).toBe(false);

    bridge.security.setErasurePending("forge", true);
    const original = bridge.getSecurity.bind(bridge);
    bridge.getSecurity = async (serverId: string) => {
      // Un événement du flux (alerte) est arrivé AVANT la réponse, et la réponse porte un numéro plus ancien
      // (course) : l'événement garde la main, la lecture n'apporte que le poste, la clé et l'effacement.
      bridge.security.setAlert(serverId, { own: true, others: 1 });
      const read = await original(serverId);
      if (read.kind !== "known") return read;
      return { kind: "known", state: { ...read.state, seq: 1 } };
    };
    await store.load("forge");
    await flushPromises();
    const merged = store.of("forge")?.state;
    expect(merged?.alert.own).toBe(true);
    expect(merged?.erasurePending).toBe(true);
  });
});
