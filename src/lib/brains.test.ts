import { applyEdit, editFrom, validateEdit, brainsForRole, ghostSummary, quietAppRows, type GhostStatus, draftToConfig, formatBytes, isCloudUrl, percent, slugify, uniqueId, validateDraft, type BrainView, type Draft } from "./brains";

const brain = (id: string, ghost: boolean, chat: boolean): BrainView => ({
  id, name: id, kind: "ollama", endpoint: "http://127.0.0.1:11434", model: null,
  capabilities: { ghost, chat }, has_api_key: false, is_cloud: false, shares_context: true,
});

describe("brains helpers", () => {
  it("formats sizes and percentages safely", () => {
    expect(formatBytes(1_117_320_736)).toBe("1.1 GB");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(-1)).toBe("?");
    expect(percent(50, 200)).toBe(25);
    expect(percent(5, 0)).toBe(0);
    expect(percent(300, 200)).toBe(100);
  });

  it("only offers capable brains for each role", () => {
    const all = [brain("g", true, false), brain("c", false, true), brain("both", true, true)];
    expect(brainsForRole(all, "ghost").map((b) => b.id)).toEqual(["g", "both"]);
    expect(brainsForRole(all, "selection").map((b) => b.id)).toEqual(["c", "both"]);
  });

  it("derives unique, valid ids from names", () => {
    expect(slugify("  My Ollama!! ")).toBe("my-ollama");
    expect(slugify("!!!")).toBe("brain");
    expect(uniqueId("a", ["a", "a-2"])).toBe("a-3");
    for (const name of ["Ünïcödé brain", "x".repeat(200), "a b/c\\\\d", ""]) {
      expect(slugify(name)).toMatch(/^[a-z0-9-]{1,48}$/);
    }
  });

  it("labels endpoints like the Rust side (loopback = local)", () => {
    expect(isCloudUrl("http://127.0.0.1:11434")).toBe(false);
    expect(isCloudUrl("http://localhost:1234")).toBe(false);
    expect(isCloudUrl("http://[::1]:80")).toBe(false);
    expect(isCloudUrl("https://api.openai.com")).toBe(true);
    expect(isCloudUrl("http://192.168.1.5:11434")).toBe(true);
    expect(isCloudUrl("http://localhost.evil.com")).toBe(true);
    expect(isCloudUrl("garbage")).toBe(true);
  });

  it("validates drafts with readable messages", () => {
    const ok: Draft = { name: "Ollama", kind: "ollama", endpoint: "http://127.0.0.1:11434", model: "", apiKey: "" };
    expect(validateDraft(ok)).toEqual([]);
    expect(validateDraft({ ...ok, name: " " })).toContain("Give the brain a name.");
    expect(validateDraft({ ...ok, endpoint: "ftp://x" })).toContain("The endpoint must start with http:// or https://.");
    expect(validateDraft({ ...ok, endpoint: "nope" })).toContain("The endpoint is not a valid URL.");
    expect(validateDraft({ ...ok, kind: "open_ai_compatible", endpoint: "https://api.openai.com" })).toContain("Cloud endpoints need an API key.");
  });

  it("builds configs with conservative capabilities for cloud APIs", () => {
    const c = draftToConfig({ name: "Cloud", kind: "open_ai_compatible", endpoint: " https://api.x.com ", model: " gpt ", apiKey: "k" }, ["cloud"]);
    expect(c).toMatchObject({ id: "cloud-2", endpoint: "https://api.x.com", model: "gpt", capabilities: { ghost: false, chat: true } });
    expect(JSON.stringify(c)).not.toContain('"k"');
  });
});

describe("ghostSummary", () => {
  const g = (over: Partial<GhostStatus> = {}): GhostStatus => ({
    enabled: true, p50_ms: null, p95_ms: null, quiet_apps: [], chip_offsets: [], recent_apps: [], typo_fixes: true, mid_sentence: true, memory_count: 0,
    stats: { requests: 0, chips_shown: 0, accepted_words: 0, accepted_all: 0, dismissed: 0, errors: 0, last_error: null, typing_through: 0 },
    ...over,
  });
  it("explains every state in plain words", () => {
    expect(ghostSummary(null, "builtin", true)).toBe("Checking...");
    expect(ghostSummary(g({ enabled: false }), "builtin", true)).toBe("Off");
    expect(ghostSummary(g(), null, true)).toMatch(/Choose a brain/);
    expect(ghostSummary(g(), "builtin", false)).toMatch(/wake up/);
    expect(ghostSummary(g(), "builtin", true)).toMatch(/Ready/);
    expect(ghostSummary(g({ p50_ms: 120, p95_ms: 240 }), "builtin", true)).toBe("Haunting your caret · p50 120 ms · p95 240 ms");
  });
});

describe("quietAppRows", () => {
  const a = (bundle_id: string, name = bundle_id) => ({ bundle_id, name });
  it("lists quiet apps first, then recent apps once each, skipping empty ids", () => {
    const rows = quietAppRows({
      quiet_apps: [a("com.googlecode.iterm2", "iTerm")],
      recent_apps: [a("com.apple.Notes", "Notes"), a("com.googlecode.iterm2", "iTerm"), a(""), a("com.apple.Notes", "Notes")],
    });
    expect(rows).toEqual([
      { bundle_id: "com.googlecode.iterm2", name: "iTerm", quiet: true },
      { bundle_id: "com.apple.Notes", name: "Notes", quiet: false },
    ]);
  });
  it("is empty when nothing is known", () => {
    expect(quietAppRows({ quiet_apps: [], recent_apps: [] })).toEqual([]);
  });
});

describe("validateDraft https rule", () => {
  it("refuses an API key over plain http to a server outside this Mac", () => {
    const d = { name: "X", kind: "open_ai_compatible" as const, endpoint: "http://api.example.com", model: "", apiKey: "k" };
    expect(validateDraft(d).join(" ")).toMatch(/https/);
    expect(validateDraft({ ...d, endpoint: "https://api.example.com" })).toEqual([]);
    expect(validateDraft({ ...d, kind: "lm_studio", endpoint: "http://127.0.0.1:1234" })).toEqual([]);
  });
});

describe("brain edits", () => {
  const cloud: BrainView = { ...brain("c", false, true), kind: "open_ai_compatible", endpoint: "https://api.x.com", has_api_key: true, is_cloud: true };
  it("keeps, replaces or removes the stored key", () => {
    const e = editFrom(cloud);
    expect(applyEdit(cloud, e).apiKey).toBeNull();
    expect(applyEdit(cloud, { ...e, apiKey: " new " }).apiKey).toBe("new");
    expect(applyEdit(cloud, { ...e, removeKey: true }).apiKey).toBe("");
  });
  it("a stored key satisfies the cloud key rule, removing it does not", () => {
    const e = editFrom(cloud);
    expect(validateEdit(cloud, e)).toEqual([]);
    expect(validateEdit(cloud, { ...e, removeKey: true }).join(" ")).toMatch(/API key/);
    expect(validateEdit(cloud, { ...e, name: " " }).join(" ")).toMatch(/name/);
  });
});
