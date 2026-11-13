import { describe, expect, it } from "vitest";
import {
  depthWidth,
  formatUnits,
  I64_MAX,
  parseUnits,
  participantId,
} from "./market";

describe("exact order amounts", () => {
  it("converts display decimals to engine integers without floating point", () => {
    expect(parseUnits("65444.50", 2)).toBe("6544450");
    expect(parseUnits("0.00000001", 8)).toBe("1");
    expect(parseUnits("9007199254740993", 0)).toBe("9007199254740993");
    expect(parseUnits("92233720368.54775807", 8)).toBe(I64_MAX.toString());
    expect(parseUnits("00001.20", 2)).toBe("120");
  });
  it.each([
    "0",
    "-1",
    "1e5",
    "Infinity",
    "NaN",
    "",
    "0.001",
    "1.2.3",
    "9223372036854775808",
  ])("rejects invalid or out-of-range price %j", (value) =>
    expect(() => parseUnits(value, 2)).toThrow(),
  );
  it("formats large values, aggregates, and zero without losing precision", () => {
    expect(formatUnits("9007199254740993", 2)).toBe("90,071,992,547,409.93");
    expect(formatUnits("1", 8)).toBe("0.00000001");
    expect(formatUnits("0", 2)).toBe("0.00");
    expect(formatUnits("6544450", 2, false)).toBe("65444.50");
    expect(formatUnits((I64_MAX * 3n).toString(), 0)).toBe(
      "27,670,116,110,564,327,421",
    );
  });
  it("preserves participant IDs and validates the supported range", () => {
    expect(participantId("9007199254740993")).toBe("9007199254740993");
    for (const id of ["0", "-1", "1.5", "abc", "9223372036854775808"])
      expect(() => participantId(id)).toThrow();
  });
  it("bounds depth percentages with exact integer ratios", () => {
    expect(depthWidth("100", 200n)).toBe(50);
    expect(depthWidth("0", 0n)).toBe(0);
    expect(depthWidth(I64_MAX.toString(), I64_MAX * 2n)).toBe(50);
  });
});

describe("order tracking progress", () => {
  it("represents partial fills accurately without rounding to completion", async () => {
    const { fillPercent, isTerminal } = await import("./market");
    expect(fillPercent("0", "10")).toBe(0);
    expect(fillPercent("4", "10")).toBe(40);
    expect(fillPercent("10", "10")).toBe(100);
    expect(fillPercent("9007199254740992", "9007199254740993")).toBe(99.99);
    expect(
      isTerminal({ client_order_id: "test", status: "partially_filled" }),
    ).toBe(false);
    expect(isTerminal({ client_order_id: "test", status: "filled" })).toBe(
      true,
    );
    expect(isTerminal({ client_order_id: "test", status: "cancelled" })).toBe(
      true,
    );
  });
});
