import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Transfer } from "@/link";
import { SimulatedLinkBridge, setLinkBridge } from "@/link";
import { POLL_MS, useTransfersStore } from "@/stores/transfers";
import TransfersStrip from "./TransfersStrip.vue";

// Le panneau « Transferts » (HRT-96, BR-FILES-019) : un envoi par ligne, les gestes qui ont un sens, rien ne repart seul.
const MIB = 1024 * 1024;
const PASSWORD = "Correct-Horse-9";

beforeEach(() => setActivePinia(createPinia()));
afterEach(() => {
  setLinkBridge(null);
  vi.useRealTimers();
});

function item(patch: Partial<Transfer> = {}): Transfer {
  return {
    id: "01J9ZY0G3Q8M2K6W4T7V5N1B9D",
    serverId: "forge",
    name: "pack.zip",
    size: 600 * MIB,
    sent: 150 * MIB,
    status: "sending",
    refusal: null,
    ...patch,
  };
}

function strip(transfers: Transfer[]) {
  return mount(TransfersStrip, { props: { transfers } });
}

describe("le panneau", () => {
  it("est absent quand il n'y a aucun envoi", () => {
    expect(strip([]).find("[data-transfers-strip]").exists()).toBe(false);
  });

  it("montre le nom, l'état et la progression en octets d'un envoi en cours, avec « Annuler » seulement", () => {
    const wrapper = strip([item()]);
    expect(wrapper.find("[data-transfer-name]").text()).toBe("pack.zip");
    expect(wrapper.find("[data-transfer-status]").text()).toBe("Envoi en cours.");
    expect(wrapper.find("[data-transfer-progress]").text()).toBe("150 Mo sur 600 Mo");
    expect(wrapper.find("[data-transfer-cancel]").exists()).toBe(true);
    expect(wrapper.find("[data-transfer-resume]").exists()).toBe(false);
    expect(wrapper.find("[data-transfer-dismiss]").exists()).toBe(false);
  });

  it("un envoi interrompu propose « Reprendre l'envoi » et « Annuler » ; reprendre émet, rien ne repart seul", async () => {
    const transfers = [item({ status: "interrupted" })];
    const wrapper = strip(transfers);
    expect(wrapper.find("[data-transfer-status]").text()).toContain("Reprenez l'envoi");
    await wrapper.find("[data-transfer-resume]").trigger("click");
    expect(wrapper.emitted("resume")).toHaveLength(1);
    expect(wrapper.emitted("cancel")).toBeUndefined();
    await wrapper.find("[data-transfer-cancel]").trigger("click");
    expect(wrapper.emitted("cancel")).toHaveLength(1);
  });

  it("un envoi fini se retire de la liste ; un envoi annulé n'a ni jauge ni progression", async () => {
    const wrapper = strip([item({ status: "cancelled" })]);
    expect(wrapper.find("[data-transfer-progress]").exists()).toBe(false);
    expect(wrapper.find("svg").exists()).toBe(false);
    await wrapper.find("[data-transfer-dismiss]").trigger("click");
    expect(wrapper.emitted("dismiss")).toHaveLength(1);
    expect(wrapper.find("[data-transfer-cancel]").exists()).toBe(false);
  });

  it("le manque de place est dit avec ce qui manque et ce qui resterait (BR-FILES-013)", () => {
    const wrapper = strip([
      item({
        status: "interrupted",
        refusal: { kind: "no_space", missing: 3 * 1024 * MIB, remaining: 512 * MIB },
      }),
    ]);
    expect(wrapper.find("[data-transfer-status]").text()).toBe(
      "Il manque 3 Go sur le disque pour écrire ce fichier. Il resterait 512 Mo de libre.",
    );
    expect(wrapper.find("[data-transfer-resume]").exists()).toBe(true);
  });

  it("au-delà de trois envois, « +N autres » déplie la liste", async () => {
    const many = ["A", "B", "C", "D", "E"].map((n) => item({ id: `ID${n}`, name: `${n}.zip` }));
    const wrapper = strip(many);
    expect(wrapper.findAll("[data-transfer]")).toHaveLength(3);
    await wrapper.find("[data-transfers-more]").trigger("click");
    expect(wrapper.findAll("[data-transfer]")).toHaveLength(5);
  });
});

describe("le magasin", () => {
  it("relit les envois à chaque passage tant que l'un envoie, puis une fois sur cinq", async () => {
    vi.useFakeTimers();
    const sim = new SimulatedLinkBridge();
    setLinkBridge(sim);
    const reads = vi.spyOn(sim, "readTransfers");
    const store = useTransfersStore();
    const started = await store.start("forge", "C:pack.zip", PASSWORD);
    expect(started.kind).toBe("started");
    const stop = await store.watch();
    expect(store.of("forge")[0]?.sent).toBe(0);
    // Des octets deviennent durables : le prochain passage les relit.
    const id = store.of("forge")[0]?.id ?? "";
    sim.transfers.advance(id, 123);
    await vi.advanceTimersByTimeAsync(POLL_MS + 10);
    expect(store.of("forge")[0]?.sent).toBe(123);
    // La fin : une relecture la voit.
    sim.transfers.complete(id);
    await vi.advanceTimersByTimeAsync(POLL_MS + 10);
    expect(store.of("forge")[0]?.status).toBe("done");
    expect(store.sending).toBe(false);
    // Plus rien n'envoie : une relecture seulement sur cinq passages.
    const before = reads.mock.calls.length;
    await vi.advanceTimersByTimeAsync(POLL_MS * 10 + 10);
    expect(reads.mock.calls.length - before).toBe(2);
    stop();
    // Plus aucun écran : plus aucun passage.
    const after = reads.mock.calls.length;
    await vi.advanceTimersByTimeAsync(POLL_MS * 10);
    expect(reads.mock.calls.length).toBe(after);
  });

  it("reprendre, annuler et retirer passent par le pont puis relisent la liste", async () => {
    const sim = new SimulatedLinkBridge();
    setLinkBridge(sim);
    const store = useTransfersStore();
    await store.start("forge", "C:\\pack.zip", PASSWORD);
    const id = store.of("forge")[0]?.id ?? "";
    sim.transfers.interrupt(id);
    await store.refresh();
    expect(store.of("forge")[0]?.status).toBe("interrupted");
    await store.resume(id);
    expect(store.of("forge")[0]?.status).toBe("sending");
    await store.cancel(id);
    expect(store.of("forge")[0]?.status).toBe("cancelled");
    await store.dismiss(id);
    expect(store.of("forge")).toEqual([]);
    expect(sim.transfers.calls).toEqual([
      expect.stringMatching(/^start /),
      `resume ${id}`,
      `cancel ${id}`,
      `dismiss ${id}`,
    ]);
  });
});

describe("ce qui demande un geste passe d'abord (revue de Nora, 16/11)", () => {
  it("garde les envois à reprendre hors de « +n autres » et dit « Réessayer » quand la place manque", () => {
    const done = (id: string) =>
      item({ id, status: "done", sent: 600 * MIB, name: `fait-${id}.zip` });
    const transfers = [
      done("A"),
      done("B"),
      done("C"),
      item({
        id: "D",
        name: "plein.zip",
        status: "interrupted",
        refusal: { kind: "no_space", missing: 5 * MIB, remaining: MIB },
      }),
    ];
    const wrapper = mount(TransfersStrip, { props: { transfers } });
    const rows = wrapper.findAll("[data-transfer]").map((row) => row.attributes("data-transfer"));
    expect(rows[0]).toBe("D");
    expect(rows).toHaveLength(3);
    expect(wrapper.get("[data-transfers-more]").text()).toContain("1");
    expect(wrapper.get("[data-transfer-resume]").text()).toBe("Réessayer");
    wrapper.unmount();
  });
});
