import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import remarkEmoji from "remark-emoji";
import remarkGfm from "remark-gfm";
import { isTauri } from "../lib/appInfo";
import {
  applyFormat,
  byteLength,
  contextApi,
  ghostLayers,
  QUOTES,
  SCOPES,
  SPELLINGS,
  TONES,
  type ContextPreview,
  type ContextSettings,
  type ContextView,
  type Format,
} from "../lib/context";

const muted = { color: "var(--text-muted)" } as const;

function Card({ title, children, label }: { title: string; children: ReactNode; label: string }) {
  return (
    <section aria-label={label} className="soft-card panel-in mt-4 p-5">
      <h2 className="mb-2 text-lg font-medium">{title}</h2>
      {children}
    </section>
  );
}

function SmallButton(props: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      {...props}
      className={`rounded-full px-3 py-1 text-xs font-medium disabled:opacity-50 ${props.className ?? ""}`}
      style={{ background: "var(--surface-raised)", border: "1px solid var(--border-strong)", ...props.style }}
    />
  );
}

/** The Personalize tab: profile.md editor, what BOB may see, writing style, Ghost Text layers, preview. */
export function PersonalizePanel() {
  const [view, setView] = useState<ContextView | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setView(await contextApi.get());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 3000);
    return () => clearInterval(t);
  }, [refresh]);

  const update = async (patch: Partial<ContextSettings>) => {
    if (!view) return;
    const settings = { ...view.settings, ...patch };
    setView({ ...view, settings });
    try {
      setError(null);
      await contextApi.setSettings(settings);
    } catch (e) {
      setError(String(e));
    }
    void refresh();
  };

  if (!isTauri()) {
    return (
      <p className="soft-card p-5 text-sm" style={muted}>
        Open the BOB app to personalize it.
      </p>
    );
  }
  if (!view) return <p style={muted}>Loading...</p>;
  const s = view.settings;
  const captureScope = s.scope !== "accessibility";

  return (
    <div>
      {error && (
        <p role="alert" className="mb-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
      <ProfileEditor view={view} onSaved={refresh} />

      <Card title="What BOB can see" label="Visible context">
        <div role="radiogroup" aria-label="Visible context" className="space-y-2">
          {SCOPES.map((o) => (
            <label key={o.id} className="flex cursor-pointer gap-3 rounded-xl p-2" style={s.scope === o.id ? { background: "var(--surface-raised)" } : undefined}>
              <input type="radio" name="scope" checked={s.scope === o.id} onChange={() => void update({ scope: o.id })} />
              <span>
                <span className="block text-sm font-medium">{o.label}</span>
                <span className="block text-xs" style={muted}>
                  {o.detail}
                </span>
              </span>
            </label>
          ))}
        </div>
        {captureScope && (
          <p className="mt-3 text-xs" data-testid="screen-permission" style={muted}>
            {view.screen_recording ? (
              <>
                Screen Recording is on.{" "}
                {view.visible.screen_chars > 0
                  ? `Last read: ${view.visible.screen_chars} characters in ${view.visible.screen_ms} ms.`
                  : "Text is read when you type, at most every 8 seconds."}
              </>
            ) : (
              <>
                Screen Recording is off for BOB.{" "}
                <SmallButton onClick={() => void contextApi.requestScreenRecording().then(refresh)}>Allow Screen Recording</SmallButton>{" "}
                If no prompt appears, turn BOB on in System Settings &gt; Privacy &amp; Security &gt; Screen &amp; System Audio Recording.
              </>
            )}
          </p>
        )}
        <p className="mt-3 text-xs" style={muted}>
          Brains only receive this when you allow it per brain in Brains (off by default for cloud brains).
        </p>
      </Card>

      <Card title="Writing style" label="Writing style">
        <div className="flex flex-wrap gap-4 text-sm">
          <label className="flex items-center gap-2">
            Tone
            <select
              aria-label="Tone"
              value={s.tone}
              onChange={(e) => void update({ tone: e.currentTarget.value as ContextSettings["tone"] })}
              className="rounded-lg px-2 py-1"
              style={{ background: "var(--surface-raised)", border: "1px solid var(--border)" }}
            >
              {TONES.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>
          <LanguageField value={s.language} onCommit={(language) => void update({ language })} />
          <label className="flex items-center gap-2">
            Spelling
            <select
              aria-label="Spelling"
              value={s.spelling}
              onChange={(e) => void update({ spelling: e.currentTarget.value as ContextSettings["spelling"] })}
              className="rounded-lg px-2 py-1"
              style={{ background: "var(--surface-raised)", border: "1px solid var(--border)" }}
            >
              {SPELLINGS.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>
          <label className="flex items-center gap-2">
            Quotes
            <select
              aria-label="Quotes"
              value={s.quotes}
              onChange={(e) => void update({ quotes: e.currentTarget.value as ContextSettings["quotes"] })}
              className="rounded-lg px-2 py-1"
              style={{ background: "var(--surface-raised)", border: "1px solid var(--border)" }}
            >
              {QUOTES.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <p className="mt-2 text-xs" style={muted}>
          Tone, language and spelling guide Selection Actions and Dictation cleanup. The quote style applies to everything BOB types: Ghost Text, dictation and rewrites.
        </p>
      </Card>

      <Card title="Ghost Text context (experimental)" label="Ghost Text context">
        <p className="mb-2 text-xs" style={muted}>
          Off by default. In BOB's benchmark, remembered phrases raised the share of right next words from 32% to about 40% and cost about 4 ms per suggestion (docs/PERFORMANCE.md).
        </p>
        <div className="space-y-2 text-sm">
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={s.ghost.memory} onChange={(e) => void update({ ghost: ghostLayers(e.currentTarget.checked, false) })} />
            Use my remembered phrases
          </label>
          <label className="ml-6 flex items-center gap-2" style={s.ghost.memory ? undefined : muted}>
            <input type="checkbox" disabled={!s.ghost.memory} checked={s.ghost.window} onChange={(e) => void update({ ghost: ghostLayers(true, e.currentTarget.checked) })} />
            Also use the window title (about 7 ms more)
          </label>
        </div>
      </Card>

      <PreviewCard />
    </div>
  );
}

function LanguageField({ value, onCommit }: { value: string; onCommit: (v: string) => void }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <label className="flex items-center gap-2">
      Language
      <input
        aria-label="Language"
        value={draft}
        maxLength={40}
        placeholder="Same as my text"
        onChange={(e) => setDraft(e.currentTarget.value)}
        onBlur={() => draft !== value && onCommit(draft.trim())}
        className="rounded-lg px-2 py-1"
        style={{ background: "var(--surface-raised)", border: "1px solid var(--border)" }}
      />
    </label>
  );
}

const TOOLBAR: { format: Format; label: string; title: string }[] = [
  { format: "bold", label: "B", title: "Bold" },
  { format: "italic", label: "I", title: "Italic" },
  { format: "heading", label: "H", title: "Heading" },
  { format: "bullet", label: "•", title: "Bulleted list" },
  { format: "task", label: "☐", title: "Checklist" },
  { format: "quote", label: "❝", title: "Quote" },
  { format: "code", label: "</>", title: "Code" },
  { format: "link", label: "🔗", title: "Link" },
];

/** profile.md editor with a Markdown toolbar and a live preview (GitHub-flavored, :emoji: shortcodes). */
function ProfileEditor({ view, onSaved }: { view: ContextView; onSaved: () => Promise<void> }) {
  const [text, setText] = useState(view.profile);
  const [saved, setSaved] = useState(view.profile);
  const [mode, setMode] = useState<"edit" | "preview">("edit");
  const [status, setStatus] = useState<string | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  // Pick up edits made outside BOB, unless the user has unsaved changes here.
  useEffect(() => {
    if (text === saved && view.profile !== saved) {
      setText(view.profile);
      setSaved(view.profile);
    }
  }, [view.profile, text, saved]);

  const bytes = byteLength(text);
  const tooLong = bytes > view.max_profile_bytes;
  const dirty = text !== saved;

  const format = (f: Format) => {
    const el = area.current;
    const r = applyFormat(text, el?.selectionStart ?? text.length, el?.selectionEnd ?? text.length, f);
    setText(r.text);
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(r.start, r.end);
    });
  };

  const save = async (value: string = text) => {
    try {
      await contextApi.saveProfile(value);
      setSaved(value);
      setStatus("Saved");
      await onSaved();
    } catch (e) {
      setStatus(String(e));
    }
  };

  // Autosave like Mac apps: 800 ms after typing stops, and when leaving the panel with unsaved edits.
  const latest = useRef({ text, dirty, tooLong });
  latest.current = { text, dirty, tooLong };
  useEffect(() => {
    if (!dirty || tooLong) return;
    const t = setTimeout(() => void save(text), 800);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);
  useEffect(
    () => () => {
      const l = latest.current;
      if (l.dirty && !l.tooLong) void contextApi.saveProfile(l.text).catch(() => {});
    },
    [],
  );

  return (
    <Card title="Your profile" label="Profile">
      <p className="mb-3 text-xs" style={muted}>
        Tell BOB who you are and how you write. Markdown is welcome: headings, lists, checklists, links and emoji (type :jack_o_lantern: or press Ctrl+Cmd+Space). Saved as{" "}
        <code>{view.profile_path ?? "profile.md"}</code>, on this Mac only.
      </p>
      <div className="mb-2 flex flex-wrap items-center gap-1">
        <div role="tablist" aria-label="Profile view" className="mr-2 flex gap-1">
          {(["edit", "preview"] as const).map((m) => (
            <SmallButton key={m} role="tab" aria-selected={mode === m} onClick={() => setMode(m)} style={mode === m ? { background: "var(--accent)", color: "var(--accent-fg)" } : undefined}>
              {m === "edit" ? "Edit" : "Preview"}
            </SmallButton>
          ))}
        </div>
        {mode === "edit" &&
          TOOLBAR.map((t) => (
            <SmallButton key={t.format} title={t.title} aria-label={t.title} onClick={() => format(t.format)}>
              {t.label}
            </SmallButton>
          ))}
        {mode === "edit" && !text && (
          <SmallButton onClick={() => setText(view.template)} className="ml-2">
            Start from a template
          </SmallButton>
        )}
      </div>
      {mode === "edit" ? (
        <textarea
          ref={area}
          aria-label="Profile (Markdown)"
          value={text}
          onChange={(e) => {
            setText(e.currentTarget.value);
            setStatus(null);
          }}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
              e.preventDefault();
              if (dirty && !tooLong) void save();
            }
            if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "b") {
              e.preventDefault();
              format("bold");
            }
            if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "i") {
              e.preventDefault();
              format("italic");
            }
          }}
          rows={12}
          spellCheck
          placeholder="# About me ✍️"
          className="w-full rounded-xl p-3 font-mono text-sm"
          style={{ background: "var(--surface-raised)", border: "1px solid var(--border)" }}
        />
      ) : (
        <div data-testid="profile-preview" className="markdown rounded-xl p-3 text-sm" style={{ background: "var(--surface-raised)", minHeight: "8rem" }}>
          {text.trim() ? (
            <ReactMarkdown
              remarkPlugins={[remarkGfm, remarkEmoji]}
              components={{
                // Links stay text: following one would navigate BOB's window away.
                a: ({ children, href }) => (
                  <span className="underline" title={href}>
                    {children}
                  </span>
                ),
                // Images are never fetched (no network requests from a profile).
                img: ({ alt }) => <span style={muted}>[image: {alt}]</span>,
              }}
            >
              {text}
            </ReactMarkdown>
          ) : (
            <span style={muted}>Nothing to preview yet.</span>
          )}
        </div>
      )}
      <div className="mt-2 flex items-center gap-3 text-xs">
        <SmallButton onClick={() => void save()} disabled={!dirty || tooLong} style={dirty && !tooLong ? { background: "var(--accent)", color: "var(--accent-fg)" } : undefined}>
          {dirty ? "Save now" : "Saved"}
        </SmallButton>
        {dirty && <SmallButton onClick={() => setText(saved)}>Discard changes</SmallButton>}
        <span style={tooLong ? { color: "var(--danger)" } : muted} data-testid="profile-size">
          {Math.round(bytes / 100) / 10} of {view.max_profile_bytes / 1024} KB
        </span>
        {status && <span role="status">{status}</span>}
      </div>
    </Card>
  );
}

/** Shows exactly what BOB would add to prompts right now (only when asked). */
function PreviewCard() {
  const [preview, setPreview] = useState<ContextPreview | null>(null);
  return (
    <Card title="Preview what BOB sends" label="Context preview">
      <p className="mb-2 text-xs" style={muted}>
        The context BOB adds right now, after redaction. Nothing is sent by previewing.
      </p>
      <SmallButton onClick={() => void contextApi.preview().then(setPreview)}>{preview ? "Refresh" : "Show preview"}</SmallButton>
      {preview && (
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          <div>
            <h3 className="mb-1 text-sm font-medium">Ghost Text notes</h3>
            <pre data-testid="preview-ghost" className="max-h-64 overflow-auto whitespace-pre-wrap rounded-xl p-3 text-xs" style={{ background: "var(--surface-raised)" }}>
              {preview.ghost_notes || "(none: every Ghost Text layer is off)"}
            </pre>
          </div>
          <div>
            <h3 className="mb-1 text-sm font-medium">Selection Actions and Dictation</h3>
            <pre data-testid="preview-chat" className="max-h-64 overflow-auto whitespace-pre-wrap rounded-xl p-3 text-xs" style={{ background: "var(--surface-raised)" }}>
              {preview.chat_system}
            </pre>
          </div>
        </div>
      )}
    </Card>
  );
}
