import { useEffect, useRef, useState, type CSSProperties } from "react";
import {
  Folder,
  File,
  FileText,
  FileCode2,
  FileArchive,
  FileSpreadsheet,
  FileImage,
  Music2,
  Film,
  Image,
  Download,
  Code2,
  BriefcaseBusiness,
  Heart,
  Camera,
  Globe2,
  Monitor,
  Home,
  Link2,
  Presentation,
  Package,
  ArrowUpRight,
  BookOpen,
  FileType2,
} from "lucide-react";
import type { Entry } from "../types";
import { useApp } from "../context";
import { request, mediaUrl } from "../api";
import { isFolder, kindColors } from "../lib/files";
const icons: Record<string, typeof File> = {
  pdf: FileText,
  text: FileText,
  markdown: FileType2,
  code: FileCode2,
  image: FileImage,
  video: Film,
  audio: Music2,
  spreadsheet: FileSpreadsheet,
  presentation: Presentation,
  archive: FileArchive,
  installer: Package,
  document: FileText,
};
const symbols: Record<string, typeof Folder> = {
  folder: Folder,
  download: Download,
  music: Music2,
  image: Image,
  code: Code2,
  work: BriefcaseBusiness,
  heart: Heart,
  camera: Camera,
  globe: Globe2,
  monitor: Monitor,
  home: Home,
  book: BookOpen,
};
export function autoSymbol(name: string) {
  if (/下载|downloads/i.test(name)) return "download";
  if (/图片|pictures|photos|images/i.test(name)) return "image";
  if (/音乐|music/i.test(name)) return "music";
  if (/桌面|desktop/i.test(name)) return "monitor";
  if (/文档|documents/i.test(name)) return "book";
  if (/项目|project|code|src|开发/i.test(name)) return "code";
  if (/视频|video/i.test(name)) return "camera";
  return "";
}
export function GlassFolder({
  name = "",
  color,
  symbol,
  count,
  small = false,
  style = "glass",
}: {
  name?: string;
  color?: string;
  symbol?: string;
  count?: number;
  small?: boolean;
  style?: string;
}) {
  const Symbol = symbols[symbol || autoSymbol(name)];
  return (
    <div
      className={`glass-folder ${small ? "small" : ""} style-${style}`}
      style={{ "--folder-color": color || "#8870ed" } as CSSProperties}
    >
      <div className="folder-back" />
      <div className="folder-paper paper-one" />
      <div className="folder-paper paper-two" />
      <div className="folder-front">
        <span>{Symbol && <Symbol strokeWidth={1.7} />}</span>
        {count != null && !small && <b>{count}</b>}
      </div>
      <div className="folder-shine" />
    </div>
  );
}
export { clearPreviewCache, loadPreview } from "../lib/previews";
export type { PreviewData } from "../lib/previews";
import { loadPreview, type PreviewData } from "../lib/previews";
export function FileIcon({
  entry,
  thumbnail = false,
  className = "",
}: {
  entry: Entry;
  thumbnail?: boolean;
  className?: string;
}) {
  const { settings, catalog } = useApp();
  const ref = useRef<HTMLDivElement>(null);
  const [preview, setPreview] = useState<PreviewData | null>(null);
  const folder = isFolder(entry);
  useEffect(() => {
    setPreview(null);
    if (
      !thumbnail ||
      settings.thumbnailMode === "icons" ||
      folder ||
      ![
        "image",
        "video",
        "pdf",
        "document",
        "spreadsheet",
        "presentation",
        "markdown",
        "text",
        "code",
      ].includes(entry.kind)
    )
      return;
    let alive = true;
    const abort = new AbortController();
    const ob = new IntersectionObserver(
      (rows) => {
        if (rows[0]?.isIntersecting) {
          ob.disconnect();
          void loadPreview(entry, true, abort.signal)
            .then((p) => {
              if (alive) setPreview(p);
            })
            .catch(() => {});
        }
      },
      { rootMargin: "120px" },
    );
    if (ref.current) ob.observe(ref.current);
    return () => {
      alive = false;
      abort.abort();
      ob.disconnect();
    };
  }, [
    entry.path,
    entry.modified,
    entry.size,
    thumbnail,
    settings.thumbnailMode,
  ]);
  const Icon = icons[entry.kind] || File;
  const custom = catalog.folderStyles[entry.path];
  return (
    <div
      ref={ref}
      className={`file-visual ${folder ? "is-folder" : ""} ${className}`}
      style={
        { "--file-color": kindColors[entry.kind] || "#9c9ca6" } as CSSProperties
      }
    >
      {folder ? (
        <GlassFolder
          name={entry.name}
          color={custom?.color || settings.folderColor}
          symbol={custom?.symbol}
          style={settings.folderStyle}
          small={!thumbnail}
        />
      ) : preview?.thumbnail ? (
        <img
          className={`file-thumbnail ${entry.kind}`}
          src={mediaUrl(preview.thumbnail)}
          alt=""
          draggable={false}
          loading="lazy"
        />
      ) : thumbnail && preview?.text ? (
        <div className="text-thumbnail">
          <span>
            {entry.kind === "markdown"
              ? "MARKDOWN"
              : entry.extension.toUpperCase()}
          </span>
          <pre>{preview.text.text.slice(0, 1400)}</pre>
        </div>
      ) : (
        <div className={`document-icon ${thumbnail ? "large" : ""}`}>
          <Icon size={thumbnail ? 36 : 19} strokeWidth={1.4} />
          {thumbnail && <span>{entry.extension.toUpperCase() || "FILE"}</span>}
        </div>
      )}
      {entry.is_symlink && (
        <span className="link-mark">
          <ArrowUpRight size={10} />
        </span>
      )}
    </div>
  );
}
export function SymbolGrid({ onChange }: { onChange: (name: string) => void }) {
  return (
    <div className="symbol-grid">
      {Object.entries(symbols).map(([name, Icon]) => (
        <button key={name} onClick={() => onChange(name)}>
          <Icon size={22} />
        </button>
      ))}
      <button onClick={() => onChange("")}>
        <Link2 size={22} />
      </button>
    </div>
  );
}
