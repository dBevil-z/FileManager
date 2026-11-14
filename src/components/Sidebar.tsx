import { t } from "../i18n";
import { useState } from "react";
import {
  Bookmark,
  BriefcaseBusiness,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  Cloud,
  Code2,
  Download,
  Folder,
  HardDrive,
  Home,
  Image,
  Layers,
  LayoutGrid,
  Monitor,
  MoreHorizontal,
  Music2,
  Package,
  Plus,
  Search,
  Settings2,
  Sparkles,
  Star,
  Trash2,
  Video,
  X,
  GripVertical,
  Puzzle,
  Info,
} from "lucide-react";
import { useApp } from "../context";
import type { Page } from "../types";
import { request } from "../api";
import { useAsync } from "../hooks/useAsync";
import { IconButton, Menu } from "./ui";
const placeIcons: Record<string, typeof Folder> = {
  home: Home,
  desktop: Monitor,
  downloads: Download,
  documents: Folder,
  pictures: Image,
  music: Music2,
  videos: Video,
  root: HardDrive,
};
export function Sidebar({
  quickSearch,
  createWorkspace,
}: {
  quickSearch: () => void;
  createWorkspace: () => void;
}) {
  const app = useApp();
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    index: number;
  } | null>(null);
  const [dragged, setDragged] = useState(-1);
  const [groups, setGroups] = useState<Record<string, boolean>>({});
  const mounts = useAsync(
    () => request<{ path: string; name: string }[]>("mounts"),
    [app.refresh],
  );
  const active = (page: Page) =>
    JSON.stringify(app.page) === JSON.stringify(page);
  function item(
    page: Page,
    name: string,
    Icon: typeof Folder,
    extra?: React.ReactNode,
  ) {
    return (
      <button
        key={name}
        className={`sidebar-item ${active(page) ? "selected" : ""}`}
        onClick={() => app.navigate(page)}
      >
        <Icon size={16} strokeWidth={1.65} />
        <span>{name}</span>
        {extra}
      </button>
    );
  }
  function heading(name: string, key: string, add?: () => void) {
    return (
      <div className="sidebar-heading">
        <button onClick={() => setGroups((v) => ({ ...v, [key]: !v[key] }))}>
          {groups[key] ? <ChevronRight size={10} /> : <ChevronDown size={10} />}
          {name}
        </button>
        {add && (
          <IconButton label={t("添加{0}", name)} onClick={add}>
            <Plus size={12} />
          </IconButton>
        )}
      </div>
    );
  }
  return (
    <aside className="sidebar">
      <div className="sidebar-brand" data-tauri-drag-region>
        <span className="brand-mark">
          <Folder size={19} strokeWidth={2} />
        </span>
        <strong>FileManager</strong>
        <span className="brand-linux">LINUX</span>
      </div>
      <button className="sidebar-search" onClick={quickSearch}>
        <Search size={14} />
        <span>{t("快速切换")}</span>
        <kbd>Ctrl K</kbd>
      </button>
      <div className="sidebar-scroll">
        {heading(t("位置"), "places")}
        {!groups.places &&
          app.boot.places
            .filter((p) => p.id !== "root")
            .map((p) =>
              item(
                { type: "folder", path: p.path },
                t(p.name),
                placeIcons[p.id] || Folder,
              ),
            )}
        {heading(t("收藏"), "favourites", async () => {
          const paths = await app.pickFiles({ directories: true });
          if (paths.length)
            await app.updateCatalog((v) => ({
              ...v,
              favourites: [
                ...v.favourites,
                ...paths
                  .filter((p) => !v.favourites.some((f) => f.path === p))
                  .map((p) => ({
                    id: crypto.randomUUID(),
                    name: p.split("/").pop() || p,
                    path: p,
                  })),
              ],
            }));
        })}
        {!groups.favourites &&
          (app.catalog.favourites.length ? (
            app.catalog.favourites.map((p, i) => (
              <div
                key={p.id}
                className="sidebar-favourite"
                draggable
                onDragStart={() => setDragged(i)}
                onDragOver={(e) => e.preventDefault()}
                onDrop={(e) => {
                  e.preventDefault();
                  if (dragged < 0 || dragged === i) return;
                  void app.updateCatalog((v) => {
                    const list = [...v.favourites];
                    const [p] = list.splice(dragged, 1);
                    list.splice(i, 0, p);
                    return { ...v, favourites: list };
                  });
                  setDragged(-1);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setMenu({ x: e.clientX, y: e.clientY, index: i });
                }}
              >
                {item({ type: "folder", path: p.path }, p.name, Star)}
              </div>
            ))
          ) : (
            <p className="sidebar-hint">{t("将常用文件夹收藏到这里")}</p>
          ))}
        {app.catalog.quickAccess.length > 0 && (
          <>
            {heading(t("快速访问"), "quickAccess")}
            {!groups.quickAccess &&
              app.catalog.quickAccess.map((path) => (
                <div
                  key={path}
                  className="sidebar-favourite"
                  onContextMenu={(e) => {
                    e.preventDefault();
                    void app.updateCatalog((v) => ({
                      ...v,
                      quickAccess: v.quickAccess.filter((p) => p !== path),
                    }));
                  }}
                >
                  {item(
                    { type: "folder", path },
                    path.split("/").pop() || path,
                    Folder,
                  )}
                </div>
              ))}
          </>
        )}
        {heading(t("工作空间"), "workspaces", createWorkspace)}
        {!groups.workspaces && (
          <>
            {item(
              { type: "workspace", id: "all" },
              t("所有工作空间"),
              LayoutGrid,
              <span className="sidebar-count">
                {app.catalog.workspaces.filter((w) => !w.archived).length}
              </span>,
            )}
            {app.catalog.workspaces
              .filter((w) => !w.archived)
              .map((w) => (
                <button
                  className={`sidebar-item workspace-link ${app.page.type === "workspace" && app.page.id === w.id ? "selected" : ""}`}
                  key={w.id}
                  onClick={() => app.navigate({ type: "workspace", id: w.id })}
                >
                  <span
                    className="workspace-dot"
                    style={{ background: w.color }}
                  />
                  <span>{w.name}</span>
                  {w.pinned.length > 0 && (
                    <span className="sidebar-count">{w.paths.length}</span>
                  )}
                </button>
              ))}
          </>
        )}
        {heading(t("书签"), "bookmarks", () =>
          app.navigate({ type: "bookmarks" }),
        )}
        {!groups.bookmarks && (
          <>
            {item(
              { type: "bookmarks" },
              t("所有书签"),
              Bookmark,
              <span className="sidebar-count">
                {app.catalog.bookmarks.length}
              </span>,
            )}
            {item({ type: "bookmarks", favourites: true }, t("收藏书签"), Star)}
            {app.catalog.collections.map((c) => (
              <button
                key={c.id}
                className={`sidebar-item ${app.page.type === "bookmarks" && app.page.id === c.id ? "selected" : ""}`}
                onClick={() => app.navigate({ type: "bookmarks", id: c.id })}
              >
                <span className="collection-dot" style={{ color: c.color }}>
                  <Layers size={15} />
                </span>
                <span>{c.name}</span>
              </button>
            ))}
          </>
        )}
        {item({ type: "remote" }, t("远程存储"), Cloud)}
        {heading(t("工具"), "tools")}
        {!groups.tools && (
          <>
            {item({ type: "clean" }, t("智能清理"), Sparkles)}
            {item({ type: "packages" }, t("软件包"), Package)}
            {item({ type: "agents" }, "AI Agents", Code2)}
            {item({ type: "plugins" }, t("插件中心"), Puzzle)}
            {item({ type: "about" }, t("关于 FileManager"), Info)}
            {item({ type: "system" }, t("关于本机"), Monitor)}
          </>
        )}
        {heading(t("设备"), "devices")}
        {!groups.devices &&
          mounts.data?.map((p) =>
            item(
              { type: "folder", path: p.path },
              p.path === "/" ? t("系统磁盘") : p.name,
              HardDrive,
            ),
          )}
        <div className="sidebar-bottom-space" />
        {item({ type: "trash" }, t("回收站"), Trash2)}
      </div>
      <div className="sidebar-footer">
        <button
          className={app.page.type === "settings" ? "selected" : ""}
          onClick={() => app.navigate({ type: "settings" })}
        >
          <Settings2 size={16} />
          <span>{t("设置")}</span>
        </button>
        <IconButton
          label={t("使用说明")}
          onClick={() =>
            void app.saveText(
              t(
                "Ctrl K 快速切换 · Ctrl L 路径 · Space 预览 · F2 重命名 · F5/F6 双栏复制/移动 · Ctrl Alt Space Drop Shelf",
              ),
            )
          }
        >
          <CircleHelp size={15} />
        </IconButton>
      </div>
      {menu && (
        <Menu
          x={menu.x}
          y={menu.y}
          close={() => setMenu(null)}
          items={[
            {
              label: t("重新命名收藏…"),
              action: async () => {
                const name = await app.prompt(
                  t("收藏名称"),
                  app.catalog.favourites[menu.index].name,
                );
                if (name)
                  await app.updateCatalog((v) => ({
                    ...v,
                    favourites: v.favourites.map((p, i) =>
                      i === menu.index ? { ...p, name } : p,
                    ),
                  }));
              },
            },
            {
              label: t("移除收藏"),
              danger: true,
              action: () =>
                void app.updateCatalog((v) => ({
                  ...v,
                  favourites: v.favourites.filter((_, i) => i !== menu.index),
                })),
            },
          ]}
        />
      )}
    </aside>
  );
}
