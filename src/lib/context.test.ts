import { applyFormat, byteLength } from "./context";

describe("applyFormat", () => {
  it("wraps a selection with inline markers and keeps it selected", () => {
    const r = applyFormat("I write emails", 8, 14, "bold");
    expect(r.text).toBe("I write **emails**");
    expect(r.text.slice(r.start, r.end)).toBe("emails");
  });

  it("inserts a placeholder when nothing is selected", () => {
    const r = applyFormat("Hi ", 3, 3, "italic");
    expect(r.text).toBe("Hi _italic text_");
    expect(r.text.slice(r.start, r.end)).toBe("italic text");
  });

  it("selects the URL of a new link", () => {
    const r = applyFormat("my site", 3, 7, "link");
    expect(r.text).toBe("my [site](https://)");
    expect(r.text.slice(r.start, r.end)).toBe("https://");
  });

  it("prefixes every selected line once", () => {
    const text = "a\nb\nc";
    const r = applyFormat(text, 0, 3, "bullet");
    expect(r.text).toBe("- a\n- b\nc");
    expect(applyFormat(r.text, 0, 7, "bullet").text).toBe("- a\n- b\nc");
    expect(applyFormat("x\ntodo", 3, 3, "task").text).toBe("x\n- [ ] todo");
    expect(applyFormat("title", 2, 2, "heading").text).toBe("## title");
  });

  it("clamps out-of-range and reversed selections", () => {
    expect(applyFormat("abc", 10, -5, "code").text).toBe("`abc`");
  });
});

describe("byteLength", () => {
  it("counts UTF-8 bytes, so emoji count as more than one", () => {
    expect(byteLength("abc")).toBe(3);
    expect(byteLength("🎃")).toBe(4);
  });
});
