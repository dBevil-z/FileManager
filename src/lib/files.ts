import { t, getLocale } from "../i18n";
import type { Entry } from "../types";

export function formatBytes(
  bytes: number | null | undefined,
  decimals = 1,
): string {
  if (bytes == null) return "—";
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(
    Math.floor(Math.log(Math.abs(bytes)) / Math.log(1024)),
    4,
  );
  return `${(bytes / 1024 ** index).toLocaleString(getLocale(), { maximumFractionDigits: index === 0 ? 0 : decimals })} ${units[index]}`;
}
export function shortDate(seconds: number | null | undefined): string {
  if (!seconds) return t("未知");
  const date = new Date(seconds * 1000);
  const now = new Date();
  if (date.toDateString() === now.toDateString())
    return t(
      "今天 {0}",
      date.toLocaleTimeString(getLocale(), {
        hour: "2-digit",
        minute: "2-digit",
      }),
    );
  const yesterday = new Date();
  yesterday.setDate(now.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return t("昨天");
  return date.toLocaleDateString(getLocale(), {
    year: date.getFullYear() !== now.getFullYear() ? "numeric" : undefined,
    month: "short",
    day: "numeric",
  });
}
export function kindLabel(entry: Entry): string {
  if (entry.is_symlink)
    return entry.target_is_dir ? t("文件夹链接") : t("符号链接");
  return (
    (
      {
        folder: t("文件夹"),
        image: t("图像"),
        video: t("视频"),
        audio: t("音频"),
        pdf: t("PDF 文档"),
        markdown: "Markdown",
        document: t("文稿"),
        spreadsheet: t("电子表格"),
        presentation: t("演示文稿"),
        archive: t("压缩归档"),
        installer: t("安装包"),
        code: t("源代码"),
        text: t("文本文稿"),
        file: t("文件"),
      } as Record<string, string>
    )[entry.kind] || t("文件")
  );
}
export const isFolder = (entry: Entry) => entry.is_dir || entry.target_is_dir;
export const basename = (path: string) =>
  path === "/"
    ? t("文件系统")
    : path.replace(/\/$/, "").split("/").pop() || path;
export function groupLabel(entry: Entry): string {
  if (/\.(crdownload|download|part|partial|tmp)$/i.test(entry.name))
    return t("下载中");
  if (/screenshot|截图|屏幕快照/i.test(entry.name) && entry.kind === "image")
    return t("截图");
  return (
    (
      {
        image: t("图像"),
        video: t("视频"),
        audio: t("音频"),
        pdf: "PDF",
        document: t("文档"),
        spreadsheet: t("文档"),
        presentation: t("文档"),
        markdown: t("文档"),
        text: t("文档"),
        code: t("代码"),
        archive: t("归档"),
        installer: t("安装包"),
      } as Record<string, string>
    )[entry.kind] || t("其他")
  );
}
export const kindColors: Record<string, string> = {
  folder: "#8a73ee",
  image: "#e785ad",
  video: "#649de9",
  audio: "#e99d58",
  pdf: "#e9787b",
  document: "#6e9de3",
  spreadsheet: "#62b997",
  presentation: "#e9ab5a",
  markdown: "#8794b5",
  code: "#9c81e5",
  archive: "#cc9d61",
  installer: "#8e96a7",
};
