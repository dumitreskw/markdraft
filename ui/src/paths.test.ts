import { describe, expect, it } from "vitest";
import { baseName, dirName, extension, joinPath, relParent, relativeTo, resolveLink, stem } from "./paths";

describe("joinPath", () => {
  it("uses the root's separator", () => {
    expect(joinPath("C:\\docs", "a/b.md")).toBe("C:\\docs\\a\\b.md");
    expect(joinPath("/home/me/docs", "a/b.md")).toBe("/home/me/docs/a/b.md");
    expect(joinPath("/home/me/docs/", "x.md")).toBe("/home/me/docs/x.md");
    expect(joinPath("/r", "")).toBe("/r");
  });
});

describe("relativeTo", () => {
  it("returns slash paths inside the root and null outside", () => {
    expect(relativeTo("C:\\docs", "C:\\docs\\a\\b.md")).toBe("a/b.md");
    expect(relativeTo("/r", "/r/x.md")).toBe("x.md");
    expect(relativeTo("/r", "/r")).toBe("");
    expect(relativeTo("/r", "/rx/y.md")).toBeNull();
  });
});

describe("names", () => {
  it("splits names and extensions", () => {
    expect(baseName("C:\\a\\b.md")).toBe("b.md");
    expect(stem("notes.v2.md")).toBe("notes.v2");
    expect(extension("README.MD")).toBe("md");
    expect(extension(".gitignore")).toBe("");
    expect(dirName("/a/b/c.md")).toBe("/a/b");
    expect(dirName("C:\\x.md")).toBe("C:\\");
    expect(relParent("a/b/c.md")).toBe("a/b");
    expect(relParent("c.md")).toBe("");
  });
});

describe("resolveLink", () => {
  it("resolves relative links and keeps fragments", () => {
    expect(resolveLink("/r/docs/a.md", "../guide.md#intro")).toEqual({ path: "/r/guide.md", fragment: "intro" });
    expect(resolveLink("C:\\r\\docs\\a.md", "sub/My%20Note.md")).toEqual({
      path: "C:\\r\\docs\\sub\\My Note.md",
      fragment: null,
    });
    expect(resolveLink("/r/a.md", "#section")).toEqual({ path: "/r/a.md", fragment: "section" });
  });

  it("ignores external links", () => {
    expect(resolveLink("/r/a.md", "https://x.org")).toBeNull();
    expect(resolveLink("/r/a.md", "mailto:a@b.c")).toBeNull();
  });
});
