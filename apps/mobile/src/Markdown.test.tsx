import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { Markdown } from "./Markdown";

it("renders bold, italic and inline code instead of literal markers", () => {
  render(<Markdown text="You have **three** meetings and *one* deadline." />);
  expect(screen.getByText("three").tagName).toBe("STRONG");
  expect(screen.getByText("one").tagName).toBe("EM");
  expect(screen.queryByText(/\*\*/)).not.toBeInTheDocument();
});

it("renders inline code", () => {
  render(<Markdown text="Run `npm test` first." />);
  expect(screen.getByText("npm test").tagName).toBe("CODE");
});

it("renders unordered and ordered lists as real lists", () => {
  const { container } = render(
    <Markdown text={"- 14:00 Design Review\n- 16:30 1:1"} />,
  );
  expect(container.querySelectorAll("ul li")).toHaveLength(2);

  const ordered = render(<Markdown text={"1. first\n2. second"} />);
  expect(ordered.container.querySelectorAll("ol li")).toHaveLength(2);
});

it("renders headings and links", () => {
  render(<Markdown text={"## Today\n\nSee [the doc](https://example.com)."} />);
  expect(screen.getByRole("heading", { name: "Today" })).toBeInTheDocument();
  const link = screen.getByRole("link", { name: "the doc" });
  expect(link).toHaveAttribute("href", "https://example.com");
  expect(link).toHaveAttribute("rel", expect.stringContaining("noopener"));
});

it("never interprets raw HTML", () => {
  const { container } = render(
    <Markdown text="<img src=x onerror=alert(1)>" />,
  );
  expect(container.querySelector("img")).toBeNull();
});

it("keeps paragraph text intact", () => {
  render(<Markdown text={"First line\nSecond line"} />);
  expect(screen.getByText(/First line/)).toBeInTheDocument();
});

it("renders a table as a real table, not raw pipes", () => {
  const { container } = render(
    <Markdown
      text={
        "| Region | Revenue |\n|---|---|\n| North | 128,400 |\n| South | 96,200 |"
      }
    />,
  );
  expect(container.querySelectorAll("table thead th")).toHaveLength(2);
  expect(container.querySelectorAll("table tbody tr")).toHaveLength(2);
  expect(
    screen.getByRole("columnheader", { name: "Region" }),
  ).toBeInTheDocument();
  expect(screen.getByRole("cell", { name: "128,400" })).toBeInTheDocument();
  // The literal pipe characters must not survive anywhere.
  expect(container.textContent).not.toContain("|");
});

it("formats cells as inline markdown and tolerates ragged rows", () => {
  const { container } = render(
    <Markdown text={"| Name | Note |\n|---|---|\n| **bold** | ok |"} />,
  );
  expect(container.querySelector("tbody strong")?.textContent).toBe("bold");
  const ragged = render(<Markdown text={"| A | B |\n|---|---|\n| only |"} />);
  expect(ragged.container.querySelectorAll("tbody td")).toHaveLength(2);
});

it("renders a fenced code block verbatim with its language", () => {
  const { container } = render(
    <Markdown text={"```bash\nnpm run build\n```"} />,
  );
  const pre = container.querySelector("pre.code-block");
  expect(pre).not.toBeNull();
  expect(pre?.querySelector("code")?.textContent).toBe("npm run build");
  expect(container.querySelector(".code-lang")?.textContent).toBe("bash");
  // Fenced content is code, never markdown.
  expect(container.querySelector("pre strong")).toBeNull();
});

it("does not interpret markup inside a code fence", () => {
  const { container } = render(
    <Markdown text={"```\n# not a heading\n- not a list\n```"} />,
  );
  expect(container.querySelector("h1, h2, h3, h4, ul, ol")).toBeNull();
  expect(container.querySelector("code")?.textContent).toContain(
    "# not a heading",
  );
});

it("keeps an unterminated fence from swallowing the rest of the answer", () => {
  const { container } = render(<Markdown text={"```js\nconst a = 1;"} />);
  expect(container.querySelector("pre.code-block")).not.toBeNull();
  expect(container.querySelector("code")?.textContent).toBe("const a = 1;");
});

it("renders blockquotes as quotes without the leading marker", () => {
  const { container } = render(
    <Markdown text={"> The East number is the one to check."} />,
  );
  const quote = container.querySelector("blockquote");
  expect(quote).not.toBeNull();
  expect(quote?.textContent).toBe("The East number is the one to check.");
});

it("does not treat a single pipe as a table", () => {
  const { container } = render(
    <Markdown text={"Use a | b for the union of two sets."} />,
  );
  expect(container.querySelector("table")).toBeNull();
  expect(screen.getByText(/Use a \| b/)).toBeInTheDocument();
});

it("still separates a table from the paragraphs around it", () => {
  const { container } = render(
    <Markdown text={"Before.\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\nAfter."} />,
  );
  const root = container.querySelector(".output-markdown");
  const children = root ? [...root.children].map((c) => c.tagName) : [];
  expect(children).toEqual(["P", "DIV", "P"]);
});
