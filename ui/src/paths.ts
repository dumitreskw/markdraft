// Path helpers. Paths from Rust are native (`\` on Windows); paths inside
// a root are `/`-separated. These helpers convert between the two.

export function separatorOf(path: string): "/" | "\\" {
  return path.includes("\\") && !path.startsWith("/") ? "\\" : "/";
}

/** Joins a native root path and a `/`-separated relative path. */
export function joinPath(root: string, rel: string): string {
  if (!rel) return root;
  const sep = separatorOf(root);
  const trimmed = root.endsWith(sep) ? root.slice(0, -1) : root;
  return trimmed + sep + rel.split("/").join(sep);
}

/** Windows and macOS file systems are case-insensitive by default. */
const caseInsensitive = () =>
  typeof navigator === "undefined" || /Win|Mac/.test(navigator.userAgent ?? "");

function normalise(path: string): string {
  const p = path.replace(/\\/g, "/").replace(/\/+$/, "");
  return caseInsensitive() ? p.toLowerCase() : p;
}

export function samePath(a: string, b: string): boolean {
  return normalise(a) === normalise(b);
}

/** `child` relative to `root` with `/` separators, or null if outside it. */
export function relativeTo(root: string, child: string): string | null {
  const r = normalise(root);
  const c = normalise(child);
  if (c === r) return "";
  if (!c.startsWith(r + "/")) return null;
  // Slice the original path so its case is preserved.
  return child.replace(/\\/g, "/").replace(/\/+$/, "").slice(r.length + 1);
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function dirName(path: string): string {
  const sep = separatorOf(path);
  const i = path.lastIndexOf(sep);
  if (i <= 0) return sep === "/" ? "/" : path;
  // Keep the trailing separator for drive roots like `C:\`.
  return /^[A-Za-z]:$/.test(path.slice(0, i)) ? path.slice(0, i + 1) : path.slice(0, i);
}

export function stem(name: string): string {
  const base = baseName(name);
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(0, dot) : base;
}

export function extension(name: string): string {
  const base = baseName(name);
  const dot = base.lastIndexOf(".");
  return dot > 0 ? base.slice(dot + 1).toLowerCase() : "";
}

/** Parent of a `/`-separated relative path ("" for top level). */
export function relParent(rel: string): string {
  const i = rel.lastIndexOf("/");
  return i < 0 ? "" : rel.slice(0, i);
}

/**
 * Resolves a relative link (`../guide.md#intro`) against the folder of a
 * document. Returns the native path and the fragment, or null for
 * external links.
 */
export function resolveLink(docPath: string, href: string): { path: string; fragment: string | null } | null {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//")) return null;
  const [rawPath, ...frag] = href.split("#");
  const fragment = frag.length ? decodeURIComponent(frag.join("#")) : null;
  if (!rawPath) return { path: docPath, fragment };
  const sep = separatorOf(docPath);
  const decoded = decodeURIComponent(rawPath.split("?")[0]);
  const base = decoded.startsWith("/") ? [] : dirName(docPath).split(/[\\/]/);
  for (const part of decoded.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (base.length > 1) base.pop();
    } else base.push(part);
  }
  let path = base.join(sep);
  if (sep === "/" && !path.startsWith("/")) path = "/" + path;
  return { path, fragment };
}
