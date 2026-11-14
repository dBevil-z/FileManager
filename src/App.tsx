import { t, setLanguage, useLanguage } from "./i18n";
import {
  useCallback,
  useReducer,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  ArrowLeftRight,
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Folder,
  Layers,
  Maximize2,
  Minus,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  PanelTop,
  Plus,
  RotateCcw,
  Search,
  Terminal,
  X,
  AlertTriangle,
  Download,
  LoaderCircle,
  Command,
  FileText,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { AppContext, type AppContextValue } from "./context";
import { request, desktop, errorText, native } from "./api";
import { useAsync } from "./hooks/useAsync";
import {
  defaultSettings,
  emptyCatalog,
  type Bootstrap,
  type Catalog,
  type Entry,
  type HistoryItem,
  type Job,
  type OperationResult,
  type Page,
  type Revision,
  type Settings,
} from "./types";
import { basename, formatBytes, isFolder } from "./lib/files";
import { Sidebar } from "./components/Sidebar";
import { FileBrowser, command } from "./components/FileBrowser";
import { PreviewPane, Peek } from "./components/PreviewPane";
import { FilePicker } from "./components/FilePicker";
import {
  Button,
  ErrorState,
  IconButton,
  Loading,
  Modal,
} from "./components/ui";
import { FileIcon } from "./components/FileIcon";
import { ConflictDialog } from "./components/ConflictDialog";
import { Workspaces, WorkspaceEditor } from "./features/Workspaces";
import { Bookmarks, BookmarkEditor } from "./features/Bookmarks";
import { Clean, Packages, SystemPage, TrashPage } from "./features/Tools";
import { Agents } from "./features/Agents";
import { SettingsPage } from "./features/Settings";
import { MarkdownEditor } from "./features/Markdown";
import { Shelf } from "./features/Shelf";
import { keyString, shortcutDefaults } from "./lib/shortcuts";
import {
  PluginsPage,
  PluginRunner,
  type PluginCatalog,
} from "./features/Plugins";
import { AboutPage } from "./features/About";
import { tabsReducer, restoreTabs } from "./lib/tabs";
import { clearPreviewCache } from "./lib/previews";
import { applySkin } from "./lib/themes";
import { dragState } from "./lib/drag";

export default function App() {
  useLanguage();
  const boot = useAsync(() => request<Bootstrap>("bootstrap"), []);
  if (boot.error)
    return (
      <div className="app-start">
        <ErrorState error={boot.error} retry={() => void boot.refresh()} />
      </div>
    );
  if (!boot.data)
    return (
      <div className="app-start">
        <div className="startup-mark">
          <Folder size={35} />
        </div>
        <Loading label={t("打开 FileManager…")} />
      </div>
    );
  return <Desktop boot={boot.data} />;
}
function Desktop({ boot }: { boot: Bootstrap }) {
  const standalone =
    new URLSearchParams(location.search).get("window") === "shelf";
  const initialPage = (boot.session.data?.page as Page) || {
    type: "folder",
    path: boot.places.find((p) => p.id === "downloads")?.path || boot.home,
  };
  const [page, setPage] = useState<Page>(initialPage);
  const [folderTabs, tabAction] = useReducer(tabsReducer, undefined, () =>
    restoreTabs(
      boot.session.data?.folderTabs,
      boot.session.data?.activeTab,
      initialPage.type === "folder" ? initialPage.path : boot.home,
    ),
  );
  useEffect(() => {
    if (page.type === "folder")
      tabAction({ type: "navigate", path: page.path });
  }, [page]);
  const activateTab = (id: string) => {
    const tab = folderTabs.items.find((t) => t.id === id);
    if (!tab) return;
    tabAction({ type: "activate", id });
    setActivePane(0);
    navigate({ type: "folder", path: tab.path });
  };
  const openTab = (path: string) => {
    if (folderTabs.items.length >= 12) {
      notify(t("最多打开 12 个标签页"), true);
      return;
    }
    tabAction({ type: "open", path, id: crypto.randomUUID() });
    setActivePane(0);
    navigate({ type: "folder", path });
  };
  const closeTab = (id: string) => {
    const next = tabsReducer(folderTabs, { type: "close", id });
    if (next === folderTabs) return;
    tabAction({ type: "close", id });
    if (id === folderTabs.active)
      navigate({
        type: "folder",
        path: next.items.find((t) => t.id === next.active)!.path,
      });
  };

  const lastFolder = useRef(
    initialPage.type === "folder"
      ? initialPage.path
      : folderTabs.items.find((t) => t.id === folderTabs.active)!.path,
  );
  if (page.type === "folder") lastFolder.current = page.path;
  const pluginCatalog = useAsync(
    () =>
      standalone
        ? Promise.resolve({ plugins: [] } as unknown as PluginCatalog)
        : request<PluginCatalog>("plugins_list"),
    [],
  );

  const [past, setPast] = useState<Page[]>([]);
  const [future, setFuture] = useState<Page[]>([]);
  const [catalog, setCatalog] = useState<Catalog>({
    ...emptyCatalog,
    ...boot.catalog.data,
  });
  const catalogRef = useRef(catalog);
  const catalogRevision = useRef(boot.catalog.revision);
  const catalogQueue = useRef(Promise.resolve());
  const [settings, setSettings] = useState<Settings>({
    ...defaultSettings,
    ...boot.settings.data,
  });
  useEffect(() => setLanguage(settings.language), [settings.language]);
  const settingsRef = useRef(settings);
  const settingsRevision = useRef(boot.settings.revision);
  const settingsQueue = useRef(Promise.resolve());
  const [modal, setModal] = useState<ReactNode>(null);
  const [auxModal, setAuxModal] = useState<ReactNode>(null);
  const [toast, setToast] = useState<{
    message: string;
    error?: boolean;
  } | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [selection, setSelection] = useState<Entry[]>([]);
  const [preview, setPreview] = useState(true);
  const [sidebar, setSidebar] = useState(true);
  const [dual, setDual] = useState(Boolean(boot.session.data?.dual));
  const [rightPath, setRightPath] = useState(
    String(boot.session.data?.rightPath || boot.home),
  );
  const [activePane, setActivePane] = useState(0);
  const [split, setSplit] = useState(Number(boot.session.data?.split || 50));
  const [peek, setPeek] = useState<{ entry: Entry; entries: Entry[] } | null>(
    null,
  );
  const [switcher, setSwitcher] = useState(false);
  const [shelf, setShelf] = useState(false);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [showJobs, setShowJobs] = useState(false);
  const history = useAsync(
    () =>
      showJobs
        ? request<{ items: HistoryItem[] }>("history")
        : Promise.resolve({ items: [] }),
    [showJobs, refresh],
  );
  const jobsSeen = useRef(new Set<string>());
  const [git, setGit] = useState<{
    branch: string;
    count: number;
    changes: { name: string; status: string; path: string }[];
  } | null>(null);
  const [showGit, setShowGit] = useState(false);
  const notify = useCallback(
    (message: string, error = false) => setToast({ message, error }),
    [],
  );
  const refreshAll = useCallback(() => {
    clearPreviewCache();
    setRefresh((v) => v + 1);
    void pluginCatalog.refresh();
  }, [pluginCatalog.refresh]);
  const [reveal, setReveal] = useState<{
    path: string;
    parent: string;
    at: number;
  } | null>(null);
  const navigate = useCallback((next: Page, revealPath?: string) => {
    const proceed = () => {
      setPage((current) => {
        setPast((p) => [...p.slice(-39), current]);
        return next;
      });
      setFuture([]);
      setSelection([]);
      setReveal(
        next.type === "folder" && revealPath
          ? { path: revealPath, parent: next.path, at: Date.now() }
          : null,
      );
    };
    const event = new CustomEvent("filemanager-before-navigate", {
      cancelable: true,
      detail: { next, proceed },
    });
    if (window.dispatchEvent(event)) proceed();
  }, []);
  const updateCatalog = useCallback(
    (change: (value: Catalog) => Catalog): Promise<void> => {
      const task = catalogQueue.current
        .catch(() => {})
        .then(async () => {
          const next = change(catalogRef.current);
          const result = await request<Revision<Catalog>>("catalog_save", {
            namespace: "catalog",
            data: next,
            revision: catalogRevision.current,
          });
          catalogRef.current = next;
          catalogRevision.current = result.revision;
          setCatalog(next);
        });
      catalogQueue.current = task;
      return task.catch(async (e) => {
        const latest = await request<Revision<Catalog>>("catalog_get", {
          namespace: "catalog",
        }).catch(() => null);
        if (latest?.data) {
          catalogRevision.current = latest.revision;
          catalogRef.current = { ...emptyCatalog, ...latest.data };
          setCatalog(catalogRef.current);
        }
        notify(errorText(e), true);
        throw e;
      });
    },
    [notify],
  );
  const updateSettings = useCallback(
    (change: Partial<Settings>): Promise<void> => {
      const task = settingsQueue.current
        .catch(() => {})
        .then(async () => {
          const before = settingsRef.current;
          const next = { ...before, ...change };
          if (native) await desktop("configure", { settings: next });
          const result = await request<Revision<Settings>>("catalog_save", {
            namespace: "settings",
            data: next,
            revision: settingsRevision.current,
          }).catch(async (error) => {
            if (native)
              await desktop("configure", { settings: before }).catch(() => {});
            throw error;
          });
          settingsRevision.current = result.revision;
          settingsRef.current = next;
          setSettings(next);
        });
      settingsQueue.current = task;
      return task.catch((e) => {
        notify(errorText(e), true);
        throw e;
      });
    },
    [notify],
  );
  const prompt = useCallback(
    (title: string, initial = "", hint?: string) =>
      new Promise<string | null>((resolve) =>
        setAuxModal(
          <Prompt
            title={title}
            initial={initial}
            hint={hint}
            done={(value) => {
              setAuxModal(null);
              resolve(value);
            }}
          />,
        ),
      ),
    [],
  );
  const confirm = useCallback(
    (title: string, message: string, danger = false) =>
      new Promise<boolean>((resolve) =>
        setAuxModal(
          <Modal
            title={title}
            onClose={() => {
              setAuxModal(null);
              resolve(false);
            }}
            footer={
              <>
                <Button
                  onClick={() => {
                    setAuxModal(null);
                    resolve(false);
                  }}
                >
                  {t("取消")}
                </Button>
                <Button
                  tone={danger ? "danger" : "primary"}
                  onClick={() => {
                    setAuxModal(null);
                    resolve(true);
                  }}
                >
                  {t("确认")}
                </Button>
              </>
            }
          >
            <p className="confirmation-text">{message}</p>
          </Modal>,
        ),
      ),
    [],
  );
  const pickFiles = useCallback(
    (
      options: {
        directories?: boolean;
        multiple?: boolean;
        initial?: string;
      } = {},
    ) =>
      new Promise<string[]>((resolve) =>
        setAuxModal(
          <FilePicker
            {...options}
            done={(paths) => {
              setAuxModal(null);
              resolve(paths);
            }}
          />,
        ),
      ),
    [],
  );
  const saveText = useCallback(
    async (text: string) => {
      try {
        if (native) await desktop("clipboard_write_text", { text });
        else await navigator.clipboard.writeText(text);
        notify(t("已复制到剪贴板"));
      } catch (e) {
        notify(errorText(e), true);
      }
    },
    [notify],
  );
  const operate = useCallback(
    async (
      kind: string,
      paths: string[],
      destination?: string,
    ): Promise<OperationResult | null> => {
      if (!paths.length) return null;
      try {
        const args = { kind, paths, destination, conflict: "ask" };
        const result = await request<OperationResult>("operate_check", args);
        if (result.requires_confirmation) {
          const resolutions = await new Promise<Record<string, string> | null>(
            (resolve) =>
              setModal(
                <ConflictDialog
                  conflicts={result.conflicts || []}
                  done={(value) => {
                    setModal(null);
                    resolve(value);
                  }}
                />,
              ),
          );
          if (!resolutions) return null;
          const replacement_versions = Object.fromEntries(
            (result.conflicts ?? [])
              .filter((item) => item.replacement_version)
              .map((item) => [item.source.path, item.replacement_version]),
          );
          Object.assign(args, { resolutions, replacement_versions });
        }
        await request("job_start", { op: "operate", args });
        setShowJobs(true);
        return { count: paths.length };
      } catch (e) {
        notify(errorText(e), true);
        refreshAll();
        return null;
      }
    },
    [notify, refreshAll],
  );
  function open(entry: Entry) {
    if (isFolder(entry)) navigate({ type: "folder", path: entry.path });
    else if (entry.kind === "markdown")
      navigate({ type: "markdown", path: entry.path, from: page });
    else
      void desktop("open", { path: entry.path }).catch((e) =>
        notify(errorText(e), true),
      );
  }
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme =
        settings.theme === "system"
          ? media.matches
            ? "dark"
            : "light"
          : settings.theme;
      applySkin(
        settings.skin,
        document.documentElement.dataset.theme as "light" | "dark",
        settings.accent,
      );
      document.documentElement.dataset.reduceMotion = String(
        settings.reduceMotion,
      );
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [settings.theme, settings.accent, settings.skin, settings.reduceMotion]);
  useEffect(() => {
    const localValue = window.setTimeout(
      () => setToast(null),
      toast?.error ? 10000 : 3500,
    );
    return () => clearTimeout(localValue);
  }, [toast]);
  useEffect(() => {
    if (standalone) return;
    const localValue = setTimeout(
      () =>
        void request("catalog_save", {
          namespace: "session",
          data: {
            page,
            dual,
            rightPath,
            split,
            folderTabs: folderTabs.items,
            activeTab: folderTabs.active,
          },
        }).catch(() => {}),
      900,
    );
    return () => clearTimeout(localValue);
  }, [page, dual, rightPath, split, standalone, folderTabs]);
  useEffect(() => {
    const poll = async () => {
      if (document.hidden) return;
      try {
        const next = await request<Job[]>("jobs");
        setJobs((current) =>
          JSON.stringify(current) === JSON.stringify(next) ? current : next,
        );
        const ids = new Set(next.map((j) => j.id));
        for (const id of jobsSeen.current)
          if (!ids.has(id)) jobsSeen.current.delete(id);
        for (const job of next)
          if (
            !jobsSeen.current.has(job.id) &&
            ["done", "failed", "cancelled"].includes(job.status)
          ) {
            jobsSeen.current.add(job.id);
            refreshAll();
            if (job.error) notify(job.error, job.status === "failed");
            else if (
              (job.result as OperationResult | undefined)?.requires_confirmation
            )
              notify(t("目标出现了新的同名项目，请重试并确认"), true);
          }
      } catch {}
    };
    void poll();
    const localValue = setInterval(() => void poll(), 2000);
    return () => clearInterval(localValue);
  }, [refreshAll, notify]);
  useEffect(() => {
    if (page.type !== "folder") {
      setGit(null);
      return;
    }
    let alive = true;
    void request<any>("git_status", {
      path: activePane === 1 && dual ? rightPath : page.path,
    })
      .then((v) => {
        if (alive) setGit(v.repository ? v : null);
      })
      .catch(() => setGit(null));
    return () => {
      alive = false;
    };
  }, [page, refresh, activePane, rightPath, dual]);
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (
        (e.target as HTMLElement).closest(
          'input,textarea,[contenteditable="true"]',
        )
      )
        return;
      const ctrl = e.ctrlKey || e.metaKey;
      if (document.querySelector(".modal,.peek-backdrop")) return;
      if (
        page.type === "folder" &&
        ctrl &&
        ["t", "w", "Tab"].includes(
          e.key.toLowerCase() === "tab" ? "Tab" : e.key.toLowerCase(),
        )
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (e.key.toLowerCase() === "t") openTab(page.path);
        else if (e.key.toLowerCase() === "w") closeTab(folderTabs.active);
        else {
          const index = folderTabs.items.findIndex(
            (t) => t.id === folderTabs.active,
          );
          activateTab(
            folderTabs.items[
              (index + (e.shiftKey ? -1 : 1) + folderTabs.items.length) %
                folderTabs.items.length
            ].id,
          );
        }
        return;
      }

      const shortcuts = { ...shortcutDefaults, ...settings.shortcuts };
      const pressed = keyString(e);
      const action = Object.keys(shortcuts).find(
        (key) => shortcuts[key] === pressed,
      );
      if (action) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (action === "search") setSwitcher((v) => !v);
        else if (action === "dual") setDual((v) => !v);
        else if (action === "undo" && page.type !== "folder") {
          void request<{ conflicts: string[] }>("undo")
            .then(async (result) => {
              const latest = await request<Revision<Catalog>>("catalog_get");
              catalogRef.current = { ...emptyCatalog, ...latest.data };
              catalogRevision.current = latest.revision;
              setCatalog(catalogRef.current);
              refreshAll();
              notify(
                result.conflicts.length
                  ? result.conflicts.join("；")
                  : t("已撤销"),
                !!result.conflicts.length,
              );
            })
            .catch((e) => notify(errorText(e), true));
        } else command(action);
        return;
      }
      if (ctrl && e.altKey && e.code === "Space" && !native) {
        e.preventDefault();
        setShelf((v) => !v);
      }
      if (page.type === "folder" && dual && e.key === "Tab" && !modal) {
        e.preventDefault();
        setActivePane((v) => 1 - v);
      }
      if (
        page.type === "folder" &&
        dual &&
        (e.key === "F5" || e.key === "F6")
      ) {
        e.preventDefault();
        void operate(
          e.key === "F5" ? "copy" : "move",
          selection.map((e) => e.path),
          activePane === 0 ? rightPath : page.path,
        );
      }
    };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [
    page,
    dual,
    modal,
    selection,
    activePane,
    rightPath,
    operate,
    settings.shortcuts,
    folderTabs,
  ]);
  useEffect(() => {
    if (!native) return;
    let alive = true;
    const unlisten: (() => void)[] = [];
    const reload = async () => {
      const [c, s] = await Promise.all([
        request<Revision<Catalog>>("catalog_get", { namespace: "catalog" }),
        request<Revision<Settings>>("catalog_get", { namespace: "settings" }),
      ]);
      if (!alive) return;
      if (c.revision > catalogRevision.current) {
        catalogRef.current = { ...emptyCatalog, ...c.data };
        catalogRevision.current = c.revision;
        setCatalog(catalogRef.current);
      }
      if (s.revision > settingsRevision.current) {
        settingsRef.current = { ...defaultSettings, ...s.data };
        settingsRevision.current = s.revision;
        setSettings(settingsRef.current);
      }
      refreshAll();
    };
    const route = async (value: any) => {
      if (!value || standalone) return;
      if (value.path && value.select) {
        setActivePane(0);
        navigate({ type: "folder", path: value.path }, value.select);
      } else if (value.path) {
        const entry = await request<Entry>("file_info", { path: value.path });
        if (isFolder(entry)) navigate({ type: "folder", path: entry.path });
        else if (entry.kind === "markdown")
          navigate({ type: "markdown", path: entry.path });
        else await desktop("open", { path: entry.path });
      } else if (value.page) navigate(value.page);
      else if (value.action === "new-workspace")
        setModal(<WorkspaceEditor close={() => setModal(null)} />);
      else if (value.action === "bookmark")
        setModal(
          <BookmarkEditor
            initialUrl={value.url}
            close={() => setModal(null)}
          />,
        );
    };
    void (async () => {
      const listeners = await Promise.all([
        listen<any>("filemanager-route", (e) => {
          void desktop("initial_route");
          void route(e.payload).catch((e) => notify(errorText(e), true));
        }),
        listen("filemanager-data-changed", () => {
          void reload().catch(() => {});
        }),
        listen("filemanager-files-changed", () => refreshAll()),
        listen("filemanager-drag-ended", () => {
          setTimeout(() => (dragState.paths = []), 500);
        }),
      ]);
      if (alive) unlisten.push(...listeners);
      else listeners.forEach((f) => f());
      if (!standalone && alive) {
        await route(await desktop("initial_route"));
        const warnings = await desktop<string[]>("startup_warnings");
        if (warnings.length && alive) notify(warnings.join("；"), true);
      }
    })().catch((e) => notify(errorText(e), true));
    return () => {
      alive = false;
      unlisten.forEach((f) => f());
    };
  }, [navigate, refreshAll, notify, standalone]);
  useEffect(() => {
    if (!native || standalone) return;
    let off: (() => void) | undefined;
    let alive = true;
    void getCurrentWindow()
      .scaleFactor()
      .then((scale) =>
        getCurrentWindow().onDragDropEvent((event) => {
          if (event.payload.type !== "drop" || modal || auxModal) return;
          const target = document.elementFromPoint(
            event.payload.position.x / scale,
            event.payload.position.y / scale,
          );
          if (page.type === "folder") {
            const destination =
              target?.closest<HTMLElement>("[data-drop-path]")?.dataset
                .dropPath || (activePane === 1 && dual ? rightPath : page.path);
            const internal =
              dragState.paths.length &&
              event.payload.paths.every((p) => dragState.paths.includes(p));
            void operate(
              internal ? dragState.kind : "copy",
              event.payload.paths,
              destination,
            );
          } else if (target) {
            const transfer = new DataTransfer();
            transfer.setData(
              "application/x-filemanager-paths",
              JSON.stringify(event.payload.paths),
            );
            target.dispatchEvent(
              new DragEvent("drop", {
                bubbles: true,
                cancelable: true,
                dataTransfer: transfer,
              }),
            );
          }
        }),
      )
      .then((fn) => {
        if (alive) off = fn;
        else fn();
      });
    return () => {
      alive = false;
      off?.();
    };
  }, [page, dual, activePane, rightPath, operate, standalone, modal, auxModal]);
  useEffect(() => {
    if (!native || standalone) return;
    void desktop("watch_directories", {
      paths:
        page.type === "folder"
          ? dual
            ? [page.path, rightPath]
            : [page.path]
          : [],
    }).catch(() => {});
  }, [page, dual, rightPath, standalone]);
  const context: AppContextValue = {
    boot,
    catalog,
    settings,
    page,
    plugins: pluginCatalog.data?.plugins || [],
    runPlugin: (id, selected, directory) => {
      const plugin = pluginCatalog.data?.plugins.find(
        (p) => p.manifest.id === id && p.enabled,
      );
      if (!plugin) {
        notify(t("插件未安装或已停用"), true);
        return;
      }
      setModal(
        <PluginRunner
          plugin={plugin}
          directory={
            directory ||
            (page.type === "folder" && activePane === 1 && dual
              ? rightPath
              : lastFolder.current)
          }
          selection={
            selected ||
            (page.type === "folder" ? selection.map((e) => e.path) : [])
          }
          close={() => setModal(null)}
        />,
      );
    },
    openTab,
    navigate,
    updateCatalog,
    updateSettings,
    notify,
    prompt,
    confirm,
    pickFiles,
    showModal: setModal,
    refresh,
    refreshAll,
    open,
    peek: (entry, entries = [entry]) => setPeek({ entry, entries }),
    operate,
    saveText,
  };
  const title =
    page.type === "folder"
      ? basename(activePane === 1 && dual ? rightPath : page.path)
      : page.type === "workspace"
        ? catalog.workspaces.find((w) => w.id === page.id)?.name ||
          t("工作空间")
        : page.type === "bookmarks"
          ? catalog.collections.find((c) => c.id === page.id)?.name ||
            (page.favourites ? t("收藏书签") : t("书签"))
          : {
              clean: t("智能清理"),
              packages: t("软件包"),
              plugins: t("插件中心"),
              about: t("关于 FileManager"),
              agents: "AI Agents",
              system: t("关于本机"),
              settings: t("设置"),
              trash: t("回收站"),
              markdown: basename(page.type === "markdown" ? page.path : ""),
            }[page.type];
  const running = jobs.filter(
    (j) => j.status === "running" || j.status === "queued",
  );
  if (new URLSearchParams(location.search).get("window") === "shelf")
    return (
      <AppContext.Provider value={context}>
        <Shelf close={() => void getCurrentWindow().hide()} standalone />
        {modal}
        {auxModal}
        {toast && (
          <div className={`toast ${toast.error ? "error" : ""}`}>
            {toast.message}
          </div>
        )}
      </AppContext.Provider>
    );
  return (
    <AppContext.Provider value={context}>
      <div className={`app-shell ${!sidebar ? "sidebar-hidden" : ""}`}>
        {sidebar && (
          <Sidebar
            quickSearch={() => setSwitcher(true)}
            createWorkspace={() =>
              setModal(<WorkspaceEditor close={() => setModal(null)} />)
            }
          />
        )}
        <main className="main-shell">
          <header className="app-toolbar" data-tauri-drag-region>
            <IconButton
              label={sidebar ? t("收起侧栏") : t("显示侧栏")}
              onClick={() => setSidebar((v) => !v)}
            >
              {sidebar ? (
                <PanelLeftClose size={16} />
              ) : (
                <PanelLeftOpen size={16} />
              )}
            </IconButton>
            <div className="history-buttons">
              <IconButton
                label={t("后退")}
                disabled={!past.length}
                onClick={() => {
                  const previous = past.at(-1);
                  if (previous) {
                    const proceed = () => {
                      setFuture((v) => [page, ...v]);
                      setPast((v) => v.slice(0, -1));
                      setPage(previous);
                    };
                    if (
                      window.dispatchEvent(
                        new CustomEvent("filemanager-before-navigate", {
                          cancelable: true,
                          detail: { next: previous, proceed },
                        }),
                      )
                    )
                      proceed();
                  }
                }}
              >
                <ChevronLeft size={18} />
              </IconButton>
              <IconButton
                label={t("前进")}
                disabled={!future.length}
                onClick={() => {
                  if (future[0]) {
                    const next = future[0];
                    const proceed = () => {
                      setPast((v) => [...v, page]);
                      setPage(next);
                      setFuture((v) => v.slice(1));
                    };
                    if (
                      window.dispatchEvent(
                        new CustomEvent("filemanager-before-navigate", {
                          cancelable: true,
                          detail: { next, proceed },
                        }),
                      )
                    )
                      proceed();
                  }
                }}
              >
                <ChevronRight size={18} />
              </IconButton>
            </div>
            <h1>{title}</h1>
            {page.type === "folder" && git && (
              <button
                className="git-status"
                title={t("Git 状态，只读")}
                onClick={() => setShowGit((v) => !v)}
              >
                <span>⑂</span>
                {git.branch}
                <b>{git.count}</b>
              </button>
            )}
            <div className="flex-1" />
            {page.type === "folder" && (
              <>
                <IconButton
                  label={t("打开终端")}
                  onClick={() =>
                    void desktop("terminal", {
                      path: activePane === 1 && dual ? rightPath : page.path,
                      app: settings.terminal,
                    }).catch((e) => notify(String(e), true))
                  }
                >
                  <Terminal size={16} />
                </IconButton>
                <IconButton
                  label={`${t("切换双栏")} (${settings.shortcuts.dual || shortcutDefaults.dual})`}
                  active={dual}
                  onClick={() => setDual((v) => !v)}
                >
                  <PanelTop size={16} className="rotate-90" />
                </IconButton>
                {dual && (
                  <IconButton
                    label={t("交换左右目录")}
                    onClick={() => {
                      const left = page.path;
                      setPage({ type: "folder", path: rightPath });
                      setRightPath(left);
                    }}
                  >
                    <ArrowLeftRight size={16} />
                  </IconButton>
                )}
              </>
            )}
            <IconButton
              label="Drop Shelf (Ctrl Alt Space)"
              active={shelf}
              onClick={() => {
                if (native) void desktop("shelf_toggle");
                else setShelf((v) => !v);
              }}
            >
              <Layers size={17} />
            </IconButton>
            <IconButton
              label={t("撤销 (Ctrl Z)")}
              onClick={async () => {
                try {
                  const r = await request<{
                    conflicts: string[];
                    count: number;
                    message?: string;
                  }>("undo");
                  const c = await request<Revision<Catalog>>("catalog_get", {
                    namespace: "catalog",
                  });
                  if (c.data) {
                    catalogRef.current = { ...emptyCatalog, ...c.data };
                    catalogRevision.current = c.revision;
                    setCatalog(catalogRef.current);
                  }
                  refreshAll();
                  notify(
                    r.conflicts.length
                      ? r.conflicts.join("；")
                      : r.message || t("已撤销 {0} 项", r.count),
                    !!r.conflicts.length,
                  );
                } catch (e) {
                  notify(errorText(e), true);
                }
              }}
            >
              <RotateCcw size={16} />
            </IconButton>
            <IconButton
              label={t("快速切换 (Ctrl K)")}
              onClick={() => setSwitcher(true)}
            >
              <Search size={17} />
            </IconButton>
            {native && (
              <div className="window-controls">
                <IconButton
                  label={t("最小化")}
                  onClick={() => void getCurrentWindow().minimize()}
                >
                  <Minus size={14} />
                </IconButton>
                <IconButton
                  label={t("最大化")}
                  onClick={() => void getCurrentWindow().toggleMaximize()}
                >
                  <Maximize2 size={12} />
                </IconButton>
                <IconButton
                  label={t("关闭窗口")}
                  onClick={() => void getCurrentWindow().close()}
                >
                  <X size={15} />
                </IconButton>
              </div>
            )}
          </header>
          {page.type === "folder" && (
            <div
              className="folder-tabs"
              role="tablist"
              aria-label={t("目录标签页")}
            >
              {folderTabs.items.map((tab) => (
                <div
                  key={tab.id}
                  className={`folder-tab ${folderTabs.active === tab.id ? "active" : ""}`}
                >
                  <button
                    role="tab"
                    aria-selected={folderTabs.active === tab.id}
                    title={tab.path}
                    onClick={() => activateTab(tab.id)}
                    onKeyDown={(e) => {
                      if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
                        e.preventDefault();
                        const index = folderTabs.items.findIndex(
                          (t) => t.id === tab.id,
                        );
                        activateTab(
                          folderTabs.items[
                            (index +
                              (e.key === "ArrowRight" ? 1 : -1) +
                              folderTabs.items.length) %
                              folderTabs.items.length
                          ].id,
                        );
                      }
                    }}
                  >
                    <Folder size={13} />
                    <span>{basename(tab.path) || "/"}</span>
                  </button>
                  <IconButton
                    label={t("关闭标签页 {0}", basename(tab.path))}
                    disabled={folderTabs.items.length === 1}
                    onClick={() => closeTab(tab.id)}
                  >
                    <X size={11} />
                  </IconButton>
                </div>
              ))}
              <IconButton
                label={t("新标签页 (Ctrl T)")}
                onClick={() => openTab(page.path)}
              >
                <Plus size={15} />
              </IconButton>
            </div>
          )}
          <div className="main-content">
            {page.type === "folder" ? (
              <>
                <div className="browsers">
                  <div
                    style={{ width: dual ? `${split}%` : "100%" }}
                    className="left-browser"
                  >
                    <FileBrowser
                      path={page.path}
                      onNavigate={(path) => navigate({ type: "folder", path })}
                      active={activePane === 0 || !dual}
                      onActivate={() => setActivePane(0)}
                      onSelect={setSelection}
                      reveal={reveal?.parent === page.path ? reveal : null}
                      showPreview={preview}
                      togglePreview={() => setPreview((v) => !v)}
                    />
                  </div>
                  {dual && (
                    <>
                      <div
                        className="pane-divider"
                        role="separator"
                        aria-label={t("调整双栏宽度")}
                        onPointerDown={(e) => {
                          e.preventDefault();
                          const parent =
                            e.currentTarget.parentElement!.getBoundingClientRect();
                          const move = (e: PointerEvent) =>
                            setSplit(
                              Math.min(
                                75,
                                Math.max(
                                  25,
                                  ((e.clientX - parent.left) / parent.width) *
                                    100,
                                ),
                              ),
                            );
                          const up = () => {
                            window.removeEventListener("pointermove", move);
                            window.removeEventListener("pointerup", up);
                          };
                          window.addEventListener("pointermove", move);
                          window.addEventListener("pointerup", up);
                        }}
                      />
                      <div className="right-browser">
                        <FileBrowser
                          path={rightPath}
                          onNavigate={setRightPath}
                          active={activePane === 1}
                          onActivate={() => setActivePane(1)}
                          onSelect={setSelection}
                          showPreview={preview}
                          togglePreview={() => setPreview((v) => !v)}
                        />
                      </div>
                    </>
                  )}
                </div>
                {preview && (
                  <PreviewPane
                    entry={selection[0]}
                    close={() => setPreview(false)}
                  />
                )}
              </>
            ) : page.type === "workspace" ? (
              <Workspaces id={page.id} />
            ) : page.type === "bookmarks" ? (
              <Bookmarks page={page} />
            ) : page.type === "clean" ? (
              <Clean
                key={`${page.scope ?? ""}:${page.category ?? ""}`}
                scope={page.scope}
                initialCategory={page.category}
              />
            ) : page.type === "packages" ? (
              <Packages />
            ) : page.type === "plugins" ? (
              <PluginsPage />
            ) : page.type === "about" ? (
              <AboutPage />
            ) : page.type === "agents" ? (
              <Agents />
            ) : page.type === "system" ? (
              <SystemPage />
            ) : page.type === "settings" ? (
              <SettingsPage />
            ) : page.type === "trash" ? (
              <TrashPage />
            ) : (
              <MarkdownEditor
                path={page.path}
                back={
                  page.from || {
                    type: "folder",
                    path: page.path.slice(0, page.path.lastIndexOf("/")),
                  }
                }
              />
            )}
          </div>
          <footer className="app-status">
            <span className="status-dot" />
            <span>{t("本地文件")}</span>
            <span className="status-divider" />
            <span>FileManager {boot.version}</span>
            <div className="flex-1" />
            <button onClick={() => setShowJobs((v) => !v)}>
              {running.length ? (
                <LoaderCircle size={12} className="spin" />
              ) : (
                <Check size={12} />
              )}
              {running.length
                ? t("{0} 个任务", running.length)
                : t("所有操作已同步")}
            </button>
            {page.type === "folder" && dual && (
              <span className="status-tip">{t("F5 复制 · F6 移动")}</span>
            )}
          </footer>
        </main>
        {showJobs && (
          <div className="jobs-popover">
            <header>
              <strong>{t("文件操作")}</strong>
              <IconButton
                label={t("关闭任务")}
                onClick={() => setShowJobs(false)}
              >
                <X size={14} />
              </IconButton>
            </header>
            {jobs.length ? (
              jobs
                .slice(-8)
                .reverse()
                .map((job) => (
                  <div className="job-item" key={job.id}>
                    <div>
                      <strong>
                        {(
                          {
                            operate: t("文件操作"),
                            clean_scan: t("扫描存储"),
                            tidy_apply: t("整理文件"),
                            packages: t("读取软件包"),
                          } as Record<string, string>
                        )[job.op] || t("后台任务")}
                      </strong>
                      <span>
                        {
                          {
                            queued: t("等待中"),
                            running: t("进行中"),
                            done: t("已完成"),
                            failed: t("失败"),
                            cancelled: t("已取消"),
                          }[job.status]
                        }
                      </span>
                      {["running", "queued"].includes(job.status) && (
                        <IconButton
                          label={t("取消任务")}
                          onClick={() =>
                            void request("job_cancel", { id: job.id })
                          }
                        >
                          <X size={13} />
                        </IconButton>
                      )}
                    </div>
                    <p>
                      {job.error || job.current || t("等待操作…")}
                      {job.status === "running" && job.bytes > 0
                        ? ` · ${formatBytes(job.bytes)}`
                        : ""}
                    </p>
                    {job.status === "running" && (
                      <div className="job-progress">
                        <i
                          style={{
                            width: job.total
                              ? `${(job.completed / job.total) * 100}%`
                              : "40%",
                          }}
                        />
                      </div>
                    )}
                  </div>
                ))
            ) : (
              <p className="muted">{t("暂无后台任务")}</p>
            )}
            {history.data?.items.some((item) =>
              ["running", "partial", "deferred"].includes(item.status),
            ) && (
              <div className="jobs-recovery">
                <strong>{t("待恢复的操作")}</strong>
                {history.data.items
                  .filter((item) =>
                    ["running", "partial", "deferred"].includes(item.status),
                  )
                  .map((item) => (
                    <div className="job-item" key={item.id}>
                      <p>
                        {t(item.label)} ·{" "}
                        {new Date(item.created).toLocaleString()}
                      </p>
                      <div>
                        <Button
                          disabled={running.length > 0}
                          onClick={async () => {
                            try {
                              const result = await request<{
                                conflicts: string[];
                              }>("undo", { id: item.id });
                              notify(
                                result.conflicts.join("；") || t("已撤销"),
                                result.conflicts.length > 0,
                              );
                              refreshAll();
                            } catch (e) {
                              notify(errorText(e), true);
                            }
                          }}
                        >
                          {t("恢复")}
                        </Button>
                        {item.status === "partial" && (
                          <Button
                            onClick={async () => {
                              try {
                                await request("defer_undo", { id: item.id });
                                refreshAll();
                              } catch (e) {
                                notify(errorText(e), true);
                              }
                            }}
                          >
                            {t("暂缓并保留记录")}
                          </Button>
                        )}
                      </div>
                    </div>
                  ))}
              </div>
            )}
          </div>
        )}
        {showGit && git && (
          <div className="git-popover">
            <header>
              <strong>{git.branch}</strong>
              <IconButton
                label={t("关闭 Git 状态")}
                onClick={() => setShowGit(false)}
              >
                <X size={14} />
              </IconButton>
            </header>
            {git.changes.length ? (
              git.changes.map((c) => (
                <button
                  key={c.path}
                  onClick={() => {
                    setShowGit(false);
                    navigate(
                      {
                        type: "folder",
                        path: c.path.slice(0, c.path.lastIndexOf("/")),
                      },
                      c.path,
                    );
                  }}
                >
                  <code>{c.status}</code>
                  <span>{c.name}</span>
                </button>
              ))
            ) : (
              <p className="muted">{t("工作目录干净")}</p>
            )}
          </div>
        )}
        {shelf && <Shelf close={() => setShelf(false)} />}
        {switcher && <QuickSwitcher close={() => setSwitcher(false)} />}
        {modal}
        {auxModal}
        {peek && (
          <Peek
            {...peek}
            close={() => setPeek(null)}
            change={(entry) => setPeek((p) => p && { ...p, entry })}
          />
        )}
        {toast && (
          <div role="status" className={`toast ${toast.error ? "error" : ""}`}>
            {toast.error ? <AlertTriangle size={16} /> : <Check size={16} />}
            <span>{toast.message}</span>
            <IconButton label={t("关闭提示")} onClick={() => setToast(null)}>
              <X size={13} />
            </IconButton>
          </div>
        )}
      </div>
    </AppContext.Provider>
  );
}
function Prompt({
  title,
  initial,
  hint,
  done,
}: {
  title: string;
  initial: string;
  hint?: string;
  done: (v: string | null) => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <Modal
      title={title}
      onClose={() => done(null)}
      footer={
        <>
          <Button onClick={() => done(null)}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!value.trim()}
            onClick={() => done(value)}
          >
            {t("确认")}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (value.trim()) done(value);
        }}
      >
        <input
          className="prompt-input"
          autoFocus
          value={value}
          onFocus={(e) => e.target.select()}
          onChange={(e) => setValue(e.target.value)}
        />
        {hint && <p className="muted">{hint}</p>}
      </form>
    </Modal>
  );
}
function QuickSwitcher({ close }: { close: () => void }) {
  const { navigate, catalog, boot, page, open } = useContextApp();
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState<Entry[]>([]);
  useEffect(() => {
    let alive = true;
    if (query.length < 2) {
      setSearch([]);
      return;
    }
    const localValue = setTimeout(
      () =>
        void request<{ entries: Entry[] }>("search", {
          query,
          roots:
            page.type === "workspace"
              ? catalog.workspaces.find((w) => w.id === page.id)?.paths || []
              : page.type === "folder"
                ? [page.path]
                : [boot.home],
          limit: 40,
        })
          .then((v) => {
            if (alive) setSearch(v.entries);
          })
          .catch(() => {}),
      300,
    );
    return () => {
      alive = false;
      clearTimeout(localValue);
    };
  }, [query]);
  const q = query.toLowerCase();
  return (
    <Modal title={t("快速切换")} onClose={close}>
      <div className="switcher-search">
        <Search size={18} />
        <input
          autoFocus
          placeholder={t("搜索位置、工作空间、书签或文件…")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <kbd>Esc</kbd>
      </div>
      <div className="switcher-results">
        {boot.places
          .filter((p) => p.name.toLowerCase().includes(q))
          .map((p) => (
            <button
              key={p.id}
              onClick={() => {
                close();
                navigate({ type: "folder", path: p.path });
              }}
            >
              <Folder size={17} />
              <span>{t(p.name)}</span>
              <small>{p.path}</small>
            </button>
          ))}
        {catalog.workspaces
          .filter((w) => w.name.toLowerCase().includes(q))
          .map((w) => (
            <button
              key={w.id}
              onClick={() => {
                close();
                navigate({ type: "workspace", id: w.id });
              }}
            >
              <Layers size={17} />
              <span>{w.name}</span>
              <small>{t("工作空间")}</small>
            </button>
          ))}
        {catalog.bookmarks
          .filter((b) => `${b.title} ${b.url}`.toLowerCase().includes(q))
          .slice(0, 15)
          .map((b) => (
            <button
              key={b.id}
              onClick={() => {
                close();
                void desktop("open_url", { url: b.url });
              }}
            >
              <ArrowLeftRight size={17} />
              <span>{b.title}</span>
              <small>{b.site}</small>
            </button>
          ))}
        {search.map((e) => (
          <button
            key={e.path}
            onClick={() => {
              close();
              open(e);
            }}
          >
            <FileIcon entry={e} />
            <span>{e.name}</span>
            <small>{e.path}</small>
          </button>
        ))}
      </div>
    </Modal>
  );
}
import { useApp as useContextApp } from "./context";
