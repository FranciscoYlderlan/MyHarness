import { render, screen } from "@testing-library/react";
import { App } from "./app";

describe("App", () => {
  it("renders the empty main window titled MyHarness", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "MyHarness" })).toBeInTheDocument();
  });
});
