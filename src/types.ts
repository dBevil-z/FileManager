export type Entry = {
  path: string;
  name: string;
  extension: string;
  kind: string;
  is_dir: boolean;
  is_symlink: boolean;
  target_is_dir: boolean;
  hidden: boolean;
  size: number | null;
  modified: number | null;
  created: number | null;
  accessed: number | null;
  inode: number;
  device: number;
  writable: boolean;
};
export type Directory = {
  path: string;
  display_path: string;
  parent: string | null;
  entries: Entry[];
  total: number;
  warnings: string[];
  writable: boolean;
};
export type Revision<T> = { data: T | null; revision: number };
export type Place = { id: string; name: string; path: string };
export type Workspace = {
  id: string;
  name: string;
  description: string;
  color: string;
  paths: string[];
  links: string[];
  pinned: string[];
  archived?: boolean;
  view?: "cards" | "list" | "tiles";
  tab?: string;
};
export type Bookmark = {
  id: string;
  url: string;
  title: string;
  description?: string;
  image?: string;
  site?: string;
  collections: string[];
  tags: string[];
  favourite: boolean;
  created: number;
  kind?: string;
};
export type Collection = {
  id: string;
  name: string;
  icon: string;
  color: string;
};
export type Catalog = {
  workspaces: Workspace[];
  bookmarks: Bookmark[];
  collections: Collection[];
  favourites: Place[];
  folderStyles: Record<string, { color?: string; symbol?: string }>;
  shelf: string[];
  holding: string[];
  quickAccess: string[];
  agentRoots: string[];
};
export type Settings = {
  theme: "system" | "light" | "dark";
  skin: string;
  reduceMotion: boolean;
  repositoryUrl: string;
  accent: string;
  folderStyle: "glass" | "plain" | "flat" | "system";
  folderColor: string;
  language: string;
  terminal: string;
  editor: string;
  showTray: boolean;
  startAtLogin: boolean;
  shelfShortcut: string;
  shelfOnDrag: boolean;
  shelfOnHover: boolean;
  sounds: boolean;
  shortcuts: Record<string, string>;
  indexWorkspaces: boolean;
  indexCollections: boolean;
  indexBookmarks: boolean;
  cacheLimitMB: number;
  thumbnailMode: "auto" | "icons";
};
export type Page =
  | { type: "folder"; path: string }
  | { type: "workspace"; id: string }
  | { type: "bookmarks"; id?: string; tag?: string; favourites?: boolean }
  | { type: "clean"; scope?: string; category?: string }
  | { type: "packages" }
  | { type: "plugins" }
  | { type: "remote" }
  | { type: "about" }
  | { type: "agents" }
  | { type: "system" }
  | { type: "settings" }
  | { type: "trash" }
  | { type: "markdown"; path: string; from?: Page };
export type Bootstrap = {
  home: string;
  data_dir: string;
  version: string;
  platform: string;
  places: Place[];
  catalog: Revision<Catalog>;
  settings: Revision<Partial<Settings>>;
  session: Revision<Record<string, unknown>>;
};
export type OperationResult = {
  id?: string;
  count?: number;
  requires_confirmation?: boolean;
  conflicts?: {
    source: Entry;
    destination: Entry;
    can_replace?: boolean;
    replacement_version?: string | null;
  }[];
};
export type Job = {
  id: string;
  op: string;
  status: "queued" | "running" | "done" | "cancelled" | "failed";
  completed: number;
  total: number;
  bytes: number;
  current: string;
  error?: string;
  result?: unknown;
  result_expired?: boolean;
};
export type HistoryItem = {
  id: string;
  label: string;
  status: "ready" | "running" | "partial" | "deferred" | "empty" | "undone";
  created: number;
};
export const emptyCatalog: Catalog = {
  workspaces: [],
  bookmarks: [],
  collections: [],
  favourites: [],
  folderStyles: {},
  shelf: [],
  holding: [],
  quickAccess: [],
  agentRoots: [],
};
export const defaultSettings: Settings = {
  theme: "system",
  skin: "graphite",
  reduceMotion: false,
  repositoryUrl: "https://github.com/dBevil-z/FileManager",
  accent: "#8b73ef",
  folderStyle: "glass",
  folderColor: "#8870ed",
  language: "zh-CN",
  terminal: "konsole",
  editor: "code",
  showTray: true,
  startAtLogin: false,
  shelfShortcut: "Control+Alt+Space",
  shelfOnDrag: true,
  shelfOnHover: true,
  sounds: false,
  shortcuts: {},
  indexWorkspaces: true,
  indexCollections: true,
  indexBookmarks: true,
  cacheLimitMB: 512,
  thumbnailMode: "auto",
};
