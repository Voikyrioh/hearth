import { describe, expect, it } from "vitest";
import { SAMPLE_SERVERS } from "./simulated";
import { SimulatedDevices } from "./simulated-devices";
import type { ServerInfo } from "./types";

const FORGE = SAMPLE_SERVERS.find((server) => server.id === "forge") as ServerInfo;
const GOOD = "Correct-Horse-9";

function simulated() {
  const devices = new SimulatedDevices(() => Date.UTC(2026, 9, 7));
  devices.seed(FORGE, [{ name: "salon/0.1.0", current: true }, { name: "bureau/0.1.0" }]);
  const [, other] = devices.ids(FORGE);
  return { devices, other: other as string, own: devices.ids(FORGE)[0] as string };
}

describe("simulated trusted devices (same rules as the agent)", () => {
  it("removes another device with the right password only", () => {
    const { devices, other } = simulated();
    expect(devices.remove(FORGE, other, "Faux-1", GOOD)).toEqual({
      kind: "refused",
      refusal: { kind: "wrong_password" },
    });
    expect(devices.ids(FORGE)).toHaveLength(2);
    expect(devices.remove(FORGE, other, GOOD, GOOD)).toEqual({ kind: "done" });
    expect(devices.ids(FORGE)).toHaveLength(1);
  });

  it("never removes the device in use, even with the right password", () => {
    const { devices, own } = simulated();
    expect(devices.remove(FORGE, own, GOOD, GOOD)).toEqual({
      kind: "refused",
      refusal: { kind: "current_device" },
    });
  });

  it("refuses everything from a PC without an enrolled key, and for an agent without the feature", () => {
    const devices = new SimulatedDevices(() => 0);
    const [first] = devices.seed(FORGE, [{ name: "bureau/0.1.0" }]);
    expect(devices.remove(FORGE, first?.id ?? "", GOOD, GOOD)).toEqual({
      kind: "refused",
      refusal: { kind: "no_device_key" },
    });
    devices.seed(FORGE, [], { supported: false });
    expect(devices.list(FORGE)).toEqual({ kind: "unsupported" });
    expect(devices.remove(FORGE, "X", GOOD, GOOD)).toEqual({
      kind: "refused",
      refusal: { kind: "unsupported" },
    });
  });

  it("a device that is gone is not found", () => {
    const { devices } = simulated();
    expect(devices.remove(FORGE, "absent", GOOD, GOOD)).toEqual({
      kind: "refused",
      refusal: { kind: "not_found" },
    });
  });
});
