import type { ButtonHTMLAttributes, ReactNode } from "react";

/**
 * Mac-style building blocks (System Settings layout): grouped inset lists with hairline separators,
 * switches, segmented controls and SF Symbols-like line icons. The haunted palette comes from the tokens.
 */

/** A macOS switch. Keeps `role="switch"` and an accessible name. */
export function Toggle({ on, label, onChange, disabled }: { on: boolean; label: string; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
      className={`mac-toggle${on ? " on" : ""}`}
    >
      <span className="mac-toggle-knob" aria-hidden />
    </button>
  );
}

/** A titled group of rows, like a System Settings section. */
export function Group({ title, footer, children, label }: { title?: string; footer?: ReactNode; children: ReactNode; label?: string }) {
  return (
    <section aria-label={label ?? title} className="mac-group panel-in">
      {title && <h2 className="mac-group-title">{title}</h2>}
      <div className="mac-group-body">{children}</div>
      {footer && <div className="mac-group-footer">{footer}</div>}
    </section>
  );
}

/** One row: title and optional detail on the left, a control on the right. */
export function Row({ title, detail, children, htmlFor }: { title: ReactNode; detail?: ReactNode; children?: ReactNode; htmlFor?: string }) {
  return (
    <div className="mac-row">
      <div className="min-w-0 flex-1">
        {htmlFor ? (
          <label htmlFor={htmlFor} className="mac-row-title">
            {title}
          </label>
        ) : (
          <div className="mac-row-title">{title}</div>
        )}
        {detail && <div className="mac-row-detail">{detail}</div>}
      </div>
      {children !== undefined && <div className="flex shrink-0 items-center gap-2">{children}</div>}
    </div>
  );
}

/** A segmented control (a radio group). */
export function Segmented<T extends string>({ value, options, onChange, label }: { value: T; options: { id: T; label: string }[]; onChange: (v: T) => void; label: string }) {
  return (
    <div role="radiogroup" aria-label={label} className="mac-segmented">
      {options.map((o) => (
        <button key={o.id} type="button" role="radio" aria-checked={value === o.id} className={value === o.id ? "on" : undefined} onClick={() => onChange(o.id)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Button({ variant = "secondary", className, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "primary" | "secondary" | "danger" | "plain" }) {
  return <button type="button" {...props} className={`mac-button ${variant}${className ? ` ${className}` : ""}`} />;
}

const paths: Record<string, ReactNode> = {
  overview: <path d="M3.5 9.5 10 4l6.5 5.5V16a1 1 0 0 1-1 1h-3.5v-4.5h-4V17H4.5a1 1 0 0 1-1-1z" />,
  ghost: (
    <>
      <path d="M5 16.5V9a5 5 0 0 1 10 0v7.5l-1.9-1.4-1.6 1.4-1.5-1.4-1.5 1.4-1.6-1.4z" />
      <circle cx="8.2" cy="9" r=".6" fill="currentColor" />
      <circle cx="11.8" cy="9" r=".6" fill="currentColor" />
    </>
  ),
  selection: (
    <>
      <path d="M4 5h12M4 9h8M4 13h10" />
      <path d="M15 11.5l.8 1.7 1.7.8-1.7.8-.8 1.7-.8-1.7-1.7-.8 1.7-.8z" />
    </>
  ),
  dictation: (
    <>
      <rect x="7.5" y="3" width="5" height="9" rx="2.5" />
      <path d="M5 9.5a5 5 0 0 0 10 0M10 14.5V17M7.5 17h5" />
    </>
  ),
  brains: (
    <>
      <rect x="5" y="5" width="10" height="10" rx="2" />
      <path d="M8 2.5v2.5M12 2.5v2.5M8 15v2.5M12 15v2.5M2.5 8h2.5M2.5 12h2.5M15 8h2.5M15 12h2.5" />
    </>
  ),
  personalize: <path d="M4 6h7M14 6h2M4 14h2M9 14h7M11 4v4M6 12v4" />,
  stats: <path d="M4 16V11M8 16V7M12 16V9M16 16V4M3 16.5h14" />,
  settings: (
    <>
      <circle cx="10" cy="10" r="2.6" />
      <path d="M10 2.8v2M10 15.2v2M2.8 10h2M15.2 10h2M4.9 4.9l1.4 1.4M13.7 13.7l1.4 1.4M4.9 15.1l1.4-1.4M13.7 6.3l1.4-1.4" />
    </>
  ),
};

/** A 20×20 line icon (SF Symbols-like weight). Decorative. */
export function Icon({ name, size = 18 }: { name: keyof typeof paths | string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      {paths[name]}
    </svg>
  );
}
