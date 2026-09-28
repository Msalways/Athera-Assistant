import { Fragment, type ReactNode } from "react";

function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  const pattern =
    /(\*\*[^*]+\*\*|__[^_]+__|`[^`]+`|\*[^*\n]+\*|_[^_\n]+_|\[[^\]]+\]\([^)\s]+\))/g;
  let cursor = 0;
  let match: RegExpExecArray | null;
  let index = 0;

  while ((match = pattern.exec(text)) !== null) {
    if (match.index > cursor) nodes.push(text.slice(cursor, match.index));
    const token = match[0];
    const key = `${keyPrefix}-i${index++}`;
    if (token.startsWith("**") || token.startsWith("__")) {
      nodes.push(<strong key={key}>{token.slice(2, -2)}</strong>);
    } else if (token.startsWith("`")) {
      nodes.push(<code key={key}>{token.slice(1, -1)}</code>);
    } else if (token.startsWith("[")) {
      const label = token.slice(1, token.indexOf("]"));
      const href = token.slice(token.indexOf("(") + 1, -1);
      nodes.push(
        <a key={key} href={href} target="_blank" rel="noreferrer noopener">
          {label}
        </a>,
      );
    } else {
      nodes.push(<em key={key}>{token.slice(1, -1)}</em>);
    }
    cursor = match.index + token.length;
  }
  if (cursor < text.length) nodes.push(text.slice(cursor));
  return nodes;
}

function splitRow(line: string): string[] {
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  const cells: string[] = [];
  let current = "";
  for (let i = 0; i < trimmed.length; i++) {
    if (trimmed[i] === "\\" && trimmed[i + 1] === "|") {
      current += "|";
      i++;
    } else if (trimmed[i] === "|") {
      cells.push(current);
      current = "";
    } else {
      current += trimmed[i];
    }
  }
  cells.push(current);
  return cells.map((cell) => cell.trim());
}

const isDelimiterRow = (line: string) =>
  /^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$/.test(line) &&
  line.includes("-");

function blocks(markdown: string): ReactNode[] {
  const out: ReactNode[] = [];
  const lines = markdown.split("\n");
  let list: { ordered: boolean; items: string[] } | null = null;
  let paragraph: string[] = [];
  let quote: string[] = [];

  const flushParagraph = () => {
    if (!paragraph.length) return;
    const body = paragraph.join("\n");
    out.push(
      <p key={`p${out.length}`}>
        {body.split("\n").map((line, i) => (
          <Fragment key={i}>
            {i > 0 && <br />}
            {renderInline(line, `p${out.length}-${i}`)}
          </Fragment>
        ))}
      </p>,
    );
    paragraph = [];
  };
  const flushList = () => {
    if (!list) return;
    const List = list.ordered ? "ol" : "ul";
    out.push(
      <List key={`l${out.length}`}>
        {list.items.map((item, i) => (
          <li key={i}>{renderInline(item, `l${out.length}-${i}`)}</li>
        ))}
      </List>,
    );
    list = null;
  };
  const flushQuote = () => {
    if (!quote.length) return;
    out.push(
      <blockquote key={`q${out.length}`}>
        {quote.map((line, i) => (
          <Fragment key={i}>
            {i > 0 && <br />}
            {renderInline(line, `q${out.length}-${i}`)}
          </Fragment>
        ))}
      </blockquote>,
    );
    quote = [];
  };
  const flushAll = () => {
    flushParagraph();
    flushList();
    flushQuote();
  };

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];

    // Fenced code block. Contents are shown verbatim, never parsed as markdown,
    // and scroll horizontally so a long line cannot widen the page.
    const fence = /^\s*```+\s*([A-Za-z0-9_+-]*)\s*$/.exec(line);
    if (fence) {
      flushAll();
      const language = fence[1];
      const body: string[] = [];
      i++;
      while (i < lines.length && !/^\s*```+\s*$/.test(lines[i])) {
        body.push(lines[i]);
        i++;
      }
      out.push(
        <pre key={`c${out.length}`} className="code-block">
          {language && <span className="code-lang">{language}</span>}
          <code>{body.join("\n")}</code>
        </pre>,
      );
      continue;
    }

    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    const bullet = /^[-*+]\s+(.*)$/.exec(line);
    const ordered = /^\d+[.)]\s+(.*)$/.exec(line);
    const quoted = /^\s*>\s?(.*)$/.exec(line);
    const looksLikeRow = line.includes("|") && line.trim().startsWith("|");
    const nextIsDelimiter = looksLikeRow && isDelimiterRow(lines[i + 1] ?? "");

    if (looksLikeRow && nextIsDelimiter) {
      flushAll();
      const header = splitRow(line);
      const rows: string[][] = [];
      i += 2;
      while (
        i < lines.length &&
        lines[i].includes("|") &&
        lines[i].trim() !== ""
      ) {
        rows.push(splitRow(lines[i]));
        i++;
      }
      i--;
      out.push(
        <div key={`t${out.length}`} className="output-table-scroll">
          <table>
            <thead>
              <tr>
                {header.map((cell, ci) => (
                  <th key={ci} scope="col">
                    {renderInline(cell, `th${out.length}-${ci}`)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row, ri) => (
                <tr key={ri}>
                  {header.map((_, ci) => (
                    <td key={ci}>
                      {renderInline(
                        row[ci] ?? "",
                        `td${out.length}-${ri}-${ci}`,
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>,
      );
      continue;
    }

    if (heading) {
      flushAll();
      const level = Math.min(heading[1].length + 2, 6);
      const Tag = `h${level}` as "h3" | "h4" | "h5" | "h6";
      out.push(
        <Tag key={`h${out.length}`}>
          {renderInline(heading[2], `h${out.length}`)}
        </Tag>,
      );
    } else if (bullet || ordered) {
      flushParagraph();
      flushQuote();
      const isOrdered = Boolean(ordered);
      const content = (bullet ? bullet[1] : ordered![1]).trim();
      if (!list || list.ordered !== isOrdered) {
        flushList();
        list = { ordered: isOrdered, items: [] };
      }
      list.items.push(content);
    } else if (quoted) {
      flushParagraph();
      flushList();
      quote.push(quoted[1]);
    } else if (line.trim() === "") {
      flushAll();
    } else {
      flushList();
      flushQuote();
      paragraph.push(line);
    }
  }
  flushAll();
  return out;
}

export function Markdown({ text }: { text: string }) {
  return <div className="output-markdown">{blocks(text)}</div>;
}
