import { formatValue } from "@statasmon/protocol";
import { splitFigure } from "@statasmon/ui";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { StorageCard } from "./StorageCard";
import { BYTES, PERCENT } from "./shared";
import { caps, fresh, IPAD, IPHONE, sample, seed } from "./testkit";

const GB = 1e9;

beforeEach(() => seed());
afterEach(() => cleanup());

function legend(container: HTMLElement): Array<{ label: string; value: string }> {
  return [...container.querySelectorAll(".sm-legend > span")].map((el) => ({
    label: (el.textContent ?? "").replace(el.querySelector("em")?.textContent ?? "", "").trim(),
    value: el.querySelector("em")?.textContent ?? "",
  }));
}

function segments(container: HTMLElement): string[] {
  return [...container.querySelectorAll(".sm-stack > i")].map(
    (el) => el.getAttribute("data-segment") ?? "",
  );
}

test("total in the subtitle, used ring, free figure, category stack and legend", () => {
  caps(IPHONE, { storage_totals: true, storage_categories: true });
  sample(IPHONE, {
    "disk.total": 256 * GB,
    "disk.used": 197 * GB,
    "disk.free": 59 * GB,
    "disk.apps": 48 * GB,
    "disk.media": 112 * GB,
    "disk.system": 12 * GB,
    "disk.other": 25 * GB,
  });
  const { container } = render(<StorageCard deviceId={IPHONE} />);

  expect(screen.getByText(formatValue(BYTES, 256 * GB))).toBeInTheDocument();
  expect(
    screen.getByRole("img", { name: `Storage ${formatValue(PERCENT, 77)}` }),
  ).toBeInTheDocument();
  expect(screen.getByTestId("figure-number")).toHaveTextContent(
    splitFigure(formatValue(BYTES, 59 * GB))?.number ?? "",
  );
  expect(container.querySelector(".sm-figure-unit")).toHaveTextContent("GB free");
  expect(segments(container)).toEqual(["Apps", "Media", "System", "Other"]);
  expect(legend(container)).toEqual([
    { label: "Apps", value: formatValue(BYTES, 48 * GB) },
    { label: "Media", value: formatValue(BYTES, 112 * GB) },
    { label: "System", value: formatValue(BYTES, 12 * GB) },
    { label: "Other", value: formatValue(BYTES, 25 * GB) },
  ]);
  expect(screen.queryByRole("img", { name: /^Unavailable/ })).toBeNull();
});

test("without categories: one used segment and a dash with the storage_categories reason", () => {
  const reason = "No category breakdown over this transport";
  fresh(IPHONE, "ios");
  caps(IPHONE, { storage_totals: true, storage_categories: reason });
  sample(IPHONE, { "disk.total": 64 * GB, "disk.used": 49.25 * GB, "disk.free": 14.75 * GB });
  const { container } = render(<StorageCard deviceId={IPHONE} />);

  expect(segments(container)).toEqual(["Used"]);
  expect(legend(container)).toEqual([]);
  expect(screen.getByRole("img", { name: `Unavailable: ${reason}` })).toBeInTheDocument();
  expect(screen.getByTestId("figure-number")).toHaveTextContent(
    splitFigure(formatValue(BYTES, 14.75 * GB))?.number ?? "",
  );
});

test("dashes with the capability reason when storage totals are unavailable", () => {
  const reason = "Storage is not readable on this device";
  fresh(IPAD, "ios");
  caps(IPAD, { storage_totals: reason, storage_categories: reason });
  const { container } = render(<StorageCard deviceId={IPAD} />);

  expect(screen.getByTestId("gauge-storage")).toHaveAttribute("title", reason);
  expect(screen.getAllByRole("img", { name: `Unavailable: ${reason}` }).length).toBeGreaterThan(1);
  expect(screen.queryByTestId("figure-number")).toBeNull();
  expect(segments(container)).toEqual([]);
});
