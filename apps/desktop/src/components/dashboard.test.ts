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
    expect(wrapper.attributes("role")).toBe("img");
    expect(wrapper.attributes("aria-label")).toBe("Débit réseau");
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
    const props = { series: [], max: 100, label: "x", window: "1h" as const };
    expect(mount(TimeSeriesChart, { props: { ...props, coveredMs: 12 * 60_000 } }).text()).toBe(
      "Depuis 12 min",
    );
    expect(mount(TimeSeriesChart, { props: { ...props, coveredMs: 3_600_000 } }).text()).toBe("");
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
