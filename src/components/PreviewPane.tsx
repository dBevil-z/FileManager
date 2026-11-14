import { t } from "../i18n";
import { MarkdownImage } from "./MarkdownImage";
import { useEffect, useState } from "react";
import {
  ArrowUpRight,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  FileSearch,
  FolderOpen,
  Info,
  Maximize2,
  Pencil,
  X,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useApp } from "../context";
import { desktop, mediaUrl, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Entry } from "../types";
import { formatBytes, isFolder, kindLabel, shortDate } from "../lib/files";
import { FileIcon, loadPreview, type PreviewData } from "./FileIcon";
import { Button, Empty, ErrorState, IconButton, Loading } from "./ui";
export function PreviewPane({
  entry,
  close,
}: {
  entry?: Entry;
  close: () => void;
}) {
  const app = useApp();
  const data = useAsync(
    (signal) =>
      entry ? loadPreview(entry, false, signal) : Promise.resolve(null),
    [entry?.path, entry?.modified, app.refresh],
  );
  const size = useAsync(
    () =>
      entry && isFolder(entry)
        ? request<{
            size: number;
            files: number;
            folders: number;
            truncated: boolean;
          }>("folder_size", { path: entry.path })
        : Promise.resolve(null),
    [entry?.path, app.refresh],
  );
  return (
    <aside className="preview-pane">
      <header>
        <strong>{t("预览")}</strong>
        <IconButton label={t("隐藏预览")} onClick={close}>
          <X size={15} />
        </IconButton>
      </header>
      {!entry ? (
        <Empty icon={<FileSearch size={28} />} title={t("查看文件详情")}>
          {t("选择文件，快速查看内容与信息。")}
        </Empty>
      ) : (
        <div className="preview-scroll">
          <div className="preview-cover" onDoubleClick={() => app.peek(entry)}>
            {isFolder(entry) ? (
              <FileIcon entry={entry} thumbnail />
            ) : (
              <PreviewContent
                entry={entry}
                data={data.data}
                compact
                loading={data.loading}
              />
            )}
          </div>
          <div className="preview-title">
            <h3>{entry.name}</h3>
            <span>{kindLabel(entry)}</span>
          </div>
          <div className="preview-actions">
            <Button onClick={() => app.open(entry)}>
              <ArrowUpRight size={13} />
              {t("打开")}
            </Button>
            <Button onClick={() => app.peek(entry)}>
              <Maximize2 size={12} />
              {t("快速预览")}
            </Button>
          </div>
          {data.error && <p className="inline-warning">{data.error}</p>}
          <Properties entry={entry} data={data.data} size={size.data?.size} />
          {isFolder(entry) && data.data?.children && (
            <div className="preview-section">
              <h4>
                {t("文件夹内容")}{" "}
                <span>{size.data ? t("{0} 个文件", size.data.files) : ""}</span>
              </h4>
              <FolderTree rows={data.data.children as any} />
              {size.data?.truncated && (
                <p className="muted">{t("仅统计部分文件")}</p>
              )}
            </div>
          )}
          {data.data?.warning && (
            <p className="inline-warning">{data.data.warning}</p>
          )}
        </div>
      )}
    </aside>
  );
}
export function PreviewContent({
  entry,
  data,
  compact = false,
  loading,
}: {
  entry: Entry;
  data: PreviewData | null;
  compact?: boolean;
  loading?: boolean;
}) {
  if (loading && !data) return <Loading label={t("生成预览…")} />;
  if (isFolder(entry)) return <FileIcon entry={entry} thumbnail />;
  if (data?.media && entry.kind === "video" && !compact)
    return (
      <video
        controls
        autoPlay={false}
        src={mediaUrl(data.media)}
        poster={data.thumbnail ? mediaUrl(data.thumbnail) : undefined}
      />
    );
  if (data?.media && entry.kind === "audio")
    return (
      <div className="audio-preview">
        <FileIcon entry={entry} thumbnail />
        <audio controls src={mediaUrl(data.media)} />
      </div>
    );
  if (data?.media && entry.kind === "pdf" && !compact)
    return (
      <PdfPages
        key={`${entry.path}:${entry.modified}`}
        entry={entry}
        first={data}
      />
    );
  if (data?.thumbnail)
    return (
      <img
        className="preview-image"
        src={mediaUrl(data.thumbnail)}
        alt={entry.name}
      />
    );
  if (data?.text)
    return entry.kind === "markdown" ? (
      <div className={`markdown-content ${compact ? "compact" : ""}`}>
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          components={{
            a: ({ href, children }) => (
              <a
                href={href}
                onClick={(e) => {
                  e.preventDefault();
                  if (href && /^https?:/.test(href))
                    void desktop("open_url", { url: href });
                }}
              >
                {children}
              </a>
            ),
            img: ({ src, alt }) => (
              <MarkdownImage document={entry.path} src={src} alt={alt} />
            ),
          }}
        >
          {compact ? data.text.text.slice(0, 2000) : data.text.text}
        </ReactMarkdown>
      </div>
    ) : (
      <pre className="code-preview">
        {compact ? data.text.text.slice(0, 2400) : data.text.text}
      </pre>
    );
  return (
    <div className="unsupported-preview">
      <FileIcon entry={entry} thumbnail />
      {!compact && <p>{data?.warning || t("此格式可用系统应用打开。")}</p>}
    </div>
  );
}
function Properties({
  entry,
  data,
  size,
}: {
  entry: Entry;
  data: PreviewData | null;
  size?: number;
}) {
  const app = useApp();
  const meta = data?.metadata || {};
  const rows: [string, string][] = [
    [t("种类"), kindLabel(entry)],
    [t("大小"), formatBytes(size ?? entry.size)],
    [t("修改时间"), shortDate(entry.modified)],
    [
      t("创建时间"),
      entry.created ? shortDate(entry.created) : t("文件系统未提供"),
    ],
    [t("访问权限"), entry.writable ? t("可读写") : t("只读")],
  ];
  if (meta.width) rows.push([t("尺寸"), `${meta.width} × ${meta.height}`]);
  if (meta.Pages) rows.push([t("页数"), String(meta.Pages)]);
  if (meta.Author) rows.push([t("作者"), String(meta.Author)]);
  for (const [key, label] of Object.entries({
    Make: t("厂商"),
    Model: t("相机"),
    DateTimeOriginal: t("拍摄时间"),
    ExposureTime: t("快门"),
    FNumber: t("光圈"),
    PhotographicSensitivity: "ISO",
    FocalLength: t("焦距"),
    LensModel: t("镜头"),
    GPSLatitude: t("纬度"),
    GPSLongitude: t("经度"),
  }))
    if (meta[key]) rows.push([label, String(meta[key])]);
  const stream = (
    meta.streams as
      { codec_type?: string; width?: number; height?: number }[] | undefined
  )?.find((s) => s.codec_type === "video");
  if (stream?.width)
    rows.push([t("分辨率"), `${stream.width} × ${stream.height}`]);
  const format = meta.format as { duration?: string } | undefined;
  if (format?.duration)
    rows.push([
      t("时长"),
      t(
        "{0} 分 {1} 秒",
        Math.floor(Number(format.duration) / 60),
        Math.round(Number(format.duration) % 60),
      ),
    ]);
  return (
    <div className="properties">
      <h4>{t("信息")}</h4>
      {rows.map(([label, value]) => (
        <div key={label}>
          <span>{label}</span>
          <b>{value}</b>
        </div>
      ))}
      <div className="property-path">
        <span>{t("位置")}</span>
        <button
          title={t("复制完整路径")}
          onClick={() => void app.saveText(entry.path)}
        >
          {entry.path}
        </button>
      </div>
    </div>
  );
}
function FolderTree({
  rows,
}: {
  rows: { entry: Entry; children: { entry: Entry; children: any[] }[] }[];
}) {
  const app = useApp();
  const [expanded, setExpanded] = useState<string[]>([]);
  return (
    <div className="folder-tree">
      {rows.map(({ entry, children }) => (
        <div key={entry.path}>
          <button
            onClick={() =>
              isFolder(entry)
                ? setExpanded((v) =>
                    v.includes(entry.path)
                      ? v.filter((x) => x !== entry.path)
                      : [...v, entry.path],
                  )
                : app.open(entry)
            }
            onDoubleClick={() => app.open(entry)}
          >
            {isFolder(entry) ? (
              expanded.includes(entry.path) ? (
                <ChevronDown size={11} />
              ) : (
                <ChevronRight size={11} />
              )
            ) : (
              <span className="tree-spacer" />
            )}
            <FileIcon entry={entry} />
            <span>{entry.name}</span>
          </button>
          {expanded.includes(entry.path) && children?.length > 0 && (
            <FolderTree rows={children} />
          )}
        </div>
      ))}
    </div>
  );
}
export function Peek({
  entry,
  entries,
  close,
  change,
}: {
  entry: Entry;
  entries: Entry[];
  close: () => void;
  change: (entry: Entry) => void;
}) {
  const app = useApp();
  const [info, setInfo] = useState(false);
  const index = entries.findIndex((e) => e.path === entry.path);
  const data = useAsync(
    (signal) => loadPreview(entry, false, signal),
    [entry.path, entry.modified, app.refresh],
  );
  const step = (n: number) => {
    const item = entries[index + n];
    if (item) change(item);
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input,textarea")) return;
      if (e.key === "Escape" || e.key === " ") {
        e.preventDefault();
        close();
      }
      if (e.key === "ArrowRight" || e.key === "ArrowDown") {
        e.preventDefault();
        step(1);
      }
      if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
        e.preventDefault();
        step(-1);
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [entry, entries]);
  return (
    <div
      className="peek-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={t("快速预览 {0}", entry.name)}
      onClick={(e) => {
        if (e.target === e.currentTarget) close();
      }}
    >
      <div className="peek-window">
        <header>
          <IconButton label={t("关闭 (Space)")} onClick={close}>
            <X size={18} />
          </IconButton>
          <span className="peek-name">{entry.name}</span>
          <span className="muted">
            {Math.max(0, index) + 1} / {entries.length}
          </span>
          <div className="flex-1" />
          {entry.kind === "markdown" && (
            <Button
              onClick={() => {
                close();
                app.navigate({
                  type: "markdown",
                  path: entry.path,
                  from: app.page,
                });
              }}
            >
              <Pencil size={13} />
              {t("编辑")}
            </Button>
          )}
          <Button onClick={() => app.open(entry)}>{t("打开")}</Button>
          <IconButton
            label={t("文件信息")}
            active={info}
            onClick={() => setInfo((v) => !v)}
          >
            <Info size={17} />
          </IconButton>
        </header>
        <div className="peek-body">
          <div className="peek-content">
            {data.error ? (
              <ErrorState error={data.error} />
            ) : (
              <PreviewContent
                entry={entry}
                data={data.data}
                loading={data.loading}
              />
            )}
            {isFolder(entry) && data.data?.children && (
              <FolderTree rows={data.data.children as any} />
            )}
          </div>
          {info && (
            <div className="peek-info">
              <Properties entry={entry} data={data.data} />
              {data.data?.warning && (
                <p className="inline-warning">{data.data.warning}</p>
              )}
            </div>
          )}
        </div>
        <IconButton
          className="peek-prev"
          label={t("上一项")}
          disabled={index <= 0}
          onClick={() => step(-1)}
        >
          <ChevronLeft size={24} />
        </IconButton>
        <IconButton
          className="peek-next"
          label={t("下一项")}
          disabled={index >= entries.length - 1}
          onClick={() => step(1)}
        >
          <ChevronRight size={24} />
        </IconButton>
      </div>
    </div>
  );
}

function PdfPages({ entry, first }: { entry: Entry; first: PreviewData }) {
  const [page, setPage] = useState(1);
  const pages = Number(first.metadata?.Pages || 1);
  const load = useAsync(
    () =>
      page === 1
        ? Promise.resolve(first)
        : request<PreviewData>("preview", { path: entry.path, page }),
    [entry.path, page],
  );
  return (
    <div className="pdf-pages">
      <div className="pdf-page-content">
        {load.loading ? (
          <Loading />
        ) : load.data?.thumbnail ? (
          <img
            className="preview-image"
            src={mediaUrl(load.data.thumbnail)}
            alt={`Page ${page}`}
          />
        ) : (
          <ErrorState
            error={load.error || first.warning || t("无法读取 PDF 页面")}
          />
        )}
      </div>
      <div className="pdf-page-controls">
        <IconButton
          label={t("上一页")}
          disabled={page <= 1}
          onClick={() => setPage((p) => p - 1)}
        >
          <ChevronLeft size={16} />
        </IconButton>
        <span>
          {page} / {pages}
        </span>
        <IconButton
          label={t("下一页")}
          disabled={page >= pages}
          onClick={() => setPage((p) => p + 1)}
        >
          <ChevronRight size={16} />
        </IconButton>
        <Button onClick={() => void desktop("open", { path: entry.path })}>
          {t("在系统应用中打开")}
        </Button>
      </div>
    </div>
  );
}
