import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import {
  auditLabel,
  generalApi,
  LOG_LEVELS,
  loginLabel,
  mcpSnippet,
  pauseLabel,
  SHELL_EVENT,
  type AuditEntry,
  type GeneralView,
  type LogLevel,
  type LogLine,
  type PauseHow,
} from "../lib/general";
import { THEMES, type Theme } from "../lib/theme";
import { useTauriEvent } from "../lib/useTauriEvent";
import { GhostMark } from "./GhostMark";
import { Button, Group, Row, Segmented, Toggle } from "./ui";

const muted = { color: "var(--text-muted)" } as const;

const AUDIT_FILTERS: { id: string; label: string }[] = [
  { id: "", label: "All" },
  { id: "data.", label: "Data sent" },
  { id: "settings.", label: "Settings" },
  { id: "brain.", label: "Brains" },
  { id: "mcp.", label: "AI agents" },
];

const time = (ms: number) => new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit", second: "2-digit" });

/** Settings (Cmd+,): Mac behavior, pause, appearance, AI agents, logs and the activity trail, about. */
export function SettingsPanel({ build, theme, onTheme, onTour }: { build: string; theme: Theme; onTheme: (t: Theme) => void; onTour: () => void }) {
  const [general, setGeneral] = useState<GeneralView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      setGeneral(await generalApi.get());
    } catch (e) {
      setError(String(e));
    }
  }, []);
  useEffect(() => void refresh(), [refresh]);
  useTauriEvent(SHELL_EVENT, () => void refresh());

  const run = async (f: () => Promise<unknown>) => {
    try {
      setError(null);
      await f();
    } catch (e) {
      setError(String(e));
    }
    await refresh();
  };
  const pauses: { how: PauseHow; label: string }[] = [
    { how: "hour", label: "1 hour" },
    { how: "tomorrow", label: "Until tomorrow" },
    { how: "indefinite", label: "Until I resume" },
  ];

  return (
    <div className="max-w-3xl">
      {error && (
        <p role="alert" className="mb-2" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}
      <Group title="General">
        {!general ? (
          <Row title={isTauri() ? "Loading..." : "Open the BOB app to change these."} />
        ) : (
          <>
            <Row title="Open at login" detail={loginLabel(general.login_item)}>
              <Toggle
                label="Open at login"
                on={general.login_item === "enabled" || general.login_item === "requires_approval"}
                disabled={general.login_item === "unavailable"}
                onChange={(v) => void run(() => generalApi.setLogin(v))}
              />
            </Row>
            <Row title="Open the window at launch" detail="Off: BOB starts quietly in the menu bar.">
              <Toggle label="Open the window at launch" on={general.open_window_at_launch} onChange={(v) => void run(() => generalApi.setOpenAtLaunch(v))} />
            </Row>
            <Row title="Show in the Dock" detail="Off: BOB lives only in the menu bar.">
              <Toggle label="Show in the Dock" on={general.show_in_dock} onChange={(v) => void run(() => generalApi.setDock(v))} />
            </Row>
            <Row title="Pause BOB" detail={`${pauseLabel(general.pause)}. Stops suggestions, Selection Actions and dictation, for screen sharing or a quiet hour.`}>
              {general.pause.paused ? (
                <Button variant="primary" onClick={() => void run(generalApi.resume)}>
                  Resume
                </Button>
              ) : (
                pauses.map((p) => (
                  <Button key={p.how} onClick={() => void run(() => generalApi.pause(p.how))}>
                    {p.label}
                  </Button>
                ))
              )}
            </Row>
          </>
        )}
      </Group>

      <Group title="Appearance" footer="With Reduce Motion on in macOS, BOB stops floating and wiggling.">
        <Row title="Theme">
          <Segmented label="Theme" value={theme} options={THEMES.map((t) => ({ id: t.id, label: t.label }))} onChange={onTheme} />
        </Row>
      </Group>

      {general && <AgentsGroup general={general} run={run} />}
      {general && <LogsGroup general={general} run={run} />}

      <Group title="Keyboard">
        <Row title="Accept a word, everything, or dismiss" detail="While a suggestion is visible">
          <span style={muted}>
            <kbd>Tab</kbd> · <kbd>⇧Tab</kbd> · <kbd>Esc</kbd>
          </span>
        </Row>
        <Row title="Selection Actions" detail="Change it in Selection Actions">
          <kbd>⌘⇧J</kbd>
        </Row>
        <Row title="Voice Dictation" detail="Hold, or tap to start and tap again to stop">
          <kbd>⌥Space</kbd>
        </Row>
        <Row title="Switch panels · Settings">
          <span style={muted}>
            <kbd>⌘1</kbd> to <kbd>⌘8</kbd> · <kbd>⌘,</kbd>
          </span>
        </Row>
      </Group>

      <Group title="Help">
        <Row title="Welcome tour" detail="Permissions, models and a cheat sheet, step by step.">
          <Button onClick={onTour}>Show the welcome tour</Button>
        </Row>
      </Group>

      <section aria-label="About BOB" className="mac-group panel-in">
        <div className="mac-group-body flex items-center gap-4 py-4">
          <div className="shrink-0">
            <GhostMark size={52} float interactive />
          </div>
          <div>
            <h2 className="font-semibold">BOB: Bring Your Own Brain</h2>
            <p className="mt-1 text-xs" style={muted}>
              {build}. No accounts, no telemetry. Text goes only to the brain you choose; cloud brains get no personal context unless you allow it.
              Speech is recognized on this Mac and audio is never saved. Built with Kiro for Kiroween 2026 with llama.cpp, whisper.cpp, Tauri, Swift and
              React (THIRD_PARTY_NOTICES.md). MIT licensed.
            </p>
          </div>
        </div>
      </section>
    </div>
  );
}

function AgentsGroup({ general, run }: { general: GeneralView; run: (f: () => Promise<unknown>) => Promise<void> }) {
  const [copied, setCopied] = useState(false);
  const snippet = mcpSnippet(general.mcp_command);
  return (
    <Group
      title="AI agents (MCP)"
      footer="Agents can rewrite text and get suggestions with your brains and style, check status, and pause BOB. Every call is in the activity list below; text is never logged."
    >
      <Row title="Allow AI agents to use BOB" detail="For Kiro, Claude Desktop and other MCP clients on this Mac. Off by default.">
        <Toggle label="Allow AI agents to use BOB" on={general.mcp_enabled} onChange={(v) => void run(() => generalApi.setMcp(v))} />
      </Row>
      {general.mcp_enabled && (
        <div className="py-2">
          <p className="mac-row-detail mb-1">Add this to the agent's MCP settings (in Kiro: .kiro/settings/mcp.json):</p>
          <pre className="mac-field overflow-auto text-xs" data-testid="mcp-snippet" style={{ userSelect: "text" }}>
            {snippet}
          </pre>
          <Button
            className="mt-2"
            onClick={() =>
              void navigator.clipboard.writeText(snippet).then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              })
            }
          >
            {copied ? "Copied" : "Copy configuration"}
          </Button>
        </div>
      )}
    </Group>
  );
}

function LogsGroup({ general, run }: { general: GeneralView; run: (f: () => Promise<unknown>) => Promise<void> }) {
  const [filter, setFilter] = useState("");
  const [audit, setAudit] = useState<AuditEntry[]>([]);
  const [lines, setLines] = useState<LogLine[] | null>(null);
  const [lineLevel, setLineLevel] = useState<LogLevel>("info");
  const load = useCallback(async () => {
    try {
      setAudit((await generalApi.audit(filter || null, 100)) ?? []);
    } catch {
      /* next refresh */
    }
  }, [filter]);
  useEffect(() => {
    void load();
    const t = setInterval(() => void load(), 4000);
    return () => clearInterval(t);
  }, [load]);
  const showLines = async (level: LogLevel) => {
    setLineLevel(level);
    setLines((await generalApi.logs(level)) ?? []);
  };
  return (
    <>
      <Group title="Logs" footer={general.log_dir ? `Saved in ${general.log_dir} (bob.log, rotated at 5 MB). Logs contain sizes and timings, never your text.` : undefined}>
        <Row title="Detail level" detail="Raise it only to troubleshoot; Detailed and Everything write more.">
          <select aria-label="Log detail level" value={general.log_level} onChange={(e) => void run(() => generalApi.setLogLevel(e.currentTarget.value as LogLevel))} className="mac-field">
            {LOG_LEVELS.map((l) => (
              <option key={l.id} value={l.id}>
                {l.label}
              </option>
            ))}
          </select>
        </Row>
        <Row title="Recent log lines">
          <Button onClick={() => void showLines(lineLevel)}>{lines ? "Refresh" : "Show"}</Button>
          <Button onClick={() => void run(generalApi.openLogs)}>Show in Finder</Button>
        </Row>
        {lines && (
          <div className="py-2">
            <Segmented
              label="Log lines to show"
              value={lineLevel}
              options={[
                { id: "error" as LogLevel, label: "Errors" },
                { id: "warn" as LogLevel, label: "Warnings" },
                { id: "info" as LogLevel, label: "Info" },
                { id: "debug" as LogLevel, label: "Debug" },
              ]}
              onChange={(l) => void showLines(l)}
            />
            <pre className="mac-field mt-2 max-h-56 overflow-auto text-[11px] leading-snug" data-testid="log-lines" style={{ userSelect: "text" }}>
              {lines.length === 0 ? "No lines at this level yet." : lines.slice(-200).map((l) => `${time(l.at)} ${l.level.padEnd(5)} ${l.target}: ${l.message}`).join("\n")}
            </pre>
          </div>
        )}
      </Group>

      <Group title="Activity" footer="What BOB did that matters for your privacy: settings changes, brains, text sent (sizes only), AI agent calls. Kept in audit.jsonl next to the log.">
        <Row title="Show">
          <Segmented label="Activity filter" value={filter} options={AUDIT_FILTERS} onChange={setFilter} />
        </Row>
        <ul className="max-h-64 overflow-auto py-1" aria-label="Activity" data-testid="audit-list">
          {audit.length === 0 && (
            <li className="py-1 text-xs" style={muted}>
              Nothing recorded yet.
            </li>
          )}
          {audit.map((e, i) => (
            <li key={`${e.at}-${i}`} className="flex gap-3 py-1 text-xs">
              <span className="w-32 shrink-0 tabular-nums" style={muted}>
                {time(e.at)}
              </span>
              <span>{auditLabel(e)}</span>
            </li>
          ))}
        </ul>
      </Group>
    </>
  );
}
