import { t } from "../i18n";
import { useState } from "react";
import {
  ChevronLeft,
  Home,
  FolderOpen,
  Check,
  Search,
  Folder,
} from "lucide-react";
import { useApp } from "../context";
import { request } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Directory } from "../types";
import { isFolder, basename } from "../lib/files";
import { Modal, Button, IconButton, Loading, ErrorState } from "./ui";
import { FileIcon } from "./FileIcon";
export function FilePicker({
  initial,
  directories,
  filesOnly,
  multiple = true,
  done,
}: {
  initial?: string;
  directories?: boolean;
  filesOnly?: boolean;
  multiple?: boolean;
  done: (paths: string[]) => void;
}) {
  const { boot } = useApp();
  const [path, setPath] = useState(initial || boot.home);
  const [selected, setSelected] = useState<string[]>([]);
  const [search, setSearch] = useState("");
  const { data, loading, error } = useAsync(
    () => request<Directory>("list_directory", { path }),
    [path],
  );
  function toggle(p: string) {
    setSelected((old) =>
      old.includes(p)
        ? old.filter((v) => v !== p)
        : multiple
          ? [...old, p]
          : [p],
    );
  }
  function openFolder(p: string) {
    if (!multiple) setSelected([]);
    setPath(p);
  }
  return (
    <Modal
      title={
        directories
          ? t("选择文件夹")
          : filesOnly
            ? t("选择文件")
            : t("添加文件与文件夹")
      }
      wide
      className="file-picker"
      onClose={() => done([])}
      footer={
        <>
          <span className="muted flex-1">
            {t("已选择 ")}
            {selected.length}
            {t(" 项")}
          </span>
          <Button onClick={() => done([])}>{t("取消")}</Button>
          <Button
            tone="primary"
            onClick={() =>
              done(selected.length ? selected : directories ? [path] : [])
            }
            disabled={!selected.length && !directories}
          >
            {t("选择")}
            {selected.length ? t(" {0} 项", selected.length) : t("此文件夹")}
          </Button>
        </>
      }
    >
      <div className="picker-toolbar">
        <IconButton
          label={t("上级目录")}
          disabled={!data?.parent}
          onClick={() => data?.parent && openFolder(data.parent)}
        >
          <ChevronLeft size={17} />
        </IconButton>
        <IconButton label={t("主目录")} onClick={() => openFolder(boot.home)}>
          <Home size={16} />
        </IconButton>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const v = new FormData(e.currentTarget).get("path");
            if (typeof v === "string") openFolder(v);
          }}
        >
          <input
            name="path"
            key={path}
            defaultValue={path}
            aria-label={t("目录路径")}
          />
        </form>
        <div className="search-input">
          <Search size={14} />
          <input
            placeholder={t("筛选…")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
      </div>
      <div className="picker-body">
        <nav>
          {boot.places.map((p) => (
            <button
              key={p.id}
              onClick={() => openFolder(p.path)}
              className={path === p.path ? "selected" : ""}
            >
              <Folder size={16} />
              {t(p.name)}
            </button>
          ))}
        </nav>
        <div className="picker-files">
          {loading ? (
            <Loading />
          ) : error ? (
            <ErrorState error={error} />
          ) : (
            data?.entries
              .filter(
                (e) =>
                  (!directories || isFolder(e)) &&
                  e.name.toLowerCase().includes(search.toLowerCase()),
              )
              .map((e) => (
                <div
                  role="option"
                  tabIndex={0}
                  key={e.path}
                  className={`picker-file ${selected.includes(e.path) ? "selected" : ""}`}
                  aria-selected={selected.includes(e.path)}
                  onKeyDown={(event) => {
                    if (
                      event.target !== event.currentTarget ||
                      !["Enter", " "].includes(event.key)
                    )
                      return;
                    event.preventDefault();
                    if (isFolder(e) && (event.key === "Enter" || filesOnly))
                      openFolder(e.path);
                    else toggle(e.path);
                  }}
                  onClick={() => (!filesOnly || !isFolder(e)) && toggle(e.path)}
                  onDoubleClick={() =>
                    isFolder(e) ? openFolder(e.path) : done([e.path])
                  }
                >
                  <FileIcon entry={e} />
                  <span>{e.name}</span>
                  {isFolder(e) && (
                    <IconButton
                      label={t("打开 {0}", e.name)}
                      onClick={(ev) => {
                        ev.stopPropagation();
                        openFolder(e.path);
                      }}
                    >
                      <FolderOpen size={15} />
                    </IconButton>
                  )}
                  {selected.includes(e.path) && <Check size={15} />}
                </div>
              ))
          )}
        </div>
      </div>
      {selected.length > 0 && (
        <div className="selection-strip">
          {selected.map((p) => (
            <button key={p} onClick={() => toggle(p)}>
              {basename(p)} ×
            </button>
          ))}
        </div>
      )}
    </Modal>
  );
}
