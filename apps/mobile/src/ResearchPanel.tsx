import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, BookOpen, Plus, Trash2 } from "lucide-react";
import { command } from "./service";
import type { ResearchSession } from "./types";

export function ResearchPanel({
  conversationId,
  onBack,
}: {
  conversationId: string;
  onBack: () => void;
}) {
  const [sessions, setSessions] = useState<ResearchSession[]>([]);
  const [active, setActive] = useState<ResearchSession | null>(null);
  const [title, setTitle] = useState("");
  const [note, setNote] = useState("");
  const [sourceTitle, setSourceTitle] = useState("");
  const [sourceUrl, setSourceUrl] = useState("");
  const [excerpt, setExcerpt] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [error, setError] = useState("");
  const load = useCallback(async () => {
    try {
      setSessions(
        await command<ResearchSession[]>("list_research", {
          conversation_id: conversationId,
        }),
      );
    } catch (e) {
      setError(textOf(e));
    }
  }, [conversationId]);
  useEffect(() => {
    void load();
  }, [load]);
  async function open(id: string) {
    try {
      setActive(await command<ResearchSession>("get_research", { id }));
    } catch (e) {
      setError(textOf(e));
    }
  }
  async function create() {
    if (!title.trim()) return;
    try {
      const session = await command<ResearchSession>("create_research", {
        conversation_id: conversationId,
        title: title.trim(),
      });
      setTitle("");
      setActive(session);
      await load();
    } catch (e) {
      setError(textOf(e));
    }
  }
  async function appendNote() {
    if (!active || !note.trim()) return;
    try {
      await command("append_research_note", {
        id: active.id,
        text: note.trim(),
      });
      setNote("");
      await open(active.id);
    } catch (e) {
      setError(textOf(e));
    }
  }
  async function addSource() {
    if (!active || !sourceTitle.trim() || !excerpt.trim()) return;
    try {
      await command("add_research_source", {
        id: active.id,
        title: sourceTitle.trim(),
        url: sourceUrl.trim() || null,
        excerpt: excerpt.trim(),
      });
      setSourceTitle("");
      setSourceUrl("");
      setExcerpt("");
      await open(active.id);
    } catch (e) {
      setError(textOf(e));
    }
  }
  async function remove() {
    if (!active) return;
    try {
      await command("delete_research", { id: active.id });
      setActive(null);
      setConfirmDelete(false);
      await load();
    } catch (e) {
      setError(textOf(e));
    }
  }
  return (
    <section className="page research-panel">
      <button
        className="text-button"
        onClick={active ? () => setActive(null) : onBack}
      >
        <ArrowLeft size={17} /> {active ? "Research list" : "Conversation"}
      </button>
      <h2>{active?.title ?? "Research"}</h2>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {!active ? (
        <>
          <form
            className="compact-form"
            onSubmit={(e) => {
              e.preventDefault();
              void create();
            }}
          >
            <label>
              Research title
              <input
                value={title}
                maxLength={200}
                onChange={(e) => setTitle(e.target.value)}
              />
            </label>
            <button className="primary" disabled={!title.trim()}>
              <Plus size={17} /> Create research
            </button>
          </form>
          {sessions.length === 0 && (
            <p className="empty-state">No research for this conversation.</p>
          )}
          {sessions.map((session) => (
            <button
              className="activity-row"
              key={session.id}
              onClick={() => void open(session.id)}
            >
              <span>{session.title}</span>
              <small>
                {session.sources.length} sources · {session.notes.length} note
                revisions
              </small>
            </button>
          ))}
        </>
      ) : (
        <>
          <section className="research-section">
            <h3>Notes</h3>
            {active.notes.length > 0 && (
              <p className="research-note">{active.notes.at(-1)?.text}</p>
            )}
            <form
              className="compact-form"
              onSubmit={(e) => {
                e.preventDefault();
                void appendNote();
              }}
            >
              <label>
                Append note
                <textarea
                  rows={4}
                  value={note}
                  onChange={(e) => setNote(e.target.value)}
                />
              </label>
              <button className="secondary" disabled={!note.trim()}>
                Save revision
              </button>
            </form>
          </section>
          <section className="research-section">
            <h3>Sources</h3>
            {active.sources.map((source) => (
              <article className="source-card" key={source.id}>
                <h4>{source.title}</h4>
                {source.url && (
                  <a href={source.url} target="_blank" rel="noreferrer">
                    Open source
                  </a>
                )}
                <p>{source.excerpt}</p>
              </article>
            ))}
            <form
              className="compact-form"
              onSubmit={(e) => {
                e.preventDefault();
                void addSource();
              }}
            >
              <label>
                Source title
                <input
                  value={sourceTitle}
                  onChange={(e) => setSourceTitle(e.target.value)}
                />
              </label>
              <label>
                URL (optional)
                <input
                  type="url"
                  value={sourceUrl}
                  onChange={(e) => setSourceUrl(e.target.value)}
                />
              </label>
              <label>
                Excerpt
                <textarea
                  rows={3}
                  value={excerpt}
                  onChange={(e) => setExcerpt(e.target.value)}
                />
              </label>
              <button
                className="secondary"
                disabled={!sourceTitle.trim() || !excerpt.trim()}
              >
                <BookOpen size={17} /> Add source
              </button>
            </form>
          </section>
          <button
            className="danger-button"
            onClick={() =>
              confirmDelete ? void remove() : setConfirmDelete(true)
            }
          >
            <Trash2 size={17} />{" "}
            {confirmDelete ? "Confirm delete research" : "Delete research"}
          </button>
        </>
      )}
    </section>
  );
}
function textOf(error: unknown) {
  return error instanceof Error ? error.message : "Research action failed";
}
