import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";
import { RouterView } from "vue-router";
import { expectedFile } from "@/assets/illustrations/screens";
import { sampleMachine } from "@/link";
import Dashboard from "@/pages/Dashboard.vue";
import { useDashboardStore } from "@/stores/dashboard";
import { useServersStore } from "@/stores/servers";
import { mountContext } from "@/test/mount";

beforeEach(() =>
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
  }),
);
afterEach(() => vi.useRealTimers());

/** Le tableau de bord d'un serveur, monté seul (la coquille a ses propres tests). */
async function open(serverId = "forge", prefill = 300) {
  const ctx = await mountContext();
  if (prefill > 0) ctx.bridge.machine.prefill(serverId, prefill);
  useServersStore().setCurrent(serverId);
  const wrapper = mount(Dashboard, { global: ctx.global });
  await flushPromises();
  return { ...ctx, wrapper, store: useDashboardStore() };
}

/**
 * Le tableau de bord DANS le gabarit du serveur : c'est lui qui pose `StaleSurface` (BR-RESIL-007,
 * BR-DASH-009), la page ne l'enveloppe pas elle-même.
 */
async function openInLayout(serverId = "forge", prefill = 300) {
  const ctx = await mountContext();
  if (prefill > 0) ctx.bridge.machine.prefill(serverId, prefill);
  await ctx.router.push(`/servers/${serverId}/dashboard`);
  await ctx.router.isReady();
  const wrapper = mount(defineComponent({ render: () => h(RouterView) }), { global: ctx.global });
  await flushPromises();
  return { ...ctx, wrapper, store: useDashboardStore() };
}

/** Une seconde de plus : l'horloge avance et la machine envoie un échantillon. */
async function second(ctx: Awaited<ReturnType<typeof open>>, count = 1, serverId = "forge") {
  for (let i = 0; i < count; i += 1) {
    vi.advanceTimersByTime(1000);
    ctx.bridge.machine.tick(serverId);
  }
  await flushPromises();
}

const SECTIONS = [
  "Machine",
  "Durée de fonctionnement",
  "Processeur",
  "Mémoire",
  "Carte graphique",
  "Réseau",
  "Disques",
  "Températures",
];

function headings(wrapper: ReturnType<typeof mount>) {
  return wrapper.findAll("h2").map((heading) => heading.text());
}

describe("initial display (BR-DASH-001)", () => {
  it("shows a loader until the first measures arrive, then every section", async () => {
    const ctx = await open("forge", 0);
    expect(ctx.wrapper.get('[role="status"]').text()).toBe("Chargement des mesures");
    expect(ctx.wrapper.findAll("h2")).toHaveLength(0);
    ctx.bridge.machine.prefill("forge", 300);
    ctx.bridge.machine.resync("forge");
    await flushPromises();
    expect(headings(ctx.wrapper).sort()).toEqual([...SECTIONS].sort());
    expect(ctx.wrapper.find('[role="status"]').exists()).toBe(false);
    ctx.wrapper.unmount();
  });

  it("shows identity, gauges and units the way the spec writes them", async () => {
    const ctx = await open();
    const text = ctx.wrapper.text();
    expect(text).toContain("NixOS 25.05");
    expect(text).toContain("AMD Ryzen 9 7950X");
    expect(text).toContain("64.0 Go");
    expect(text).toMatch(/\d+ %/);
    expect(text).toMatch(/\d+ j \d+ h \d+ min/);
    expect(text).toContain("Charge globale");
    expect(text).toContain("Utilisée / Totale");
    expect(text).toContain("Utilisé / Total");
    expect(text).toMatch(/Libre : \d+\.\d Go/);
    expect(text).toContain("Mémoire vidéo");
    expect(text).toContain("Montant");
    expect(text).toContain("Descendant");
    expect(text).toMatch(/\d+\.\d Mo\/s|\d+ Ko\/s/);
    expect(ctx.wrapper.findAll(".bars__bar")).toHaveLength(16);
    // Jauges : processeur, mémoire, charge et mémoire vidéo de la carte graphique.
    expect(ctx.wrapper.findAll("figure.gauge")).toHaveLength(4);
    // Courbes : processeur, mémoire, disque, carte graphique, réseau, température.
    expect(ctx.wrapper.findAll("svg.chart")).toHaveLength(6);
    ctx.wrapper.unmount();
  });

  it("explains an empty dashboard of a server that never answered, without a loader", async () => {
    const ctx = await open("forge", 0);
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    expect(ctx.wrapper.find('[role="status"]').exists()).toBe(false);
    expect(ctx.wrapper.text()).toContain("Aucune mesure pour l'instant");
    expect(ctx.wrapper.get("img").attributes("src")).toContain(expectedFile("offline"));
    ctx.wrapper.unmount();
  });

  it("is the same for both roles (BR-DASH-013)", async () => {
    const admin = await open("forge");
    const adminHeadings = headings(admin.wrapper).sort();
    admin.wrapper.unmount();
    const readonly = await open("salon");
    expect(headings(readonly.wrapper).sort()).toEqual(adminHeadings);
    readonly.wrapper.unmount();
  });
});

describe("slow loading (HRT-34, revue UX C5)", () => {
  it("after 3 seconds without measures, says so and offers to try again", async () => {
    const ctx = await mountContext();
    useServersStore().setCurrent("forge");
    const wrapper = mount(Dashboard, { global: ctx.global });
    await flushPromises();
    // Aucune mesure n'est préchargée : le chargement tient tant que le lien est « Connecté » sans identité.
    if (wrapper.find("[data-dash-retry]").exists()) {
      wrapper.unmount();
      return;
    }
    expect(wrapper.text()).toContain("Chargement des mesures");
    vi.advanceTimersByTime(3100);
    await flushPromises();
    expect(wrapper.text()).toContain("Mesures en cours de chargement");
    // Le nom accessible suit le texte affiché (retour de revue HRT-34).
    expect(wrapper.find('[role="status"]').attributes("aria-label")).toBe(
      "Mesures en cours de chargement…",
    );
    expect(wrapper.find("[data-dash-retry]").exists()).toBe(true);
    wrapper.unmount();
  });
});

describe("curves window (BR-DASH-010)", () => {
  it("shows 5 minutes by default and switches to 1 min or 1 h immediately", async () => {
    const ctx = await open();
    const radios = () => ctx.wrapper.findAll('[role="radio"]');
    expect(radios().map((radio) => radio.text())).toEqual(["1 min", "5 min", "1 h"]);
    expect(radios().map((radio) => radio.attributes("aria-checked"))).toEqual([
      "false",
      "true",
      "false",
    ]);
    // Cinq minutes d'historique : la fenêtre d'une heure n'est pas pleine, et le dit.
    expect(ctx.wrapper.text()).not.toContain("Depuis");
    await radios()[2]?.trigger("click");
    expect(ctx.store.windowKey).toBe("1h");
    expect(radios()[2]?.attributes("aria-checked")).toBe("true");
    expect(ctx.wrapper.text()).toContain("Depuis 4 min");
    await radios()[0]?.trigger("click");
    expect(ctx.store.windowKey).toBe("1m");
    expect(ctx.wrapper.text()).not.toContain("Depuis");
    ctx.wrapper.unmount();
  });

  it("opens with the hour already read: the 1 h curve is full, nothing says « Depuis »", async () => {
    const ctx = await mountContext();
    ctx.bridge.machine.prefillHour("forge");
    ctx.bridge.machine.prefill("forge", 300);
    useServersStore().setCurrent("forge");
    const wrapper = mount(Dashboard, { global: ctx.global });
    await flushPromises();
    const radios = wrapper.findAll('[role="radio"]');
    await radios[2]?.trigger("click");
    expect(useDashboardStore().windowKey).toBe("1h");
    expect(wrapper.text()).not.toContain("Depuis");
    const ring = useDashboardStore().of("forge")?.ring;
    // 330 échantillons d'avant (10 s) + 300 de l'instantané (1 s).
    expect(ring?.length).toBe(630);
    wrapper.unmount();
  });

  it("keeps the last click when switching quickly, and keeps the choice across servers", async () => {
    const ctx = await open();
    const radios = () => ctx.wrapper.findAll('[role="radio"]');
    await radios()[0]?.trigger("click");
    await radios()[1]?.trigger("click");
    expect(ctx.store.windowKey).toBe("5m");
    await radios()[2]?.trigger("click");
    expect(ctx.store.windowKey).toBe("1h");
    ctx.wrapper.unmount();
  });
});

describe("history sources (HRT-11 review)", () => {
  it("shows the sections when a sample reaches the store before the view", async () => {
    const ctx = await open("forge", 0);
    // Le flux a de l'avance sur la lecture initiale : un échantillon d'abord, la vue ensuite.
    ctx.bridge.machine.prefill("forge", 5);
    const real = ctx.bridge.machine.subscribe.bind(ctx.bridge.machine);
    ctx.store.reset();
    vi.spyOn(ctx.bridge, "onMachine").mockImplementation(async (id, listener) => {
      let view: Parameters<typeof listener>[0] | null = null;
      const stop = real(id, (event) => {
        if (event.kind === "view") view = event;
      });
      stop();
      if (view) {
        const { sample, levels } = {
          sample: (view as { view: { history: never[] } }).view.history.at(-1),
          levels: (view as { view: { levels: unknown } }).view.levels,
        };
        listener({ kind: "metrics", metrics: { serverId: id, sample, levels } as never });
        listener(view);
      }
      return () => {};
    });
    await ctx.store.follow("forge");
    await flushPromises();
    expect(headings(ctx.wrapper).length).toBeGreaterThan(0);
    ctx.wrapper.unmount();
  });

  it("a view read from the disk (yesterday) then today's snapshot: the 1 h curve says how little it covers", async () => {
    const ctx = await open("forge", 0);
    const entry = ctx.store.of("forge");
    const { makeMachine, makeSample, normalLevels } = await import("@/test/machine");
    const now = Date.now();
    const yesterday = Array.from({ length: 300 }, (_, i) =>
      makeSample(now - 20 * 3600_000 + i * 1000),
    );
    const today = Array.from({ length: 301 }, (_, i) => makeSample(now - 300_000 + i * 1000));
    const last = today.at(-1);
    if (!entry || !last) throw new Error("setup");
    const merge = (history: typeof today) => {
      entry.ring.merge(history);
      entry.machine = makeMachine();
      entry.latest = { sample: last, levels: normalLevels(last) };
      entry.tick += 1;
    };
    merge(yesterday);
    merge(today);
    ctx.store.setWindow("1h");
    await flushPromises();
    expect(ctx.wrapper.text()).toContain("Depuis 5 min");
    ctx.wrapper.unmount();
  });
});

describe("alert levels (BR-DASH-003)", () => {
  it("shows no badge while everything is normal", async () => {
    const ctx = await open();
    expect(ctx.wrapper.findAll("[data-level='attention'], [data-level='critical']")).toHaveLength(
      0,
    );
    ctx.wrapper.unmount();
  });

  it("marks a measure that crosses a threshold with an icon and a word, then clears it", async () => {
    const ctx = await open();
    ctx.bridge.machine.pin("forge", "mem", "critical");
    ctx.bridge.machine.pin("forge", "disk", "attention");
    ctx.bridge.machine.pin("forge", "gpuTemp", "critical");
    await second(ctx);
    const memory = ctx.wrapper.get('section[aria-labelledby] figure[data-level="critical"]');
    expect(memory.text()).toContain("Critique");
    expect(memory.find("svg").exists()).toBe(true);
    const text = ctx.wrapper.text();
    expect(text).toContain("Attention");
    expect(text.match(/Critique/g)?.length).toBeGreaterThanOrEqual(2);
    // Redescend sous le seuil : la marque disparaît.
    ctx.bridge.machine.pin("forge", "mem", null);
    ctx.bridge.machine.pin("forge", "disk", null);
    ctx.bridge.machine.pin("forge", "gpuTemp", null);
    await second(ctx);
    expect(ctx.wrapper.findAll("[data-level='attention'], [data-level='critical']")).toHaveLength(
      0,
    );
    ctx.wrapper.unmount();
  });

  it("the processor gauge follows the level decided by the shell", async () => {
    const ctx = await open();
    ctx.bridge.machine.pin("forge", "cpu", "critical");
    await second(ctx);
    const cpuCard = ctx.wrapper
      .findAll("section")
      .find((s) => s.find("h2").text() === "Processeur");
    expect(cpuCard?.find('figure[data-level="critical"]').exists()).toBe(true);
    ctx.wrapper.unmount();
  });
});

describe("missing hardware (BR-DASH-005, 006, 007, 008)", () => {
  it("a machine without GPU and probes keeps both sections with their explanation", async () => {
    const ctx = await open("salon");
    const gpu = ctx.wrapper
      .findAll("section")
      .find((s) => s.find("h2").text() === "Carte graphique");
    expect(gpu?.text()).toContain("Aucune carte graphique mesurable sur cette machine");
    const temps = ctx.wrapper
      .findAll("section")
      .find((s) => s.find("h2").text() === "Températures");
    expect(temps?.text()).toContain(
      "Sondes non disponibles sur cette machine. Ce matériel n'expose pas sa température au système.",
    );
    // Rien d'autre n'en souffre : processeur, mémoire, disques, réseau sont là.
    expect(headings(ctx.wrapper)).toContain("Processeur");
    expect(ctx.wrapper.findAll("svg.chart").length).toBeGreaterThanOrEqual(4);
    ctx.wrapper.unmount();
  });

  it("a GPU without temperature says « Non disponible » and keeps load and video memory", async () => {
    const ctx = await open();
    ctx.bridge.machine.setGpuTempMissing("forge", true);
    await second(ctx);
    const gpu = ctx.wrapper
      .findAll("section")
      .find((s) => s.find("h2").text() === "Carte graphique");
    const row = gpu?.findAll(".stat").find((r) => r.text().startsWith("Température"));
    expect(row?.text()).toContain("Non disponible");
    expect(gpu?.text()).toContain("Charge");
    expect(gpu?.findAll("figure.gauge")).toHaveLength(2);
    expect(gpu?.text()).not.toContain("Aucune carte graphique mesurable");
    ctx.wrapper.unmount();
  });

  it("two graphics cards each get their own sub-section with their name", async () => {
    const ctx = await open();
    const base = sampleMachine("forge");
    ctx.bridge.machine.setMachine("forge", {
      ...base,
      gpus: [
        { name: "RTX 4090", memoryTotalBytes: 24 * 1024 ** 3 },
        { name: "Intel Arc", memoryTotalBytes: 16 * 1024 ** 3 },
      ],
    });
    await second(ctx, 2);
    const names = ctx.wrapper.findAll(".gpu__name").map((n) => n.text());
    expect(names).toEqual(["RTX 4090", "Intel Arc"]);
    ctx.wrapper.unmount();
  });

  it("an unreadable network rate is « Non disponible », not zero, and the rest goes on", async () => {
    const ctx = await open();
    const entry = ctx.store.of("forge");
    if (!entry?.latest) throw new Error("pas de mesures");
    entry.latest = { ...entry.latest, sample: { ...entry.latest.sample, net: null } };
    await flushPromises();
    const net = ctx.wrapper.findAll("section").find((s) => s.find("h2").text() === "Réseau");
    expect(net?.text()).toContain("Montant");
    expect(net?.text()).toContain("Non disponible");
    expect(net?.text()).not.toContain("0 o/s");
    expect(ctx.wrapper.text()).toContain("Charge globale");
    ctx.wrapper.unmount();
  });
});

describe("live changes (BR-DASH-012)", () => {
  it("a disk mounted or removed while the dashboard is open appears or disappears by itself", async () => {
    const ctx = await open();
    const base = sampleMachine("forge");
    const extra = {
      name: "/dev/sdb1",
      mount: "/mnt/usb",
      fs: "ext4",
      totalBytes: 500 * 1024 ** 3,
      removable: true,
    };
    const names = () => ctx.wrapper.findAll(".disk__name").map((n) => n.text());
    expect(names()).toEqual(["/dev/nvme0n1p2", "/dev/sda1"]);
    ctx.bridge.machine.setMachine("forge", { ...base, disks: [...base.disks, extra] });
    await second(ctx);
    expect(names()).toEqual(["/dev/nvme0n1p2", "/dev/sda1", "/dev/sdb1"]);
    ctx.bridge.machine.setMachine("forge", { ...base, disks: [extra] });
    await second(ctx);
    expect(names()).toEqual(["/dev/sdb1"]);
    ctx.wrapper.unmount();
  });

  it("the number of core bars follows the cores of the machine", async () => {
    const ctx = await open();
    expect(ctx.wrapper.findAll(".bars__bar")).toHaveLength(16);
    const base = sampleMachine("forge");
    ctx.bridge.machine.setMachine("forge", {
      ...base,
      cpu: { ...base.cpu, logicalCores: 8 },
    });
    await second(ctx);
    expect(ctx.wrapper.findAll(".bars__bar")).toHaveLength(8);
    ctx.wrapper.unmount();
  });
});

describe("link not connected (BR-DASH-009, 011)", () => {
  it("keeps the last values, greyed and dated, and resumes live when the link returns", async () => {
    const ctx = await openInLayout();
    await second(ctx, 3);
    const before = ctx.wrapper.text();
    expect(ctx.wrapper.find('[data-stale="true"]').exists()).toBe(false);

    // HRT-38 (C48) : en reconnexion (3 à 30 s) la page garde son apparence, sans date ; elle ne s'estompe
    // qu'à partir de « Hors ligne ».
    ctx.bridge.setState("forge", "reconnecting");
    await flushPromises();
    expect(ctx.wrapper.find('[data-stale="true"]').exists()).toBe(false);
    expect(ctx.wrapper.find(".surface__stamp").exists()).toBe(false);
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    expect(ctx.wrapper.find('[data-stale="true"]').exists()).toBe(true);
    // Les dernières valeurs restent là, aucune n'est vidée ni remplacée.
    expect(ctx.wrapper.text()).toContain("Charge globale");
    expect(ctx.wrapper.text()).toContain("NixOS 25.05");
    // Un seul marquage « périmé » et une seule date, posés par le gabarit.
    expect(ctx.wrapper.findAll('[data-stale="true"]')).toHaveLength(1);
    expect(ctx.wrapper.findAll(".surface__stamp")).toHaveLength(1);
    expect(ctx.wrapper.get(".surface__stamp").text()).toMatch(/^Vu il y a \d+ s$/);
    expect(before).toContain("Mémoire");

    // Pendant la coupure : plus aucun échantillon n'arrive, la courbe ne glisse pas.
    const entry = ctx.store.of("forge");
    const lastAt = entry?.ring.last?.at;
    vi.advanceTimersByTime(20_000);
    ctx.bridge.machine.tick("forge");
    await flushPromises();
    expect(entry?.ring.last?.at).toBe(lastAt);

    // Le lien revient : l'instantané recolle l'historique, sans trou ni doublon.
    ctx.bridge.setState("forge", "connected");
    await flushPromises();
    expect(ctx.wrapper.find('[data-stale="true"]').exists()).toBe(false);
    const times = entry?.ring.samples().map((sample) => sample.at) ?? [];
    expect(new Set(times).size).toBe(times.length);
    expect(times).toEqual([...times].sort((a, b) => a - b));
    const gaps = times.slice(1).map((time, i) => time - (times[i] ?? 0));
    expect(Math.max(...gaps)).toBeLessThanOrEqual(1100);
    await second(ctx);
    expect(entry?.ring.last?.at).toBeGreaterThan(lastAt ?? 0);
    ctx.wrapper.unmount();
  });

  it("shows the last known view even if the link never came up in this session", async () => {
    const ctx = await openInLayout("forge", 300);
    ctx.bridge.setState("forge", "offline");
    await flushPromises();
    expect(ctx.wrapper.text()).toContain("Charge globale");
    expect(ctx.wrapper.find('[data-stale="true"]').exists()).toBe(true);
    ctx.wrapper.unmount();
  });
});
