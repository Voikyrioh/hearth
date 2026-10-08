import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { SimulatedMachine } from "@/link";
import HAreaChart from "./atoms/HAreaChart.vue";
import HBars from "./atoms/HBars.vue";
import HGaugeArc from "./atoms/HGaugeArc.vue";
import HMeter from "./atoms/HMeter.vue";
import CoreBars from "./molecules/CoreBars.vue";
import Gauge from "./molecules/Gauge.vue";
import LevelBadge from "./molecules/LevelBadge.vue";
import TimeSeriesChart from "./molecules/TimeSeriesChart.vue";

describe("HGaugeArc", () => {
  it("draws the value as a fraction of the 270° arc, clamped", () => {
    const value = (ratio: number | null) =>
      mount(HGaugeArc, { props: { ratio } }).find(".arc__value");
    expect(value(0.5).attributes("stroke-dasharray")).toBe("50.00 100");
    expect(value(2).attributes("stroke-dasharray")).toBe("100.00 100");
    expect(value(-1).attributes("stroke-dasharray")).toBe("0.00 100");
    // Mesure illisible : la piste seule.
    expect(value(null).exists()).toBe(false);
  });

  it("changes color with the level (class), not only the stroke", () => {
    for (const level of ["normal", "attention", "critical"] as const) {
      const wrapper = mount(HGaugeArc, { props: { ratio: 0.9, level } });
      expect(wrapper.find(`.arc__value--${level}`).exists()).toBe(true);
    }
  });
});

describe("HAreaChart", () => {
  const points = (values: (number | null)[]) => values.map((v, i) => ({ t: i, v }));

  it("interrupts the line where a measure is missing: a hole, never a zero", () => {
    const wrapper = mount(HAreaChart, {
      props: {
        series: [{ points: points([10, 20, null, null, 30, 40]), tone: "ac" }],
        max: 100,
        label: "x",
      },
    });
    const line = wrapper.get(".chart__line--ac").attributes("d") ?? "";
    expect(line.match(/M/g)).toHaveLength(2);
    expect(wrapper.findAll("circle")).toHaveLength(1);
  });

  it("draws nothing for a series without any reading", () => {
    const wrapper = mount(HAreaChart, {
      props: { series: [{ points: points([null, null]), tone: "cool" }], max: null, label: "x" },
    });
    expect(wrapper.find(".chart__line").exists()).toBe(false);
    expect(wrapper.find("circle").exists()).toBe(false);
  });

  it("scales to the data when no maximum is given, and is a labelled image", () => {
    const wrapper = mount(HAreaChart, {
      props: {
        series: [
          { points: points([0, 500]), tone: "cool" },
          { points: points([0, 250]), tone: "ac" },
        ],
        max: null,
        label: "Débit réseau",
      },
    });
    expect(wrapper.find("svg").attributes("role")).toBe("img");
    expect(wrapper.find("svg").attributes("aria-label")).toBe("Débit réseau");
    expect(wrapper.findAll(".chart__line")).toHaveLength(2);
  });
});

describe("bars and meter", () => {
  it("draws one bar per core with its name as a tooltip", () => {
    const wrapper = mount(CoreBars, { props: { cores: [10, 50.4], label: "Charge par cœur" } });
    const titles = wrapper.findAll("title").map((title) => title.text());
    expect(titles).toEqual(["Cœur 1 : 10 %", "Cœur 2 : 50 %"]);
    expect(
      mount(HBars, { props: { values: [], titles: [], label: "x" } }).findAll("rect"),
    ).toHaveLength(0);
  });

  it("fills the meter with its ratio and colors it by level", () => {
    const wrapper = mount(HMeter, { props: { ratio: 0.42, level: "critical", label: "disque" } });
    expect(wrapper.get(".meter__value").attributes("width")).toBe("42");
    expect(wrapper.find(".meter__value--critical").exists()).toBe(true);
    expect(
      mount(HMeter, { props: { ratio: null, label: "d" } })
        .find(".meter__value")
        .exists(),
    ).toBe(false);
  });
});

describe("molecules", () => {
  it("shows nothing for a normal level, an icon and a word for the others", () => {
    expect(mount(LevelBadge, { props: { level: "normal" } }).text()).toBe("");
    const attention = mount(LevelBadge, { props: { level: "attention" } });
    expect(attention.text()).toBe("Attention");
    expect(attention.find("svg").exists()).toBe(true);
    expect(mount(LevelBadge, { props: { level: "critical" } }).text()).toBe("Critique");
  });

  it("a gauge says « Non disponible » instead of a value when the measure is missing", () => {
    const missing = mount(Gauge, { props: { label: "Charge", ratio: null, valueText: "0 %" } });
    expect(missing.text()).toContain("Non disponible");
    expect(missing.text()).not.toContain("0 %");
    expect(missing.attributes("aria-label")).toBe("Charge : Non disponible");
    const ok = mount(Gauge, {
      props: { label: "Charge", ratio: 0.37, valueText: "37 %", level: "attention" },
    });
    expect(ok.text()).toContain("37 %");
    expect(ok.text()).toContain("Attention");
    expect(ok.attributes("data-level")).toBe("attention");
  });

  it("a chart says when its history does not cover the whole window", () => {
    const props = {
      series: [],
      max: 100,
      label: "x",
      window: "1h" as const,
      format: (value: number) => `${value} %`,
    };
    const partial = mount(TimeSeriesChart, { props: { ...props, coveredMs: 12 * 60_000 } });
    expect(partial.get(".series__covered").text()).toBe("Depuis 12 min");
    expect(
      mount(TimeSeriesChart, { props: { ...props, coveredMs: 3_600_000 } })
        .find(".series__covered")
        .exists(),
    ).toBe(false);
  });

  it("a chart always says its duration and its scale, and gives a text equivalent", () => {
    const points = [
      { t: 1_000, v: 10 },
      { t: 2_000, v: 40 },
      { t: 3_000, v: 20 },
    ];
    const wrapper = mount(TimeSeriesChart, {
      props: {
        series: [{ points, tone: "ac" as const }],
        max: 100,
        label: "Charge",
        window: "5m" as const,
        coveredMs: 3_600_000,
        format: (value: number) => `${value} %`,
      },
    });
    expect(wrapper.get(".series__span-full").text()).toBe("5 dernières minutes");
    expect(wrapper.get(".series__scale").text()).toBe("0 à 100 %");
    expect(wrapper.get("svg").attributes("aria-label")).toBe(
      "Charge. Dernière valeur 20 %, minimum 10 %, maximum 40 %, sur 5 dernières minutes",
    );
  });

  it("a chart of two series gives the summary of each, named", () => {
    const wrapper = mount(TimeSeriesChart, {
      props: {
        series: [
          {
            points: [
              { t: 1_000, v: 100 },
              { t: 2_000, v: 300 },
            ],
            tone: "cool" as const,
            name: "Descendant",
          },
          {
            points: [
              { t: 1_000, v: 10 },
              { t: 2_000, v: 30 },
            ],
            tone: "ac" as const,
            name: "Montant",
          },
        ],
        max: null,
        label: "Débit réseau",
        window: "5m" as const,
        coveredMs: 3_600_000,
        format: (value: number) => `${value} o/s`,
      },
    });
    const label = wrapper.get("svg").attributes("aria-label") ?? "";
    expect(label).toContain("Montant : dernière valeur 30 o/s, minimum 10 o/s, maximum 30 o/s");
    expect(label).toContain(
      "Descendant : dernière valeur 300 o/s, minimum 100 o/s, maximum 300 o/s",
    );
  });

  it("a chart is one tab stop: arrows move the marker and say value and time, Escape clears it", async () => {
    const points = [
      { t: Date.UTC(2026, 9, 8, 10, 0, 0), v: 10 },
      { t: Date.UTC(2026, 9, 8, 10, 0, 1), v: 40 },
      { t: Date.UTC(2026, 9, 8, 10, 0, 2), v: 20 },
    ];
    const wrapper = mount(HAreaChart, {
      props: {
        series: [{ points, tone: "ac" as const }],
        max: 100,
        label: "Charge",
        format: (v: number) => `${v} %`,
      },
    });
    const stop = wrapper.get(".chart-box");
    expect(stop.attributes("tabindex")).toBe("0");
    expect(wrapper.findAll("[tabindex]")).toHaveLength(1);
    await stop.trigger("focus");
    await stop.trigger("keydown", { key: "ArrowLeft" });
    expect(wrapper.find(".chart__tip").exists()).toBe(true);
    const first = stop.attributes("aria-valuetext");
    expect(first).toMatch(/^40 %, \d{2}:\d{2}:\d{2}$/);
    await stop.trigger("keydown", { key: "ArrowLeft" });
    expect(stop.attributes("aria-valuetext")).toMatch(/^10 %, /);
    await stop.trigger("keydown", { key: "ArrowRight" });
    expect(stop.attributes("aria-valuetext")).toBe(first);
    await stop.trigger("keydown", { key: "Escape" });
    expect(wrapper.find(".chart__tip").exists()).toBe(false);
  });

  it("a focused chart does not speak every second: no value text until the user moves the marker", async () => {
    const make = (last: number) => [
      { t: 1_000, v: 10 },
      { t: 2_000, v: last },
    ];
    const wrapper = mount(HAreaChart, {
      props: {
        series: [{ points: make(20), tone: "ac" as const }],
        max: 100,
        label: "Charge",
        format: (v: number) => `${v} %`,
      },
    });
    const stop = wrapper.get(".chart-box");
    // Sans repère : une consigne fixe (jamais un nombre brut, jamais une valeur qui change chaque seconde).
    expect(stop.attributes("aria-valuetext")).toBe("Flèches pour parcourir les valeurs");
    expect(stop.attributes("aria-valuenow")).toBeUndefined();
    await wrapper.setProps({ series: [{ points: make(55), tone: "ac" as const }] });
    expect(stop.attributes("aria-valuetext")).toBe("Flèches pour parcourir les valeurs");
    await stop.trigger("keydown", { key: "ArrowLeft" });
    const posed = stop.attributes("aria-valuetext");
    expect(posed).toMatch(/^10 %, /);
    // Le repère posé garde son texte même quand la fenêtre glisse dessous.
    await wrapper.setProps({ series: [{ points: make(80), tone: "ac" as const }] });
    expect(stop.attributes("aria-valuetext")).toBe(posed);
  });

  it("a gauge at the normal level is not drawn with the alert gradient", () => {
    const wrapper = mount(HGaugeArc, { props: { ratio: 0.22, level: "normal" } });
    expect(wrapper.find("linearGradient").exists()).toBe(false);
    expect(wrapper.get(".arc__value").attributes("stroke")).toBeUndefined();
  });
});

describe("SimulatedMachine", () => {
  it("makes plausible measures and announces levels it was given, never computes them", () => {
    let now = 1_000_000;
    const machine = new SimulatedMachine({ now: () => now });
    const seen: number[] = [];
    machine.subscribe("m", (event) => {
      if (event.kind === "metrics") seen.push(event.metrics.sample.cpu);
    });
    for (let i = 0; i < 50; i += 1) {
      now += 1000;
      const metrics = machine.tick("m");
      expect(metrics?.sample.cpu).toBeGreaterThanOrEqual(0);
      expect(metrics?.sample.cpu).toBeLessThanOrEqual(60);
      expect(metrics?.levels.cpu).toBe("normal");
      expect(metrics?.sample.mem.usedBytes).toBeLessThan(metrics?.sample.mem.totalBytes ?? 0);
    }
    expect(seen).toHaveLength(50);
    machine.pin("m", "cpu", "critical");
    now += 1000;
    expect(machine.tick("m")?.levels.cpu).toBe("critical");
    machine.pin("m", "cpu", null);
    now += 1000;
    expect(machine.tick("m")?.levels.cpu).toBe("normal");
  });

  it("keeps five minutes of history at most and sends nothing while the server is not connected", () => {
    let now = 1_000_000;
    let up = true;
    const machine = new SimulatedMachine({ now: () => now, connected: () => up });
    machine.prefill("m", 400);
    let views = 0;
    machine.subscribe("m", (event) => {
      if (event.kind === "view") {
        views += 1;
        expect(event.view.history.length).toBeLessThanOrEqual(300);
      }
    });
    expect(views).toBe(1);
    up = false;
    now += 1000;
    expect(machine.tick("m")).toBeNull();
  });
});
