import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowUp,
  Clock3,
  History,
  MessageSquare,
  RotateCcw,
  Square,
  Trash2,
} from "lucide-react";
import { command } from "./service";
import { ResearchPanel } from "./ResearchPanel";
import { PreferenceFeedback } from "./PreferenceFeedback";
import type { Conversation, Message, ModelStatus } from "./types";

type Props = {
  model: ModelStatus | null;
  modelOffline: boolean;
  onOpenSettings: () => void;
};

export function ConversationView({
  model,
  modelOffline,
  onOpenSettings,
}: Props) {
  const [conversationId, setConversationId] = useState<string>(() =>
    crypto.randomUUID(),
  );
  const conversationRef = useRef(conversationId);
  const [known, setKnown] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [history, setHistory] = useState<Conversation[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const [showResearch, setShowResearch] = useState(false);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [temporary, setTemporary] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [sending, setSending] = useState(false);
  const end = useRef<HTMLDivElement>(null);
  const generating = messages.some(
    (message) => message.status === "generating",
  );

  const loadHistory = useCallback(async () => {
    try {
      setHistory(await command<Conversation[]>("list_conversations"));
    } catch (e) {
      setError(messageOf(e, "Could not load conversation history"));
    }
  }, []);
  const refresh = useCallback(async () => {
    if (!known) return;
    const requestedId = conversationId;
    try {
      const result = await command<{
        conversation: Conversation;
        messages: Message[];
      }>("get_conversation", { conversation_id: conversationId });
      if (conversationRef.current !== requestedId) return;
      setMessages(result.messages);
      setTemporary(result.conversation.temporary);
    } catch (e) {
      if (conversationRef.current !== requestedId) return;
      setError(messageOf(e, "Could not refresh this conversation"));
    }
  }, [conversationId, known]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!generating) return;
    const timer = setInterval(() => void refresh(), 500);
    return () => clearInterval(timer);
  }, [generating, refresh]);
  useEffect(() => {
    if (messages.length > 0) end.current?.scrollIntoView?.({ block: "end" });
  }, [messages]);

  async function send(text = draft) {
    const value = text.trim();
    if (!value || sending || model?.availability !== "ready") return;
    setSending(true);
    const requestedId = conversationId;
    setError("");
    const optimistic: Message = {
      id: crypto.randomUUID(),
      conversation_id: conversationId,
      role: "user",
      content: value,
      status: "complete",
      created_at: Date.now(),
    };
    setMessages((current) => [...current, optimistic]);
    setDraft("");
    try {
      const assistant = await command<Message>("send_message", {
        conversation_id: conversationId,
        text: value,
        temporary,
      });
      if (conversationRef.current !== requestedId) return;
      setKnown(true);
      setMessages((current) => [...current, assistant]);
      await loadHistory();
    } catch (e) {
      if (conversationRef.current !== requestedId) return;
      setMessages((current) =>
        current.filter((item) => item.id !== optimistic.id),
      );
      setDraft(value);
      setError(messageOf(e, "Could not send message"));
    } finally {
      setSending(false);
    }
  }
  async function cancel() {
    try {
      await command("cancel_message", { conversation_id: conversationId });
      await refresh();
    } catch (e) {
      setError(messageOf(e, "Could not stop generation"));
    }
  }
  function newConversation() {
    const id = crypto.randomUUID();
    conversationRef.current = id;
    setConversationId(id);
    setKnown(false);
    setMessages([]);
    setDraft("");
    setShowHistory(false);
    setShowResearch(false);
    setDeleteId(null);
    setError("");
  }
  async function remove(id: string) {
    try {
      await command("delete_conversation", { conversation_id: id });
      if (id === conversationId) newConversation();
      setDeleteId(null);
      await loadHistory();
    } catch (e) {
      setError(messageOf(e, "Could not delete conversation"));
    }
  }
  const lastUser = [...messages].reverse().find((item) => item.role === "user");

  if (showResearch)
    return (
      <ResearchPanel
        conversationId={conversationId}
        onBack={() => setShowResearch(false)}
      />
    );

  return (
    <>
      <div className="conversation-toolbar">
        {showHistory ? (
          <button className="text-button" onClick={() => setShowHistory(false)}>
            <ArrowLeft size={17} /> Back
          </button>
        ) : (
          <button
            className="text-button"
            onClick={() => {
              setShowHistory(true);
              void loadHistory();
            }}
          >
            <History size={17} /> History
          </button>
        )}
        <div className="toolbar-actions">
          <button
            className="text-button"
            disabled={!known || temporary}
            title={
              known && !temporary
                ? "Research notes and sources"
                : temporary
                  ? "Research is not saved for temporary chats"
                  : "Send a message first"
            }
            onClick={() => setShowResearch(true)}
          >
            Research
          </button>
          <button className="text-button" onClick={newConversation}>
            New chat
          </button>
        </div>
      </div>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {showHistory ? (
        <section
          className="conversation history-list"
          aria-label="Conversation history"
        >
          <h2>History</h2>
          {history.length === 0 && (
            <p className="empty-state">No saved conversations.</p>
          )}
          {history.map((item) => (
            <div className="history-row" key={item.id}>
              <button
                className="history-open"
                onClick={() => {
                  conversationRef.current = item.id;
                  setConversationId(item.id);
                  setKnown(true);
                  setTemporary(item.temporary);
                  setShowHistory(false);
                  setMessages([]);
                }}
              >
                <span>{item.title || "Untitled conversation"}</span>
                <small>
                  <Clock3 size={13} />{" "}
                  {new Date(item.updated_at).toLocaleDateString()}
                </small>
              </button>
              {deleteId === item.id ? (
                <button
                  className="danger-button"
                  onClick={() => void remove(item.id)}
                >
                  Confirm delete
                </button>
              ) : (
                <button
                  className="icon-button"
                  aria-label={`Delete ${item.title}`}
                  onClick={() => setDeleteId(item.id)}
                >
                  <Trash2 size={18} />
                </button>
              )}
            </div>
          ))}
        </section>
      ) : (
        <>
          <section className="conversation" aria-live="polite">
            {messages.length === 0 ? (
              <EmptyConversation
                model={model}
                offline={modelOffline}
                onOpenSettings={onOpenSettings}
              />
            ) : (
              messages.map((message) => (
                <article
                  className={`chat-message ${message.role}`}
                  key={message.id}
                >
                  <div>
                    {message.content ||
                      (message.status === "generating" ? "Thinking…" : "")}
                  </div>
                  {!temporary &&
                    message.role === "assistant" &&
                    message.status === "complete" && (
                      <PreferenceFeedback message={message} />
                    )}
                  {message.role === "assistant" &&
                    message.status !== "complete" && (
                      <small>{statusLabel(message.status)}</small>
                    )}
                </article>
              ))
            )}
            {lastUser &&
              messages.at(-1)?.role === "assistant" &&
              ["failed", "cancelled", "interrupted"].includes(
                messages.at(-1)!.status,
              ) && (
                <button
                  className="text-button retry-button"
                  onClick={() => void send(lastUser.content)}
                >
                  <RotateCcw size={16} /> Retry
                </button>
              )}
            <div ref={end} />
          </section>
          <form
            className="composer"
            onSubmit={(event) => {
              event.preventDefault();
              void send();
            }}
          >
            <label className="temporary-control">
              <input
                type="checkbox"
                checked={temporary}
                disabled={known}
                onChange={(event) => setTemporary(event.target.checked)}
              />
              Temporary
            </label>
            <div className="compose-row">
              <label className="sr-only" htmlFor="message">
                Message
              </label>
              <textarea
                id="message"
                rows={2}
                placeholder="Message…"
                value={draft}
                maxLength={8000}
                onChange={(event) => setDraft(event.target.value)}
              />
              {generating ? (
                <button
                  className="send-button"
                  type="button"
                  aria-label="Stop response"
                  onClick={() => void cancel()}
                >
                  <Square size={18} />
                </button>
              ) : (
                <button
                  className="send-button"
                  aria-label="Send message"
                  disabled={
                    !draft.trim() || sending || model?.availability !== "ready"
                  }
                >
                  <ArrowUp size={22} />
                </button>
              )}
            </div>
          </form>
        </>
      )}
    </>
  );
}

function EmptyConversation({
  model,
  offline,
  onOpenSettings,
}: {
  model: ModelStatus | null;
  offline: boolean;
  onOpenSettings: () => void;
}) {
  const availability = model?.availability;
  const installation = model?.installation;
  const copy = offline
    ? "The assistant bridge is offline. Check that it is running, then refresh."
    : installation?.status === "downloading"
      ? `Downloading the local model${model?.manifest ? ` (${formatProgress(installation.downloaded_bytes, model.manifest.size_bytes)})` : ""}.`
      : installation?.status === "verifying"
        ? "Verifying the downloaded local model."
        : installation?.status === "failed"
          ? `The model download failed${installation.error ? `: ${installation.error}` : ". Retry from Settings."}`
          : installation?.status === "cancelled"
            ? "The model download was cancelled. Set it up when you are ready."
            : availability === "missing_model"
              ? "An on-device model is available for lightweight routing. Configure a cloud model in Settings for conversational answers; the offline model is optional."
              : availability === "unavailable"
                ? "Local conversation is unavailable on this device."
                : availability === "busy"
                  ? "The local model is busy. Try again shortly."
                  : availability === "ready"
                    ? "Your conversations run locally on this device."
                    : "Checking the local conversation model…";
  return (
    <div className="empty-conversation">
      <MessageSquare size={34} />
      <h2>How can I help?</h2>
      <p>{copy}</p>
      {(installation?.status === "failed" ||
        installation?.status === "cancelled") && (
        <button className="primary" onClick={onOpenSettings}>
          {installation?.status === "failed"
            ? "Retry offline chat setup"
            : "Optional: download offline chat model"}
        </button>
      )}
    </div>
  );
}
function formatProgress(downloaded: number, total: number) {
  return `${formatBytes(downloaded)} of ${formatBytes(total)}`;
}
function formatBytes(bytes: number) {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function statusLabel(status: Message["status"]) {
  return {
    generating: "Generating",
    complete: "",
    cancelled: "Stopped",
    failed: "Response failed",
    interrupted: "Interrupted",
  }[status];
}
function messageOf(error: unknown, fallback: string) {
  return error instanceof Error ? error.message : fallback;
}
