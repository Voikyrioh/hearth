import { describe, expect, it } from "vitest";
import { formatClock, formatSeen, initials } from "./format";

describe("formatSeen", () => {
  const now = 1_000_000_000_000;
  it("uses seconds, minutes, hours then days", () => {
    expect(formatSeen(now - 12_000, now)).toBe("Vu il y a 12 s");
    expect(formatSeen(now - 120_000, now)).toBe("Vu il y a 2 min");
    expect(formatSeen(now - 5 * 3600_000, now)).toBe("Vu il y a 5 h");
    expect(formatSeen(now - 3 * 86_400_000, now)).toBe("Vu il y a 3 j");
  });
  it("never goes negative when the clock moves back, and handles no contact", () => {
    expect(formatSeen(now + 5000, now)).toBe("Vu il y a 0 s");
    expect(formatSeen(null, now)).toBe("Jamais vu");
  });
});

describe("formatClock", () => {
  it("formats the local time as 14h05", () => {
    expect(formatClock(new Date(2026, 9, 5, 14, 5).getTime())).toBe("14h05");
  });
});

describe("initials", () => {
  it("takes the initials of two words, or the first two letters", () => {
    expect(initials("nas-salon")).toBe("NS");
    expect(initials("forge")).toBe("FO");
    expect(initials("Mon Serveur Maison")).toBe("MS");
    expect(initials("x")).toBe("X");
    expect(initials("  ")).toBe("?");
  });
});
