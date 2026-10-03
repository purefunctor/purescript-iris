import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, test, vi } from "vitest";

const generated = await import(/* @vite-ignore */ `/@fs/${process.env.IRIS_REACT_OUTPUT}/Main/index.js`);

globalThis.IS_REACT_ACT_ENVIRONMENT = true;

let root;

afterEach(async () => {
  if (root) await act(async () => root.unmount());
  root = undefined;
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

async function render(view) {
  if (!root) {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  }
  await act(async () => root.render(view));
}

test("renders and updates native Iris React elements in a real browser", async () => {
  const errors = vi.spyOn(console, "error");
  await render(generated.view(false));

  const panel = document.querySelector("[data-testid=panel]");
  expect(panel.getAttribute("title")).toBe("Iris React");
  expect(panel.textContent).toBe("Heading: first:0second:0 · readyqualified");
  expect(panel.querySelector("[data-qualified=true]").textContent).toBe("qualified");

  await act(async () => document.querySelector("[data-name=first]").click());
  expect(panel.textContent).toBe("Heading: first:1second:0 · readyqualified");

  await render(generated.view(true));
  expect(panel.getAttribute("data-reversed")).toBe("true");
  expect([...panel.querySelectorAll("button")].map((button) => button.textContent)).toEqual([
    "second:0",
    "first:1",
  ]);
  expect(errors).not.toHaveBeenCalled();
});

test("retains React key validation for dynamic fragment children", async () => {
  const errors = vi.spyOn(console, "error").mockImplementation(() => {});
  await render(generated.list([generated.view(false), generated.view(true)]));
  expect(document.querySelectorAll("[data-testid=panel]")).toHaveLength(2);
  expect(errors.mock.calls.some((arguments_) => arguments_.join(" ").includes('unique "key"'))).toBe(true);
});
