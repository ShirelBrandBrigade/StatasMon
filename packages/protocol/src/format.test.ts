import { expect, test } from "vitest";
import { formatValue, shortName } from "./format";

test("formats bytes, rates, and dashes", () => {
  expect(formatValue({ unit: "bytes" }, 5.4 * 1024 ** 3)).toBe("5.4 GB");
  expect(formatValue({ unit: "bits_per_sec" }, 24_600_000)).toBe("24.6 Mb/s");
  expect(formatValue({ unit: "percent" }, 38)).toBe("38%");
  expect(formatValue({ unit: "celsius" }, 41.23)).toBe("41.2 °C");
  expect(formatValue({ unit: "count" }, 1842)).toBe("1 842");
  expect(formatValue({ unit: "percent" }, null)).toBe("—");
});

test("shortName strips wrappers and reverse-DNS prefixes", () => {
  expect(shortName("[osservice<com.apple.AppSSODaemon>:268]")).toBe("AppSSODaemon");
  expect(shortName("xpcservice<com.apple.MTLCompilerService([osservice<x>:1])>:5")).toBe(
    "MTLCompilerService",
  );
  expect(shortName("com.example.fieldkit")).toBe("fieldkit");
  expect(shortName("SpringBoard")).toBe("SpringBoard");
  expect(shortName("FieldKit (debug)")).toBe("FieldKit");
});
