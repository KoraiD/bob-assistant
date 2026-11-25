import { useCallback, useEffect, useState } from "react";
import { isTauri } from "../lib/appInfo";
import { api, ghostSummary, type GhostStatus, type Snapshot } from "../lib/brains";
import { ChipOffsetEditor, PhraseEditor, QuietAppsEditor } from "./GhostLists";
import { Group, Row, Toggle } from "./ui";
import { useTauriEvent } from "../lib/useTauriEvent";

/** Overview card for Ghost Text: on/off and live latency (refreshes every 2 s while visible). */
export function GhostCard() {
  const [status, setStatus] = useState<GhostStatus | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) return;
    try {
      const [g, s] = await Promise.all([api.ghostStatus(), api.snapshot()]);
      setStatus(g);
      setSnapshot(s);
    } catch {
      /* the next tick retries */
    }
  }, []);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), 2000);
    return () => clearInterval(t);
  }, [refresh]);
  useTauriEvent("brains-changed", () => void refresh());

  const toggle = async () => {
    if (!status) return;
    await api.ghostSetEnabled(!status.enabled);
    void refresh();
  };

  const summary = isTauri()
    ? ghostSummary(status, snapshot?.roles.ghost ?? null, snapshot?.builtin.state === "ready")
    : "Open the BOB app to use Ghost Text.";
  const s = status?.stats;
  const option = async (name: "typo_fixes" | "mid_sentence", on: boolean) => {
    await api.ghostSetOption(name, on);
    void refresh();
  };
  return (
    <div className="max-w-3xl">
      <Group
        label="Ghost Text"
        footer={
          <>
            <kbd>Tab</kbd> takes a word, <kbd>⇧Tab</kbd> takes the rest, <kbd>Esc</kbd> dismisses. Never in password fields, and never while an app shows its own list
            (Slack @mentions, #channels, :emoji:, /commands).
          </>
        }
      >
        <Row title="Suggest as you type" detail={<span data-testid="ghost-summary">{summary}</span>}>
          {status && <Toggle label="Ghost Text" on={status.enabled} onChange={() => void toggle()} />}
        </Row>
        {status && (
          <>
            <Row title="Fix typos" detail="After you finish a misspelled word, BOB offers the fix (teh → the). Tab replaces it. Uses the macOS spell checker on this Mac.">
              <Toggle label="Fix typos" on={status.typo_fixes} onChange={(v) => void option("typo_fixes", v)} />
            </Row>
            <Row title="Suggest inside a sentence" detail="When the cursor is in the middle of a sentence, BOB fills the gap using the words after it.">
              <Toggle label="Suggest inside a sentence" on={status.mid_sentence} onChange={(v) => void option("mid_sentence", v)} />
            </Row>
          </>
        )}
        {s && s.chips_shown > 0 && (
          <Row
            title="This session"
            detail={
              <span data-testid="ghost-stats">
                {s.chips_shown} suggestions · {s.accepted_words} words taken with Tab · {s.accepted_all} with ⇧Tab · {s.typing_through} typed through · {s.dismissed} dismissed
              </span>
            }
          />
        )}
        {s?.last_error && (
          <Row
            title="Last problem"
            detail={
              <span role="alert" style={{ color: "var(--danger)" }}>
                {s.last_error}
              </span>
            }
          />
        )}
      </Group>
      {status && (
        <Group title="Apps and phrases" label="Apps and phrases">
          <div className="pb-2">
            <QuietAppsEditor status={status} onChanged={refresh} />
            <ChipOffsetEditor status={status} onChanged={refresh} />
            <PhraseEditor count={status.memory_count} onChanged={refresh} />
          </div>
        </Group>
      )}
    </div>
  );
}
