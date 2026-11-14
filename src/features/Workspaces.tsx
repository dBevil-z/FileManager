import { t } from "../i18n";
import { useMemo, useState, type CSSProperties } from "react";
import {
  Archive,
  ArrowLeft,
  ArrowUpRight,
  Check,
  Clock3,
  FolderOpen,
  Grid2X2,
  Layers,
  Link2,
  List,
  MoreHorizontal,
  Pin,
  Plus,
  Search,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useApp } from "../context";
import { request, desktop } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Entry, Workspace } from "../types";
import { basename, formatBytes, isFolder, shortDate } from "../lib/files";
import { FileIcon, GlassFolder } from "../components/FileIcon";
import {
  Badge,
  Button,
  Empty,
  ErrorState,
  Field,
  IconButton,
  Loading,
  Menu,
  Modal,
  Select,
} from "../components/ui";
const colors = [
  "#9c7fea",
  "#7d9be4",
  "#6ebaa9",
  "#dbb56e",
  "#d88ba8",
  "#a3a8b7",
];
export function Workspaces({ id }: { id: string }) {
  const app = useApp();
  const [query, setQuery] = useState("");
  const [archived, setArchived] = useState(false);
  const [view, setView] = useState("cards");
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    workspace: Workspace;
  } | null>(null);
  const workspace = app.catalog.workspaces.find((w) => w.id === id);
  const list = app.catalog.workspaces.filter(
    (w) =>
      Boolean(w.archived) === archived &&
      `${w.name} ${w.description}`.toLowerCase().includes(query.toLowerCase()),
  );
  if (id !== "all" && workspace)
    return <WorkspaceDetail workspace={workspace} />;
  return (
    <div className="feature-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>{t("工作空间")}</h2>
          <p>{t("让文件、文件夹和链接，围绕你的工作聚在一起。")}</p>
        </div>
        <Button
          tone="primary"
          onClick={() =>
            app.showModal(<WorkspaceEditor close={() => app.showModal(null)} />)
          }
        >
          <Plus size={14} />
          {t("新建工作空间")}
        </Button>
      </div>
      <div className="page-tabs">
        <button
          className={!archived ? "active" : ""}
          onClick={() => setArchived(false)}
        >
          {t("所有工作空间")}
          <span>
            {app.catalog.workspaces.filter((w) => !w.archived).length}
          </span>
        </button>
        <button
          className={archived ? "active" : ""}
          onClick={() => setArchived(true)}
        >
          {t("已归档")}
          <span>{app.catalog.workspaces.filter((w) => w.archived).length}</span>
        </button>
      </div>
      <div className="page-toolbar">
        <div className="search-input">
          <Search size={14} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("搜索工作空间…")}
          />
        </div>
        <div className="flex-1" />
        <Select
          value={view}
          onChange={(e) => setView(e.target.value)}
          aria-label={t("工作空间视图")}
        >
          <option value="cards">{t("卡片")}</option>
          <option value="tiles">{t("小卡片")}</option>
          <option value="list">{t("列表")}</option>
        </Select>
      </div>
      {!list.length && query ? (
        <Empty icon={<Search size={28} />} title={t("没有找到工作空间")}>
          {t("试试其他名称。")}
        </Empty>
      ) : (
        <div
          className={`workspace-grid ${view === "list" ? "workspace-list-cards" : ""}`}
          style={
            view === "tiles"
              ? { gridTemplateColumns: "repeat(auto-fill,minmax(210px,1fr))" }
              : undefined
          }
        >
          {list.map((w) => (
            <article
              key={w.id}
              className="workspace-card"
              style={{ "--workspace-color": w.color } as CSSProperties}
              onDoubleClick={() =>
                app.navigate({ type: "workspace", id: w.id })
              }
            >
              <div
                className="workspace-cover"
                onClick={() => app.navigate({ type: "workspace", id: w.id })}
              >
                <GlassFolder name={w.name} color={w.color} symbol="work" />
                <div className="workspace-paper">
                  <i />
                  <i />
                  <i />
                  <i />
                  <i />
                </div>
              </div>
              <div
                className="workspace-content"
                onClick={() => app.navigate({ type: "workspace", id: w.id })}
              >
                <h3>
                  <span
                    className="workspace-dot"
                    style={{ background: w.color }}
                  />
                  {w.name}
                </h3>
                <p>{w.description || t("为你的下一件事留出空间")}</p>
                <footer>
                  <FolderOpen size={11} />
                  {w.paths.length}
                  {t(" 个位置")}
                  <span>·</span>
                  <Link2 size={11} />
                  {w.links.length}
                  {t(" 个链接")}
                  <div className="flex-1" />
                  <ArrowUpRight size={12} />
                </footer>
              </div>
              <IconButton
                className="workspace-menu"
                label={t("{0} 的选项", w.name)}
                onClick={(e) =>
                  setMenu({ x: e.clientX, y: e.clientY, workspace: w })
                }
              >
                <MoreHorizontal size={16} />
              </IconButton>
            </article>
          ))}
          {!archived && (
            <button
              className="workspace-create"
              onClick={() =>
                app.showModal(
                  <WorkspaceEditor close={() => app.showModal(null)} />,
                )
              }
            >
              <Plus size={23} strokeWidth={1.4} />
              <span>{t("新建工作空间")}</span>
            </button>
          )}
        </div>
      )}
      {menu && (
        <Menu
          x={menu.x}
          y={menu.y}
          close={() => setMenu(null)}
          items={[
            {
              label: t("编辑工作空间…"),
              action: () =>
                app.showModal(
                  <WorkspaceEditor
                    workspace={menu.workspace}
                    close={() => app.showModal(null)}
                  />,
                ),
            },
            {
              label: menu.workspace.archived
                ? t("恢复工作空间")
                : t("归档工作空间"),
              icon: <Archive size={14} />,
              action: () =>
                void app.updateCatalog((v) => ({
                  ...v,
                  workspaces: v.workspaces.map((w) =>
                    w.id === menu.workspace.id
                      ? { ...w, archived: !w.archived }
                      : w,
                  ),
                })),
            },
            { separator: true, label: "" },
            {
              label: t("移除工作空间"),
              icon: <Trash2 size={14} />,
              danger: true,
              action: async () => {
                if (
                  await app.confirm(
                    t("移除工作空间？"),
                    t("此操作只移除工作空间中的引用，实际文件仍在原来的位置。"),
                  )
                )
                  await app.updateCatalog((v) => ({
                    ...v,
                    workspaces: v.workspaces.filter(
                      (w) => w.id !== menu.workspace.id,
                    ),
                  }));
              },
            },
          ]}
        />
      )}
    </div>
  );
}
export function WorkspaceEditor({
  workspace,
  close,
}: {
  workspace?: Workspace;
  close: () => void;
}) {
  const app = useApp();
  const [name, setName] = useState(workspace?.name || "");
  const [description, setDescription] = useState(workspace?.description || "");
  const [color, setColor] = useState(workspace?.color || colors[0]);
  const [paths, setPaths] = useState<string[]>(workspace?.paths || []);
  const [links, setLinks] = useState<string[]>(workspace?.links || []);
  const [busy, setBusy] = useState(false);
  async function save() {
    if (!name.trim()) return;
    setBusy(true);
    try {
      const w: Workspace = {
        ...workspace,
        id: workspace?.id || crypto.randomUUID(),
        name: name.trim(),
        description,
        color,
        paths,
        links,
        pinned: workspace?.pinned || [],
      };
      await app.updateCatalog((v) => ({
        ...v,
        workspaces: workspace
          ? v.workspaces.map((x) => (x.id === w.id ? w : x))
          : [...v.workspaces, w],
      }));
      close();
      app.navigate({ type: "workspace", id: w.id });
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={workspace ? t("编辑工作空间") : t("新建工作空间")}
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!name.trim() || busy}
            onClick={() => void save()}
          >
            {workspace ? t("保存") : t("创建工作空间")}
          </Button>
        </>
      }
    >
      <Field label={t("名称")}>
        <input
          autoFocus
          placeholder={t("例如：产品设计、旅行、个人项目")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <Field label={t("描述")}>
        <textarea
          rows={2}
          placeholder={t("关于这个空间…")}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </Field>
      <Field label={t("颜色")}>
        <div className="color-options">
          {colors.map((c) => (
            <button
              key={c}
              style={{ backgroundColor: c }}
              className={color === c ? "selected" : ""}
              aria-label={c}
              onClick={() => setColor(c)}
            />
          ))}
          <input
            type="color"
            value={color}
            onChange={(e) => setColor(e.target.value)}
          />
        </div>
      </Field>
      <Field
        label={t("位置 · {0}", paths.length)}
        hint={t("文件仍保存在原来的位置，工作空间仅保存引用。")}
      >
        <div className="selection-strip">
          {paths.map((p) => (
            <button
              key={p}
              onClick={() => setPaths((v) => v.filter((x) => x !== p))}
            >
              {basename(p)} ×
            </button>
          ))}
        </div>
        <Button
          onClick={async () => {
            const chosen = await app.pickFiles({ multiple: true });
            setPaths((v) => [...new Set([...v, ...chosen])]);
          }}
        >
          <Plus size={13} />
          {t("添加文件与文件夹")}
        </Button>
      </Field>
      {app.catalog.bookmarks.length > 0 && (
        <Field label={t("关联书签")}>
          <div className="bookmark-select">
            {app.catalog.bookmarks.map((b) => (
              <label className="check-label" key={b.id}>
                <input
                  type="checkbox"
                  checked={links.includes(b.id)}
                  onChange={(e) =>
                    setLinks((v) =>
                      e.target.checked
                        ? [...v, b.id]
                        : v.filter((x) => x !== b.id),
                    )
                  }
                />
                {b.title}
              </label>
            ))}
          </div>
        </Field>
      )}
    </Modal>
  );
}
function WorkspaceDetail({ workspace: w }: { workspace: Workspace }) {
  const app = useApp();
  const [tab, setTab] = useState("all");
  const [view, setView] = useState(w.view || "cards");
  const [query, setQuery] = useState("");
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    path: string;
  } | null>(null);
  const [recursive, setRecursive] = useState(false);
  const entries = useAsync(async () => {
    const values = await Promise.allSettled(
      w.paths.map((path) => request<Entry>("file_info", { path })),
    );
    return values.map((v, i) => ({
      path: w.paths[i],
      entry: v.status === "fulfilled" ? v.value : null,
      error: v.status === "rejected" ? String(v.reason) : "",
    }));
  }, [w.paths.join("\0"), app.refresh]);
  const search = useAsync(
    () =>
      recursive && query.length > 1
        ? request<{ entries: Entry[]; truncated: boolean }>("search", {
            query,
            roots: w.paths,
            limit: 300,
          })
        : Promise.resolve(null),
    [recursive, query, w.paths.join("\0")],
  );
  const rows =
    recursive && search.data
      ? search.data.entries.map((entry) => ({
          entry,
          path: entry.path,
          error: "",
        }))
      : entries.data || [];
  let filtered = rows.filter(
    ({ entry, path }) =>
      (!query ||
        recursive ||
        (entry?.name || path).toLowerCase().includes(query.toLowerCase())) &&
      (tab === "all" || tab === "recent" || tab === "folders"
        ? tab !== "folders" || (entry && isFolder(entry))
        : entry?.kind === tab),
  );
  if (tab === "recent")
    filtered = [...filtered].sort(
      (a, b) => (b.entry?.modified || 0) - (a.entry?.modified || 0),
    );
  const save = (patch: Partial<Workspace>) =>
    app.updateCatalog((v) => ({
      ...v,
      workspaces: v.workspaces.map((x) =>
        x.id === w.id ? { ...x, ...patch } : x,
      ),
    }));
  const links = app.catalog.bookmarks.filter((b) => w.links.includes(b.id));
  return (
    <div
      className="feature-page"
      onDragOver={(e) => e.preventDefault()}
      onDrop={async (e) => {
        e.preventDefault();
        const raw = e.dataTransfer.getData("application/x-filemanager-paths");
        if (raw)
          await save({ paths: [...new Set([...w.paths, ...JSON.parse(raw)])] });
      }}
    >
      <div className="page-toolbar">
        <Button onClick={() => app.navigate({ type: "workspace", id: "all" })}>
          <ArrowLeft size={13} />
          {t("工作空间")}
        </Button>
        <div className="flex-1" />
        <IconButton
          label={t("编辑工作空间")}
          onClick={() =>
            app.showModal(
              <WorkspaceEditor
                workspace={w}
                close={() => app.showModal(null)}
              />,
            )
          }
        >
          <MoreHorizontal size={17} />
        </IconButton>
      </div>
      <div
        className="workspace-detail-cover"
        style={{ "--workspace-color": w.color } as CSSProperties}
      >
        <GlassFolder color={w.color} name={w.name} symbol="work" />
        <div className="flex-1">
          <h2>{w.name}</h2>
          <p>
            {w.description ||
              t("{0} 个位置 · {1} 个链接", w.paths.length, w.links.length)}
          </p>
        </div>
        <Button
          onClick={async () => {
            const p = await app.pickFiles();
            if (p.length)
              await save({ paths: [...new Set([...w.paths, ...p])] });
          }}
        >
          <Plus size={14} />
          {t("添加项目")}
        </Button>
      </div>
      {w.pinned.length > 0 && (
        <div className="workspace-pins">
          {w.pinned.slice(0, 6).map((p) => {
            const e = entries.data?.find((e) => e.path === p)?.entry;
            return (
              <button
                className="workspace-pin"
                key={p}
                onClick={() => e && app.open(e)}
              >
                {e ? <FileIcon entry={e} /> : <FolderOpen size={16} />}
                <span>{e?.name || basename(p)}</span>
                <Pin size={10} />
              </button>
            );
          })}
        </div>
      )}
      <div className="page-tabs">
        {[
          ["all", t("所有项目")],
          ["folders", t("文件夹")],
          ["image", t("图像")],
          ["pdf", t("文档")],
          ["recent", t("最近变化")],
          ["links", t("链接")],
        ].map(([value, label]) => (
          <button
            key={value}
            className={tab === value ? "active" : ""}
            onClick={() => setTab(value)}
          >
            {label}
          </button>
        ))}
      </div>
      <div className="page-toolbar">
        <div className="search-input">
          <Search size={14} />
          <input
            placeholder={t("搜索空间内的文件…")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <label className="check-label" style={{ margin: 0 }}>
          <input
            type="checkbox"
            checked={recursive}
            onChange={(e) => setRecursive(e.target.checked)}
          />
          {t("跨目录")}
        </label>
        <div className="flex-1" />
        <Select
          value={view}
          onChange={(e) => {
            setView(e.target.value as any);
            void save({ view: e.target.value as any });
          }}
        >
          <option value="cards">{t("卡片")}</option>
          <option value="tiles">{t("小图标")}</option>
          <option value="list">{t("列表")}</option>
        </Select>
      </div>
      {tab === "links" ? (
        links.length ? (
          <div className="bookmark-grid">
            {links.map((b) => (
              <article className="bookmark-card" key={b.id}>
                <div
                  className="bookmark-body"
                  onClick={() => void desktop("open_url", { url: b.url })}
                >
                  <h3>{b.title}</h3>
                  <p>{b.url}</p>
                  <footer>
                    <Link2 size={12} />
                    {b.site}
                  </footer>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <Empty icon={<Link2 size={28} />} title={t("还没有关联链接")}>
            <Button
              onClick={() =>
                app.showModal(
                  <WorkspaceEditor
                    workspace={w}
                    close={() => app.showModal(null)}
                  />,
                )
              }
            >
              {t("选择书签")}
            </Button>
          </Empty>
        )
      ) : entries.loading ? (
        <Loading />
      ) : !filtered.length ? (
        <Empty icon={<Layers size={28} />} title={t("为这件事留一个空间")}>
          <p>{t("添加文件、文件夹和链接，它们会保留在原来的位置。")}</p>
          <Button
            onClick={async () => {
              const p = await app.pickFiles();
              if (p.length)
                await save({ paths: [...new Set([...w.paths, ...p])] });
            }}
          >
            <Plus size={13} />
            {t("添加项目")}
          </Button>
        </Empty>
      ) : (
        <div
          className={view === "list" ? "workspace-list" : "workspace-files"}
          style={
            view === "tiles"
              ? { gridTemplateColumns: "repeat(auto-fill,minmax(125px,1fr))" }
              : undefined
          }
        >
          {filtered.map(({ path, entry, error }) => (
            <div
              className="workspace-file"
              key={path}
              onDoubleClick={() => entry && app.open(entry)}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenu({ x: e.clientX, y: e.clientY, path });
              }}
            >
              {entry ? (
                <>
                  <div
                    onClick={() =>
                      app.peek(
                        entry,
                        filtered.flatMap((v) => (v.entry ? [v.entry] : [])),
                      )
                    }
                  >
                    <FileIcon entry={entry} thumbnail={view !== "list"} />
                  </div>
                  <div className="file-name" onClick={() => app.open(entry)}>
                    {entry.name}
                  </div>
                  <div className="file-path" title={path}>
                    {tab === "recent" ? shortDate(entry.modified) : path}
                  </div>
                </>
              ) : (
                <>
                  <div className="file-visual">
                    <FolderOpen size={28} className="muted" />
                  </div>
                  <div className="file-name">{basename(path)}</div>
                  <div className="file-path danger-text" title={error}>
                    {t("原位置已失联")}
                  </div>
                  <Button
                    onClick={async () => {
                      const p = await app.pickFiles({ multiple: false });
                      if (p[0])
                        await save({
                          paths: w.paths.map((x) => (x === path ? p[0] : x)),
                          pinned: w.pinned.map((x) => (x === path ? p[0] : x)),
                        });
                    }}
                  >
                    {t("重新定位")}
                  </Button>
                </>
              )}
              <IconButton
                label={t("项目选项")}
                onClick={(e) => setMenu({ x: e.clientX, y: e.clientY, path })}
              >
                <MoreHorizontal size={15} />
              </IconButton>
            </div>
          ))}
        </div>
      )}
      {search.data?.truncated && (
        <p className="muted">{t("搜索结果较多，仅显示前 300 项。")}</p>
      )}
      {menu && (
        <Menu
          {...menu}
          close={() => setMenu(null)}
          items={[
            {
              label: w.pinned.includes(menu.path)
                ? t("取消固定")
                : t("固定到顶部"),
              icon: <Pin size={14} />,
              disabled: !w.pinned.includes(menu.path) && w.pinned.length >= 6,
              action: () =>
                void save({
                  pinned: w.pinned.includes(menu.path)
                    ? w.pinned.filter((x) => x !== menu.path)
                    : [...w.pinned, menu.path],
                }),
            },
            {
              label: t("加入全局快速访问"),
              icon: <Star size={14} />,
              action: () =>
                void app.updateCatalog((v) => ({
                  ...v,
                  quickAccess: [...new Set([...v.quickAccess, menu.path])],
                })),
            },
            {
              label: t("移除引用"),
              icon: <X size={14} />,
              action: () =>
                void save({
                  paths: w.paths.filter((x) => x !== menu.path),
                  pinned: w.pinned.filter((x) => x !== menu.path),
                }),
            },
          ]}
        />
      )}
    </div>
  );
}
