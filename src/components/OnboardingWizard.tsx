import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { api, formatBytes, percent, type ModelInfo, type ProgressEvent } from "../lib/brains";
import { dictationApi } from "../lib/dictation";
import { accessibilityState, getNativeStatus, microphoneState, onNativeStatus, requestPermissions, DISCONNECTED, type NativeStatus } from "../lib/nativeStatus";
import { usageApi, type SystemInfo } from "../lib/usage";
import { useTauriEvent } from "../lib/useTauriEvent";
import { GhostMark } from "./GhostMark";

export const ONBOARDED_KEY = "bob.onboarded";

export function isOnboarded(storage: Pick<Storage, "getItem"> = localStorage): boolean {
  return storage.getItem(ONBOARDED_KEY) === "1";
}

export const STEPS = ["welcome", "permissions", "brain", "voice", "tryit"] as const;
export type Step = (typeof STEPS)[number];

const muted = { color: "var(--text-muted)" } as const;
const field = { background: "var(--surface-raised)", border: "1px solid var(--border-strong)" } as const;

function Primary(props: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" {...props} className="rounded-full px-5 py-2 text-sm font-medium disabled:opacity-50" style={{ background: "var(--accent)", color: "var(--accent-fg)" }} />;
}
function Secondary(props: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" {...props} className="rounded-full px-4 py-2 text-sm disabled:opacity-50" style={field} />;
}

function Check({ ok, label, detail }: { ok: boolean; label: string; detail: string }) {
  return (
    <li className="flex items-start gap-3 rounded-xl p-3" style={{ background: "var(--surface-raised)" }}>
      <span aria-hidden="true" className="text-lg">{ok ? "✅" : "🕯️"}</span>
      <span>
        <span className="block text-sm font-medium">
          {label}: {ok ? "ready" : "not yet"}
        </span>
        <span className="block text-xs" style={muted}>{detail}</span>
      </span>
    </li>
  );
}

/** "Recommended for your Mac (Apple M3, 16 GB)" with the reason. */
function ForYourMac({ info }: { info: SystemInfo | null }) {
  if (!info) return null;
  return (
    <p className="mt-3 rounded-xl p-3 text-xs" style={{ background: "var(--accent-soft)" }} data-testid="wizard-recommendation">
      <b>Recommended for your Mac</b> ({info.chip}, {info.ram_gb} GB): {info.recommendation.note}
    </p>
  );
}
/** One downloadable model row with progress. */
function ModelRow({ model, progress, onDownload }: { model: ModelInfo | undefined; progress?: ProgressEvent; onDownload: () => void }) {
  if (!model) return null;
  const downloading = progress && !progress.done;
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl p-3" style={{ background: "var(--surface-raised)" }} data-testid={`wizard-model-${model.id}`}>
      <span>
        <span className="block text-sm font-medium">{model.name}</span>
        <span className="block text-xs" style={muted}>
          {formatBytes(model.size)} · {model.license} · stays on this Mac
        </span>
      </span>
      {model.installed ? (
        <span className="text-sm">✅ Installed</span>
      ) : downloading ? (
        <span className="text-sm" aria-live="polite">Summoning... {percent(progress.downloaded, progress.total)}%</span>
      ) : (
        <Primary onClick={onDownload}>Download</Primary>
      )}
      {progress?.error && <span role="alert" className="w-full text-xs" style={{ color: "var(--danger)" }}>{progress.error}. Check the connection and try again.</span>}
    </div>
  );
}

/**
 * First-run guide (UI-4): welcome, permissions, the built-in brain, the speech model, and a cheat sheet.
 * Every step can be skipped; it can be reopened from About.
 */
export function OnboardingWizard({ onClose }: { onClose: () => void }) {
  const [step, setStep] = useState<Step>("welcome");
  const [status, setStatus] = useState<NativeStatus>(DISCONNECTED);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [speech, setSpeech] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, ProgressEvent>>({});
  const [error, setError] = useState<string | null>(null);
  const [system, setSystem] = useState<SystemInfo | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);

  const refresh = useCallback(async () => {
    setStatus(await getNativeStatus());
    try {
      setModels(await api.models());
      setSpeech((await dictationApi.get()).models);
      setSystem(await usageApi.system());
    } catch {
      /* outside the app */
    }
  }, []);
  useEffect(() => {
    void refresh();
    let off: (() => void) | undefined;
    void onNativeStatus(setStatus).then((u) => (off = u));
    return () => off?.();
  }, [refresh]);
  useEffect(() => heading.current?.focus(), [step]);
  useTauriEvent<ProgressEvent>("model-progress", (p) => {
    setProgress((cur) => ({ ...cur, [p.id]: p }));
    if (p.done && !p.error) {
      // Wake BOB's own brain as soon as its model is in.
      if (models.some((m) => m.id === p.id)) void api.startBuiltin(p.id).catch(() => {});
      void refresh();
    }
  });

  const finish = () => {
    localStorage.setItem(ONBOARDED_KEY, "1");
    onClose();
  };
  const index = STEPS.indexOf(step);
  const next = () => (index === STEPS.length - 1 ? finish() : setStep(STEPS[index + 1]));
  const back = () => index > 0 && setStep(STEPS[index - 1]);
  const download = (id: string) => void api.download(id).catch((e) => setError(String(e)));
  const rec = system?.recommendation;
  const brains = rec ? rec.brain_models.map((id) => models.find((m) => m.id === id)).filter((m): m is ModelInfo => !!m) : models.slice(0, 1);
  const voice = speech.find((m) => m.id === rec?.speech_model) ?? speech[0];
  /** Download the speech model and make it the one dictation uses. */
  const downloadVoice = async (id: string) => {
    download(id);
    try {
      const v = await dictationApi.get();
      if (v.settings.model !== id) await dictationApi.setSettings({ ...v.settings, model: id });
    } catch (e) {
      setError(String(e));
    }
  };

  const pages: Record<Step, { title: string; body: ReactNode }> = {
    welcome: {
      title: "Boo! I'm BOB.",
      body: (
        <>
          <p className="text-sm" style={muted}>
            A friendly ghost that helps you write in any app, with a brain you choose. Everything runs on this Mac unless you connect a cloud brain.
          </p>
          <ul className="mt-4 space-y-2 text-sm">
            <li>👻 <b>Ghost Text</b>: I suggest the next words at your cursor. Tab takes them.</li>
            <li>✨ <b>Selection Actions</b>: select text, press ⌘⇧J, and I rewrite it in place.</li>
            <li>🎙️ <b>Voice Dictation</b>: hold ⌥Space, speak, and I type what you said.</li>
          </ul>
          <p className="mt-4 text-xs" style={muted}>Setup takes about two minutes, plus downloads.</p>
        </>
      ),
    },
    permissions: {
      title: "Let me in (politely)",
      body: (
        <>
          <p className="text-sm" style={muted}>macOS asks you to allow two things. I never read password fields, and I never save audio.</p>
          <ul className="mt-4 space-y-2">
            <Check ok={accessibilityState(status) === "ok"} label="Accessibility" detail="To read the text around your cursor and type suggestions. System Settings > Privacy & Security > Accessibility > BOB." />
            <Check ok={microphoneState(status) === "ok"} label="Microphone" detail="Only while you hold the dictation key. System Settings > Privacy & Security > Microphone > BOB." />
          </ul>
          {!status.connected && <p className="mt-3 text-xs" style={{ color: "var(--accent-2)" }}>My helper is still waking up. If this stays, quit and reopen BOB.</p>}
          <div className="mt-4">
            <Secondary onClick={() => void requestPermissions().catch((e) => setError(String(e)))}>Ask macOS for access</Secondary>
          </div>
        </>
      ),
    },
    brain: {
      title: "Give me a brain",
      body: (
        <>
          <p className="text-sm" style={muted}>My own brain is a small open model that runs on this Mac (best result in my benchmark, about 1.4 GB of memory). You can also connect Ollama, LM Studio or a cloud brain later in Brains.</p>
          <ForYourMac info={system} />
          <div className="mt-4 space-y-2">
            {brains.map((b) => (
              <ModelRow key={b.id} model={b} progress={progress[b.id]} onDownload={() => download(b.id)} />
            ))}
          </div>
        </>
      ),
    },
    voice: {
      title: "Teach me to listen",
      body: (
        <>
          <p className="text-sm" style={muted}>Speech is recognized on this Mac by whisper.{voice ? ` ${voice.name}: ${voice.role}.` : ""}{rec && voice?.id === rec.speech_model ? " Picked for your Mac." : ""}</p>
          <div className="mt-4">
            <ModelRow model={voice} progress={voice && progress[voice.id]} onDownload={() => voice && void downloadVoice(voice.id)} />
          </div>
        </>
      ),
    },
    tryit: {
      title: "Let's haunt something",
      body: (
        <>
          <p className="text-sm" style={muted}>Open any app (Notes or Mail is a good start) and try:</p>
          <ul className="mt-4 space-y-3 text-sm">
            <li>
              Type <i>"Thanks for your email, I will get back to you"</i> and pause. Press <kbd>Tab</kbd> for a word, <kbd>Shift</kbd>+<kbd>Tab</kbd> for all, <kbd>Esc</kbd> to dismiss.
            </li>
            <li>
              Select a sentence and press <kbd>⌘⇧J</kbd>. Pick "Fix grammar" with <kbd>↓</kbd> and <kbd>⏎</kbd>. <kbd>⌘Z</kbd> undoes it.
            </li>
            <li>
              Hold <kbd>⌥Space</kbd>, say "see you tomorrow", and let go.
            </li>
          </ul>
          <p className="mt-4 text-xs" style={muted}>You can change hotkeys and tell me about yourself in Personalize.</p>
        </>
      ),
    },
  };
  const page = pages[step];

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-6" style={{ background: "rgba(5, 4, 12, 0.72)" }} onKeyDown={(e) => e.key === "Escape" && finish()}>
      <section role="dialog" aria-modal="true" aria-labelledby="wizard-title" className="soft-card panel-in w-full max-w-xl p-7">
        <div className="mb-4 flex items-center justify-between">
          <GhostMark size={44} float />
          <span className="text-xs" style={muted}>
            <span className="sr-only">
              Step {index + 1} of {STEPS.length}
            </span>
            {STEPS.map((s, i) => (
              <span key={s} aria-hidden="true" style={{ color: i <= index ? "var(--accent)" : "var(--text-muted)" }}>●</span>
            ))}
          </span>
        </div>
        <h2 id="wizard-title" ref={heading} tabIndex={-1} className="mb-3 text-2xl font-semibold outline-none">
          {page.title}
        </h2>
        {page.body}
        {error && <p role="alert" className="mt-3 text-xs" style={{ color: "var(--danger)" }}>{error}</p>}
        <div className="mt-6 flex items-center justify-between gap-2">
          <Secondary onClick={finish}>{index === STEPS.length - 1 ? "Close" : "Skip setup"}</Secondary>
          <div className="flex gap-2">
            {index > 0 && <Secondary onClick={back}>Back</Secondary>}
            <Primary onClick={next}>{index === STEPS.length - 1 ? "Start haunting" : "Next"}</Primary>
          </div>
        </div>
      </section>
    </div>
  );
}
