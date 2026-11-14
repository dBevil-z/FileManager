import { t } from "../i18n";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";
import { Layers, Plus, Trash2, X } from "lucide-react";
import { useApp } from "../context";
import { desktop, native, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Entry } from "../types";
import { basename } from "../lib/files";
import { FileIcon, GlassFolder } from "../components/FileIcon";
import { Button, IconButton } from "../components/ui";
export function Shelf({
  close,
  standalone = false,
}: {
  close: () => void;
  standalone?: boolean;
}) {
  const app = useApp();
  const targets = app.catalog.shelf.length
    ? app.catalog.shelf.slice(0, 9)
    : app.boot.places
        .filter((p) => !["root", "home", "music"].includes(p.id))
        .slice(0, 6)
        .map((p) => p.path);
  const [dragPaths, setDragPaths] = useState<string[]>([]);
  const files = useAsync(async () => {
    const results = await Promise.allSettled(
      app.catalog.holding.map((path) => request<Entry>("file_info", { path })),
    );
    return results.map((v, i) => ({
      path: app.catalog.holding[i],
      entry: v.status === "fulfilled" ? v.value : null,
    }));
  }, [app.catalog.holding.join("\0"), app.refresh]);
  const addHolding = (paths: string[]) =>
    app.updateCatalog((v) => ({
      ...v,
      holding: [...new Set([...v.holding, ...paths])],
    }));
  const send = async (destination: string, paths: string[], move = false) => {
    if (paths.length)
      await app.operate(move ? "move" : "copy", paths, destination);
    else {
      if (standalone) {
        await desktop("navigate", { path: destination });
        close();
      } else {
        app.navigate({ type: "folder", path: destination });
        close();
      }
    }
  };
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input,textarea")) return;
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
      if (/^[1-9]$/.test(e.key) && targets[Number(e.key) - 1]) {
        e.preventDefault();
        void send(targets[Number(e.key) - 1], app.catalog.holding, e.altKey);
      }
    };
    window.addEventListener("keydown", k, true);
    return () => window.removeEventListener("keydown", k, true);
  }, [targets.join("\0"), app.catalog.holding.join("\0")]);
  useEffect(() => {
    if (!native || !standalone) return;
    let off: (() => void) | undefined;
    let alive = true;
    void (async () => {
      const win = getCurrentWindow();
      const scale = await win.scaleFactor();
      return win.onDragDropEvent((e) => {
        if (e.payload.type === "drop") {
          const target = document
            .elementFromPoint(
              e.payload.position.x / scale,
              e.payload.position.y / scale,
            )
            ?.closest<HTMLElement>("[data-shelf-path]");
          if (target?.dataset.shelfPath)
            void send(target.dataset.shelfPath, e.payload.paths);
          else void addHolding(e.payload.paths);
        }
      });
    })()
      .then((fn) => {
        if (alive) off = fn;
        else fn();
      })
      .catch((e) => app.notify(String(e), true));
    return () => {
      alive = false;
      off?.();
    };
  }, [standalone, app.catalog.holding.join("\0")]);
  function getPaths(e: React.DragEvent) {
    try {
      return JSON.parse(
        e.dataTransfer.getData("application/x-filemanager-paths"),
      ) as string[];
    } catch {
      return dragPaths;
    }
  }
  const content = (
    <section className={`shelf-window ${standalone ? "shelf-standalone" : ""}`}>
      <header data-tauri-drag-region>
        <Layers size={17} />
        <h2>Drop Shelf</h2>
        <span>{t("把文件，放到顺手的地方。")}</span>
        <div className="flex-1" />
        <IconButton
          label={t("设置固定目录")}
          onClick={async () => {
            const paths = await app.pickFiles({ directories: true });
            if (paths.length)
              await app.updateCatalog((v) => ({
                ...v,
                shelf: paths.slice(0, 9),
              }));
          }}
        >
          <Plus size={15} />
        </IconButton>
        <IconButton label={t("关闭 Shelf")} onClick={close}>
          <X size={16} />
        </IconButton>
      </header>
      <div
        className="shelf-targets"
        style={{
          gridTemplateColumns: `repeat(${Math.min(9, Math.max(1, targets.length))},minmax(0,1fr))`,
        }}
      >
        {targets.map((path, i) => (
          <button
            data-shelf-path={path}
            className="shelf-target"
            key={path}
            onClick={() => void send(path, app.catalog.holding)}
            onDragOver={(e) => {
              e.preventDefault();
              e.currentTarget.classList.add("drop-target");
            }}
            onDragLeave={(e) => e.currentTarget.classList.remove("drop-target")}
            onDrop={(e) => {
              e.preventDefault();
              e.stopPropagation();
              e.currentTarget.classList.remove("drop-target");
              void send(path, getPaths(e), e.altKey);
            }}
          >
            <kbd>{i + 1}</kbd>
            <GlassFolder
              name={basename(path)}
              color={app.settings.folderColor}
            />
            <span>
              {app.boot.places.find((p) => p.path === path)?.name ||
                basename(path)}
            </span>
          </button>
        ))}
      </div>
      <div
        style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 10 }}
      >
        <span className="muted">Holding</span>
        <span className="badge">{app.catalog.holding.length}</span>
        <div className="flex-1" />
        {app.catalog.holding.length > 0 && (
          <button
            className="muted"
            onClick={() =>
              void app.updateCatalog((v) => ({ ...v, holding: [] }))
            }
          >
            {t("清空引用")}
          </button>
        )}
      </div>
      <div
        className="shelf-holding"
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          e.preventDefault();
          void addHolding(getPaths(e));
        }}
      >
        {files.data?.length ? (
          files.data.map(({ path, entry }) => (
            <div
              className="holding-item"
              key={path}
              draggable
              onDragStart={(e) => {
                setDragPaths([path]);
                if (native) {
                  e.preventDefault();
                  void desktop("drag_files", { paths: [path] });
                } else {
                  e.dataTransfer.setData(
                    "application/x-filemanager-paths",
                    JSON.stringify([path]),
                  );
                  e.dataTransfer.setData(
                    "text/uri-list",
                    `file://${encodeURI(path)}`,
                  );
                }
              }}
              title={path}
            >
              {entry ? (
                <FileIcon entry={entry} />
              ) : (
                <Layers size={25} className="muted" />
              )}
              <span>{entry?.name || basename(path)}</span>
              <IconButton
                label={t("移除引用")}
                onClick={() =>
                  void app.updateCatalog((v) => ({
                    ...v,
                    holding: v.holding.filter((x) => x !== path),
                  }))
                }
              >
                <X size={10} />
              </IconButton>
            </div>
          ))
        ) : (
          <p>{t("将文件暂存在这里，再拖到其他应用或固定目录。")}</p>
        )}
      </div>
      <footer>
        <span>{t("文件保留在原位置")}</span>
        <div className="flex-1" />
        <span>{t("1–9 复制到目录 · Alt 拖动移动 · Esc 关闭")}</span>
      </footer>
    </section>
  );
  return standalone ? (
    content
  ) : (
    <div
      className="shelf-backdrop"
      onClick={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      {content}
    </div>
  );
}
