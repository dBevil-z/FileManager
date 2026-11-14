export const project = {
  repository: "https://github.com/dBevil-z/FileManager",
  author: "https://github.com/dBevil-z",
  issues: "https://github.com/dBevil-z/FileManager/issues/new/choose",
  releases: "https://github.com/dBevil-z/FileManager/releases",
  latest: "https://github.com/dBevil-z/FileManager/releases/latest",
  guide: "https://github.com/dBevil-z/FileManager/blob/main/README.zh-CN.md",
  roadmap: "https://github.com/dBevil-z/FileManager/blob/main/ROADMAP.zh-CN.md",
} as const;

export function repository(url: string) {
  try {
    const u = new URL(url);
    if (
      u.protocol !== "https:" ||
      u.hostname !== "github.com" ||
      u.port ||
      u.username ||
      u.password ||
      u.search ||
      u.hash
    )
      return null;
    const parts = u.pathname.split("/").filter(Boolean);
    if (
      parts.length < 1 ||
      parts.length > 2 ||
      parts.some((p) => !/^[A-Za-z0-9_.-]+$/.test(p) || p === "." || p === "..")
    )
      return null;
    return {
      url: u.origin + "/" + parts.join("/"),
      isRepo: parts.length === 2,
    };
  } catch {
    return null;
  }
}

export function migrateRepositoryUrl(value: unknown): string {
  const valid = typeof value === "string" ? repository(value) : null;
  return !valid || valid.url === project.author
    ? project.repository
    : valid.url;
}
