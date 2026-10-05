import { describe, expect, it } from "vitest";
import { GIB } from "@/test/machine";
import {
  formatCovered,
  formatFrequency,
  formatGb,
  formatPercent,
  formatRate,
  formatTemperature,
  formatUptime,
  formatUsage,
} from "./format";

describe("formats (BR-DASH-014)", () => {
  it("rounds percentages to an integer", () => {
    expect(formatPercent(87.4)).toBe("87 %");
    // Tronqué : le seuil d'attention (85) n'est atteint que si « 85 % » s'affiche.
    expect(formatPercent(84.6)).toBe("84 %");
    expect(formatPercent(94.6)).toBe("94 %");
    expect(formatPercent(85)).toBe("85 %");
    expect(formatPercent(0)).toBe("0 %");
    expect(formatPercent(100)).toBe("100 %");
  });

  it("writes memory and disks in Go with one decimal", () => {
    expect(formatGb(12.5 * GIB)).toBe("12.5 Go");
    expect(formatGb(0)).toBe("0.0 Go");
    expect(formatUsage(8 * GIB, 64 * GIB)).toBe("8.0 Go / 64.0 Go");
  });

  it("adapts the unit of a rate: Mo/s, Ko/s, and a zero stays a zero", () => {
    expect(formatRate(1.2 * 1024 * 1024)).toBe("1.2 Mo/s");
    expect(formatRate(450 * 1024)).toBe("450 Ko/s");
    expect(formatRate(300)).toBe("300 o/s");
    expect(formatRate(0)).toBe("0 o/s");
    expect(formatRate(2.5 * GIB)).toBe("2.5 Go/s");
    // La bascule se fait après l'arrondi : jamais « 1024 Ko/s ».
    expect(formatRate(1_048_500)).toBe("1.0 Mo/s");
    expect(formatRate(1023.4 * 1024)).toBe("1023 Ko/s");
  });

  it("writes the uptime long: days, hours, minutes", () => {
    expect(formatUptime(3 * 86_400 + 4 * 3600 + 12 * 60 + 30)).toBe("3 j 4 h 12 min");
    expect(formatUptime(2 * 3600 + 30 * 60)).toBe("2 h 30 min");
    expect(formatUptime(12 * 60)).toBe("12 min");
    expect(formatUptime(5)).toBe("0 min");
  });

  it("says « Non disponible » for a missing measure, never a zero", () => {
    for (const text of [
      formatPercent(null),
      formatGb(null),
      formatUsage(null, 5),
      formatRate(null),
      formatTemperature(null),
      formatFrequency(null),
      formatUptime(null),
    ]) {
      expect(text).toBe("Non disponible");
    }
  });

  it("writes temperatures, frequencies and covered time", () => {
    expect(formatTemperature(52.4)).toBe("52 °C");
    expect(formatTemperature(79.9)).toBe("79 °C");
    expect(formatFrequency(4500)).toBe("4.5 GHz");
    expect(formatFrequency(800)).toBe("800 MHz");
    expect(formatCovered(12 * 60_000 + 20_000)).toBe("12 min");
    expect(formatCovered(30_000)).toBe("30 s");
    expect(formatCovered(59.6 * 60_000)).toBe("59 min");
    expect(formatCovered(90 * 60_000)).toBe("1 h 30 min");
  });
});
