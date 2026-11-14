import { useEffect, useState } from "react";
import { RefreshCw, Gauge } from "lucide-react";
import { t } from "../i18n";
import { desktop, native, request, errorText } from "../api";
import { useApp } from "../context";
import { useAsync } from "../hooks/useAsync";
import { previewCacheStats, clearPreviewCache } from "../lib/previews";
import { formatBytes } from "../lib/files";
import { Button } from "../components/ui";
export function ResourcePanel() {
  const app = useApp();
  const [cache, setCache] = useState(previewCacheStats());
  const usage = useAsync(
    () =>
      request<{
        pss: number | null;
        rss: number;
        processes: number;
        rows: { pid: number; name: string; pss: number | null; rss: number }[];
      }>("resource_usage"),
    [],
  );
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.hidden) return;
      setCache(previewCacheStats());
      void usage.refresh();
    }, 5000);
    return () => clearInterval(timer);
  }, [usage.refresh]);
  return (
    <div className="resource-panel">
      <div className="resource-heading">
        <Gauge size={16} />
        <strong>{t(native ? "应用资源" : "开发后端资源")}</strong>
        <span className="flex-1" />
        <Button
          onClick={() => {
            setCache(previewCacheStats());
            void usage.refresh();
          }}
        >
          <RefreshCw size={12} />
          {t("刷新")}
        </Button>
      </div>
      <div className="resource-numbers">
        <div>
          <b>{usage.data?.pss != null ? formatBytes(usage.data.pss) : "—"}</b>
          <span>{t("实际分摊内存 · PSS")}</span>
        </div>
        <div>
          <b>{usage.data ? formatBytes(usage.data.rss) : "—"}</b>
          <span>{t("驻留页合计 · RSS")}</span>
        </div>
        <div>
          <b>{formatBytes(cache.bytes)}</b>
          <span>{t("预览对象缓存 · 上限 8 MiB")}</span>
        </div>
      </div>
      <p>
        {t(
          native
            ? "包含主进程与 WebKit 子进程。RSS 会重复计算共享页；清理缓存不会中断文件任务。"
            : "浏览器预览仅统计 Rust 开发后端，不包含浏览器进程。桌面版会统计完整应用进程树。",
        )}
      </p>
      <details>
        <summary>
          {t("查看进程明细")} · {usage.data?.processes || 0}
        </summary>
        {usage.data?.rows.map((r) => (
          <div className="resource-process" key={r.pid}>
            <span>
              {r.name} · {r.pid}
            </span>
            <span>PSS {r.pss === null ? "—" : formatBytes(r.pss)}</span>
            <span>RSS {formatBytes(r.rss)}</span>
          </div>
        ))}
      </details>
      <Button
        onClick={async () => {
          try {
            clearPreviewCache();
            if (native) await desktop("release_memory");
            setCache(previewCacheStats());
            await usage.refresh();
            app.notify(t("闲置预览与网页缓存已释放"));
          } catch (e) {
            app.notify(errorText(e), true);
          }
        }}
      >
        {t("释放闲置缓存")}
      </Button>
      {usage.error && <p className="danger-text">{usage.error}</p>}
    </div>
  );
}
