/**
 * Vitest global setup — issue #23.
 * Configures @testing-library/react's cleanup after each test.
 */
import "@testing-library/react/pure";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});
