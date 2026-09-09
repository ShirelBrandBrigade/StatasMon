import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import { installCanvasStub } from "../charts/testing/jsdomCanvas";

// uPlot touches matchMedia and canvas at import time; jsdom has neither.
// Installed at module scope so it precedes every test file's imports.
installCanvasStub();

afterEach(() => cleanup());
