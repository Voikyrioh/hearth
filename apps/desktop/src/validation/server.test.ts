import { describe, expect, it } from "vitest";
import type { ServerInfo } from "@/link";
import { hostError, isValidHost, nameError, parsePort, portError, serverAt } from "./server";

function server(name: string, host: string, port = 7341): ServerInfo {
  return {
    id: name,
    name,
    address: host,
    host,
    port,
    color: 1,
    role: "admin",
    username: "",
    remember: false,
  };
}

describe("host validation (mirror of hearth-link domain/book.rs)", () => {
  it("accepts IPv4, IPv6 and names", () => {
    for (const good of [
      "192.168.1.20",
      "::1",
      "[fe80::1]",
      "2001:db8::7",
      "forge",
      "forge.maison",
      "nas-salon.local",
      "FORGE.Maison.",
      "a_b.lan",
    ]) {
      expect(isValidHost(good), good).toBe(true);
    }
  });

  it("refuses everything else", () => {
    for (const bad of [
      "",
      "for ge",
      "forge/",
      "http://forge",
      "forge:7341",
      "-forge",
      "forge-.lan",
      "a..b",
      "300.1.1.1",
      "1.2.3",
      ".",
      "a".repeat(64),
      `${"a".repeat(63)}.${"b".repeat(200)}`,
    ]) {
      expect(isValidHost(bad), bad).toBe(false);
    }
  });

  it("gives the spec messages", () => {
    expect(hostError("   ")).toBe("validation.hostRequired");
    expect(hostError("pas une adresse")).toBe("validation.hostInvalid");
    expect(hostError(" 192.168.1.20 ")).toBeNull();
  });
});

describe("port", () => {
  it("is optional, then an integer from 1 to 65535", () => {
    expect(parsePort("")).toBeNull();
    expect(parsePort(" 7443 ")).toBe(7443);
    expect(parsePort("65535")).toBe(65535);
    for (const bad of ["0", "65536", "-1", "12a", "1.5", "123456"]) {
      expect(parsePort(bad), bad).toBeUndefined();
      expect(portError(bad)).toBe("validation.portInvalid");
    }
    expect(portError("")).toBeNull();
  });
});

describe("name", () => {
  const others = [server("Forge", "a.lan")];
  it("is required, bounded and unique ignoring case", () => {
    expect(nameError("  ", others)).toBe("validation.nameRequired");
    expect(nameError("x".repeat(256), others)).toBe("validation.nameRequired");
    expect(nameError("x".repeat(255), others)).toBeNull();
    expect(nameError(" forge ", others)).toBe("validation.nameTaken");
    expect(nameError("Salon", others)).toBeNull();
  });
});

describe("serverAt", () => {
  it("finds a server by host (ignoring case) and port, default port included", () => {
    const list = [server("Forge", "forge.lan"), server("Salon", "nas.lan", 7443)];
    expect(serverAt(list, "FORGE.lan", null)?.name).toBe("Forge");
    expect(serverAt(list, "forge.lan", 7341)?.name).toBe("Forge");
    expect(serverAt(list, "nas.lan", null)).toBeUndefined();
    expect(serverAt(list, "nas.lan", 7443)?.name).toBe("Salon");
  });
});
