import { useEffect, useRef, useState } from "react";
import { FolderOpen, RefreshCw } from "lucide-react";
import { errorText, request } from "../api";
import {
  Button,
  Empty,
  ErrorState,
  Field,
  Loading,
  Modal,
  Select,
} from "../components/ui";
import { useApp } from "../context";
import { useAsync } from "../hooks/useAsync";
import { t } from "../i18n";
import { formatBytes } from "../lib/files";
import type { Profile } from "./RemoteStorage";

export type TransferSummary = {
  report_id: string;
  total: number;
  completed: number;
  failed: number;
  pending: number;
  bytes: number;
};
type TransferItem = {
  path: string;
  is_dir: boolean;
  status: "done" | "failed" | "unprocessed";
  bytes: number;
  error: string | null;
};
type Report = {
  summary: TransferSummary;
  count: number;
  items: TransferItem[];
};

export function TransferReport({
  id,
  bucket,
  reportId,
  close,
}: {
  id: string;
  bucket?: string;
  reportId: string;
  close: () => void;
}) {
  const [offset, setOffset] = useState(0);
  const [failedOnly, setFailedOnly] = useState(false);
  const data = useAsync(
    () =>
      request<Report>("remote_transfer_report", {
        id,
        ...(bucket ? { bucket } : {}),
        report_id: reportId,
        offset,
        failed_only: failedOnly,
      }),
    [id, bucket, reportId, offset, failedOnly],
  );
  return (
    <Modal
      title={t("传输逐项结果")}
      wide
      onClose={close}
      footer={<Button onClick={close}>{t("关闭")}</Button>}
    >
      {data.data && (
        <p className="remote-dialog-summary" role="status">
          {t(
            "完成 {0} 项，失败 {1} 项，未处理 {2} 项",
            data.data.summary.completed,
            data.data.summary.failed,
            data.data.summary.pending,
          )}{" "}
          · {formatBytes(data.data.summary.bytes)}
        </p>
      )}
      <label className="remote-check">
        <input
          type="checkbox"
          checked={failedOnly}
          onChange={(e) => {
            setFailedOnly(e.target.checked);
            setOffset(0);
          }}
        />
        {t("只看失败项")}
      </label>
      {data.error ? (
        <ErrorState error={t(data.error)} retry={() => void data.refresh()} />
      ) : data.loading ? (
        <Loading />
      ) : (
        data.data && (
          <>
            {data.data.count > 100 && (
              <NumberedPages
                offset={offset}
                total={data.data.count}
                change={setOffset}
              />
            )}
            {!data.data.items.length ? (
              <Empty
                icon={<FolderOpen size={24} aria-hidden="true" />}
                title={t("没有匹配的项目")}
              />
            ) : (
              <div className="remote-table-wrap">
                <table className="remote-table remote-report-table">
                  <thead>
                    <tr>
                      <th scope="col">{t("路径")}</th>
                      <th scope="col">{t("结果")}</th>
                      <th scope="col">{t("详情")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.data.items.map((entry) => (
                      <tr key={entry.path}>
                        <td title={entry.path}>{entry.path}</td>
                        <td>
                          {t(
                            entry.status === "done"
                              ? "已完成"
                              : entry.status === "failed"
                                ? "失败"
                                : "未处理",
                          )}
                        </td>
                        <td>
                          {entry.error
                            ? t(entry.error)
                            : entry.is_dir
                              ? t("文件夹")
                              : formatBytes(entry.bytes)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            <NumberedPages
              offset={offset}
              total={data.data.count}
              change={setOffset}
            />
          </>
        )
      )}
    </Modal>
  );
}

type Snapshot = {
  kind: "file" | "directory" | "unsupported";
  size: number | null;
  modified: number | null;
};
type Status =
  | "local_only"
  | "remote_only"
  | "type_conflict"
  | "parent_conflict"
  | "size_differs"
  | "metadata_differs"
  | "unverified"
  | "directory"
  | "unsupported";
type Summary = {
  plan_id: string;
  total: number;
  counts: Partial<Record<Status, number>>;
  local_dir: string;
  path: string;
  direction: "upload" | "download";
};
type Difference = {
  path: string;
  status: Status;
  local: Snapshot | null;
  remote: Snapshot | null;
};
const statuses: Record<Status, string> = {
  local_only: "仅本地存在",
  remote_only: "仅远程存在",
  type_conflict: "文件与目录冲突",
  parent_conflict: "上级目录冲突",
  size_differs: "文件大小不同",
  metadata_differs: "修改时间不同",
  unverified: "大小相同，内容未校验",
  directory: "双方均为目录",
  unsupported: "符号链接或特殊文件",
};

function NumberedPages({
  offset,
  total,
  change,
}: {
  offset: number;
  total: number;
  change: (offset: number) => void;
}) {
  return (
    <div className="remote-pagination">
      <Button
        disabled={offset === 0}
        onClick={() => change(Math.max(0, offset - 100))}
      >
        {t("上一页")}
      </Button>
      <span aria-live="polite">
        {t("第 {0} 页，共 {1} 项", Math.floor(offset / 100) + 1, total)}
      </span>
      <Button
        disabled={offset + 100 >= total}
        onClick={() => change(offset + 100)}
      >
        {t("下一页")}
      </Button>
    </div>
  );
}

export function SyncPreview({
  profile,
  bucket,
  path,
  choose,
  close,
}: {
  profile: Profile;
  bucket?: string;
  path: string;
  choose: () => Promise<string[]>;
  close: () => void;
}) {
  const app = useApp();
  const [localDir, setLocalDir] = useState("");
  const [direction, setDirection] = useState<"upload" | "download">("upload");
  const [jobId, setJobId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [summary, setSummary] = useState<Summary | null>(null);
  const [status, setStatus] = useState("");
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState("");
  const [settling, setSettling] = useState(false);
  const job = jobId ? app.jobs.find((j) => j.id === jobId) : undefined;
  const jobStatus = job?.status;
  const active = Boolean(
    starting ||
    settling ||
    (jobId && (!jobStatus || ["queued", "running"].includes(jobStatus))),
  );
  const planRef = useRef<string | null>(null);
  const jobRef = useRef<string | null>(null);
  const mounted = useRef(true);
  const conn = {
    id: profile.id,
    revision: profile.revision,
    ...(bucket ? { bucket } : {}),
  };
  const page = useAsync(
    async () =>
      summary
        ? request<{ count: number; entries: Difference[] }>(
            "remote_sync_page",
            {
              ...conn,
              plan_id: summary.plan_id,
              status,
              query: search,
              offset,
            },
          )
        : null,
    [
      profile.id,
      profile.revision,
      bucket,
      summary?.plan_id,
      status,
      search,
      offset,
    ],
  );
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (jobRef.current)
        void request("job_cancel", { id: jobRef.current }).catch(() => {});
      if (planRef.current)
        void request("remote_sync_close", {
          id: profile.id,
          plan_id: planRef.current,
        }).catch(() => {});
    };
  }, [profile.id]);
  useEffect(() => {
    if (!jobId || !jobStatus || ["queued", "running"].includes(jobStatus))
      return;
    jobRef.current = null;
    setSettling(true);
    let disposed = false;
    void request<{ id: string; error?: string; result?: Summary }[]>("jobs", {
      result_id: jobId,
    })
      .then((jobs) => {
        const result = jobs.find((j) => j.id === jobId);
        if (disposed) {
          if (result?.result?.plan_id)
            void request("remote_sync_close", {
              id: profile.id,
              plan_id: result.result.plan_id,
            }).catch(() => {});
          return;
        }
        if (result?.result) {
          planRef.current = result.result.plan_id;
          setSummary(result.result);
        } else
          setError(
            t(result?.error || "扫描已结束，预览结果不可用，请重新扫描"),
          );
      })
      .catch((e) => !disposed && setError(t(errorText(e))))
      .finally(() => !disposed && setSettling(false));
    return () => {
      disposed = true;
    };
  }, [jobId, jobStatus, profile.id]);
  async function start() {
    if (active || !localDir.trim()) return;
    setStarting(true);
    setError("");
    setOffset(0);
    setStatus("");
    setQuery("");
    setSearch("");
    if (planRef.current) {
      void request("remote_sync_close", {
        id: profile.id,
        plan_id: planRef.current,
      }).catch(() => {});
      planRef.current = null;
    }
    setSummary(null);
    try {
      const result = await request<{ id: string }>("job_start", {
        op: "remote_sync_preview",
        args: { ...conn, path, local_dir: localDir, direction },
      });
      if (!mounted.current) {
        void request("job_cancel", { id: result.id });
        return;
      }
      jobRef.current = result.id;
      setJobId(result.id);
    } catch (e) {
      if (mounted.current) setError(t(errorText(e)));
    } finally {
      if (mounted.current) setStarting(false);
    }
  }
  function cell(snapshot: Snapshot | null) {
    return snapshot
      ? snapshot.kind === "directory"
        ? t("文件夹")
        : snapshot.kind === "unsupported"
          ? t("不支持")
          : snapshot.size === null
            ? "—"
            : formatBytes(snapshot.size)
      : "—";
  }
  return (
    <Modal
      title={t("目录同步预览")}
      wide
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("关闭")}</Button>
          {active ? (
            <Button
              disabled={!jobId}
              onClick={() =>
                void request("job_cancel", { id: jobId }).catch((e) =>
                  setError(t(errorText(e))),
                )
              }
            >
              {t("取消扫描")}
            </Button>
          ) : (
            <Button
              tone="primary"
              disabled={!localDir.trim()}
              onClick={() => void start()}
            >
              <RefreshCw size={15} aria-hidden="true" />
              {t(summary ? "重新扫描" : "开始扫描")}
            </Button>
          )}
        </>
      }
    >
      <form
        className="remote-sync-form"
        onSubmit={(e) => {
          e.preventDefault();
          void start();
        }}
      >
        <Field label={t("本地目录")}>
          <div className="remote-path-input">
            <input
              value={localDir}
              onChange={(e) => setLocalDir(e.target.value)}
              disabled={active}
              placeholder="~/Documents"
            />
            <Button
              disabled={active}
              onClick={() =>
                void choose()
                  .then(([value]) => value && setLocalDir(value))
                  .catch((e) => setError(t(errorText(e))))
              }
            >
              <FolderOpen size={15} aria-hidden="true" />
              {t("选择文件夹")}
            </Button>
          </div>
        </Field>
        <Field label={t("比较方向")}>
          <Select
            value={direction}
            disabled={active}
            onChange={(e) =>
              setDirection(e.target.value as "upload" | "download")
            }
          >
            <option value="upload">{t("本地 → 远程")}</option>
            <option value="download">{t("远程 → 本地")}</option>
          </Select>
        </Field>
        <p className="muted remote-sync-target">
          {t("远程目录")}：{profile.config.label} /{" "}
          {bucket ? `${bucket} / ` : ""}
          {path || t("起始目录")}
        </p>
        <p className="muted remote-sync-target">
          {t("预览只读取目录。按大小和修改时间比较，内容相同需另行校验。")}
        </p>
      </form>
      {error && (
        <p role="alert" className="remote-form-error">
          {error}
        </p>
      )}
      {active && (
        <div role="status">
          <Loading label={t("正在扫描本地与远程目录…")} />
          <p className="remote-scan-current">{job?.current || t("等待中")}</p>
        </div>
      )}
      {summary && !active && (
        <div className="remote-sync-results">
          <p className="remote-dialog-summary" role="status">
            {t(
              "已比较 {0} 项；当前方向有 {1} 个仅源端存在的项目。",
              summary.total,
              summary.counts[
                summary.direction === "upload" ? "local_only" : "remote_only"
              ] || 0,
            )}
          </p>
          <p className="muted remote-sync-target">
            {summary.local_dir} {summary.direction === "upload" ? "→" : "←"}{" "}
            {profile.config.label} / {bucket ? `${bucket} / ` : ""}
            {summary.path || t("起始目录")}
          </p>
          <form
            className="remote-sync-filter"
            onSubmit={(e) => {
              e.preventDefault();
              setSearch(query);
              setOffset(0);
            }}
          >
            <Select
              aria-label={t("比较结果筛选")}
              value={status}
              onChange={(e) => {
                setStatus(e.target.value);
                setOffset(0);
              }}
            >
              <option value="">{t("所有结果")}</option>
              {Object.entries(statuses).map(([value, label]) => (
                <option key={value} value={value}>
                  {t(label)} ({summary.counts[value as Status] || 0})
                </option>
              ))}
            </Select>
            <input
              type="search"
              aria-label={t("筛选比较路径")}
              placeholder={t("筛选比较路径")}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <Button type="submit">{t("筛选")}</Button>
          </form>
          {page.error ? (
            <ErrorState
              error={t(page.error)}
              retry={() => void page.refresh()}
            />
          ) : page.loading ? (
            <Loading />
          ) : (
            page.data && (
              <>
                {page.data.count > 100 && (
                  <NumberedPages
                    offset={offset}
                    total={page.data.count}
                    change={setOffset}
                  />
                )}
                {!page.data.entries.length ? (
                  <Empty
                    icon={<FolderOpen size={24} aria-hidden="true" />}
                    title={t("没有匹配的项目")}
                  />
                ) : (
                  <div className="remote-table-wrap">
                    <table className="remote-table remote-sync-table">
                      <thead>
                        <tr>
                          <th scope="col">{t("相对路径")}</th>
                          <th scope="col">{t("本地")}</th>
                          <th scope="col">{t("远程")}</th>
                          <th scope="col">{t("比较结果")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        {page.data.entries.map((entry) => (
                          <tr key={entry.path}>
                            <td title={entry.path}>{entry.path}</td>
                            <td>{cell(entry.local)}</td>
                            <td>{cell(entry.remote)}</td>
                            <td>{t(statuses[entry.status])}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
                <NumberedPages
                  offset={offset}
                  total={page.data.count}
                  change={setOffset}
                />
              </>
            )
          )}
        </div>
      )}
    </Modal>
  );
}
