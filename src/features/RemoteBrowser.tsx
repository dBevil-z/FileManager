import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowLeftRight,
  Download,
  File,
  Folder,
  FolderPlus,
  RefreshCw,
  Database,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { errorText, native, request } from "../api";
import {
  Button,
  Empty,
  ErrorState,
  IconButton,
  Loading,
} from "../components/ui";
import { useApp } from "../context";
import { getLocale, t } from "../i18n";
import { formatBytes } from "../lib/files";
import type { Listing, Profile, RemoteEntry } from "./RemoteStorage";
import {
  SyncPreview,
  TransferReport,
  type TransferSummary,
} from "./RemoteStorageTools";

type Connection = { id: string; revision: number; bucket?: string };

function useRemotePages(conn: Connection, path: string) {
  const [pages, setPages] = useState<{ number: number; listing: Listing }[]>(
    [],
  );
  const [index, setIndex] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const cursor = useRef<string | null>(null);
  const active = useRef(false);
  const closeCursor = useCallback(
    (token: string | null) => {
      if (token)
        void request("remote_list_close", { id: conn.id, cursor: token }).catch(
          () => {},
        );
    },
    [conn.id],
  );
  const refresh = useCallback(async () => {
    const version = ++generation.current;
    closeCursor(cursor.current);
    cursor.current = null;
    setLoading(true);
    active.current = true;
    setError("");
    setPages([]);
    setIndex(0);
    try {
      const listing = await request<Listing>("remote_list", {
        ...conn,
        path,
        page_size: 100,
      });
      if (version !== generation.current) {
        closeCursor(listing.next_cursor);
        return;
      }
      cursor.current = listing.next_cursor;
      setPages([{ number: 1, listing }]);
    } catch (e) {
      if (version === generation.current) setError(errorText(e));
    } finally {
      if (version === generation.current) {
        setLoading(false);
        active.current = false;
      }
    }
  }, [conn.id, conn.revision, conn.bucket, path, closeCursor]);
  useEffect(() => {
    void refresh();
    return () => {
      generation.current++;
      closeCursor(cursor.current);
      cursor.current = null;
    };
  }, [refresh, closeCursor]);
  const next = async () => {
    if (active.current) return;
    if (index + 1 < pages.length) {
      setIndex(index + 1);
      return;
    }
    const token = cursor.current;
    if (!token) return;
    const version = generation.current;
    setLoading(true);
    active.current = true;
    setError("");
    try {
      const listing = await request<Listing>("remote_list", {
        ...conn,
        path,
        page_size: 100,
        cursor: token,
      });
      if (version !== generation.current) {
        closeCursor(listing.next_cursor);
        return;
      }
      cursor.current = listing.next_cursor;
      const updated = [
        ...pages,
        { number: (pages.at(-1)?.number ?? 0) + 1, listing },
      ].slice(-20);
      setPages(updated);
      setIndex(updated.length - 1);
    } catch (e) {
      if (version === generation.current) setError(errorText(e));
    } finally {
      if (version === generation.current) {
        setLoading(false);
        active.current = false;
      }
    }
  };
  return {
    listing: pages[index]?.listing,
    number: pages[index]?.number ?? 1,
    loading,
    error,
    refresh,
    next,
    previous: () => setIndex(Math.max(0, index - 1)),
    hasPrevious: index > 0,
    hasNext: index + 1 < pages.length || Boolean(cursor.current),
  };
}

export function RemoteBrowser({ profile }: { profile: Profile }) {
  const [bucket, setBucket] = useState<string | null>(null);
  const provider = profile.config.provider;
  const allBuckets =
    (provider.kind === "minio" || provider.kind === "s3") && !provider.bucket;
  if (!allBuckets) return <RemoteFiles profile={profile} />;
  if (!bucket) return <BucketList profile={profile} enter={setBucket} />;
  return (
    <>
      <nav className="remote-bucket-nav" aria-label={t("Bucket 导航")}>
        <Button onClick={() => setBucket(null)}>
          <ArrowLeft size={15} aria-hidden="true" />
          {t("所有 Bucket")}
        </Button>
        <Database size={16} aria-hidden="true" />
        <strong>{bucket}</strong>
      </nav>
      <RemoteFiles key={bucket} profile={profile} bucket={bucket} />
    </>
  );
}

function BucketList({
  profile,
  enter,
}: {
  profile: Profile;
  enter: (bucket: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [manual, setManual] = useState("");
  const data = useRemotePages(
    { id: profile.id, revision: profile.revision },
    "",
  );
  const entries =
    data.listing?.entries.filter((e) =>
      e.name.toLowerCase().includes(query.trim().toLowerCase()),
    ) ?? [];
  const date = new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "medium",
    timeStyle: "short",
  });
  const pagination = (
    <nav className="remote-pagination" aria-label={t("Bucket 分页")}>
      <Button
        disabled={data.loading || !data.hasPrevious}
        onClick={data.previous}
      >
        {t("上一页")}
      </Button>
      <span>{t("第 {0} 页", data.number)}</span>
      <Button
        disabled={data.loading || !data.hasNext}
        onClick={() => void data.next()}
      >
        {t("下一页")}
      </Button>
    </nav>
  );
  return (
    <>
      <div className="remote-toolbar">
        <h3>{t("所有 Bucket")}</h3>
        <IconButton
          label={t("刷新 Bucket 列表")}
          disabled={data.loading}
          onClick={() => void data.refresh()}
        >
          <RefreshCw size={16} />
        </IconButton>
      </div>
      <p className="muted remote-bucket-hint">
        {t("这里只显示账号有权限列出的桶。选择一个桶后浏览目录和传输文件。")}
      </p>
      <form
        className="remote-bucket-open"
        onSubmit={(e) => {
          e.preventDefault();
          if (manual.trim()) enter(manual.trim());
        }}
      >
        <label htmlFor="remote-bucket-name">{t("直接打开 Bucket")}</label>
        <input
          id="remote-bucket-name"
          value={manual}
          maxLength={63}
          required
          autoComplete="off"
          spellCheck={false}
          placeholder={t("输入已授权的 Bucket 名称")}
          onChange={(e) => setManual(e.target.value)}
        />
        <Button type="submit">{t("打开 Bucket")}</Button>
      </form>
      <div className="remote-list-filter">
        <input
          aria-label={t("筛选本页 Bucket")}
          placeholder={t("筛选本页 Bucket")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      {(data.hasNext || data.hasPrevious || data.number > 1) && pagination}
      {data.error ? (
        <ErrorState error={t(data.error)} retry={() => void data.refresh()} />
      ) : data.loading ? (
        <Loading />
      ) : !entries.length ? (
        <Empty
          icon={<Database size={26} aria-hidden="true" />}
          title={t(query ? "本页没有匹配的 Bucket" : "没有可列出的 Bucket")}
        />
      ) : (
        <div className="remote-table-wrap">
          <table className="remote-table remote-buckets-table">
            <thead>
              <tr>
                <th>Bucket</th>
                <th>{t("创建时间")}</th>
                <th>{t("操作")}</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.name}>
                  <td>
                    <button
                      type="button"
                      className="remote-entry"
                      onClick={() => enter(entry.name)}
                    >
                      <Database size={16} aria-hidden="true" />
                      <span>{entry.name}</span>
                    </button>
                  </td>
                  <td>
                    {entry.modified === null ||
                    Math.abs(entry.modified) > 8.64e15
                      ? "—"
                      : date.format(entry.modified)}
                  </td>
                  <td>
                    <Button onClick={() => enter(entry.name)}>
                      {t("打开")}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {pagination}
    </>
  );
}

function RemoteFiles({
  profile,
  bucket,
}: {
  profile: Profile;
  bucket?: string;
}) {
  const app = useApp();
  const [path, setPath] = useState("");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [sync, setSync] = useState(false);
  const [report, setReport] = useState<string | null>(null);
  const [transfer, setTransfer] = useState<{
    id: string;
    uploadPath?: string;
  } | null>(null);
  const [outcome, setOutcome] = useState<TransferSummary | null>(null);
  const conn = {
    id: profile.id,
    revision: profile.revision,
    ...(bucket ? { bucket } : {}),
  };
  const data = useRemotePages(conn, path);
  const listing = data.listing;
  const job = transfer && app.jobs.find((j) => j.id === transfer.id);
  const pending = Boolean(
    transfer && (!job || job.status === "queued" || job.status === "running"),
  );
  const jobStatus = job?.status;
  const refresh = data.refresh;
  useEffect(() => {
    if (!transfer || !jobStatus || ["queued", "running"].includes(jobStatus))
      return;
    let disposed = false;
    void request<{ id: string; result?: TransferSummary }[]>("jobs", {
      result_id: transfer.id,
    })
      .then((jobs) => {
        if (!disposed)
          setOutcome(jobs.find((j) => j.id === transfer.id)?.result ?? null);
      })
      .catch((e) => !disposed && app.notify(t(errorText(e)), true));
    if (transfer.uploadPath === path) void refresh();
    return () => {
      disposed = true;
    };
  }, [transfer?.id, jobStatus, refresh]);
  const enter = (next: string) => {
    setPath(next);
    setQuery("");
    setSelected([]);
  };
  const visible =
    listing?.entries.filter((e) =>
      e.name.toLowerCase().includes(query.trim().toLowerCase()),
    ) ?? [];
  const locked = busy || data.loading || !listing;
  const run = async (action: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    try {
      await action();
    } catch (e) {
      app.notify(t(errorText(e)), true);
    } finally {
      setBusy(false);
    }
  };
  async function choose(directories: boolean, multiple = false) {
    if (native) {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const files = await open({ directory: directories, multiple });
      return files ? (typeof files === "string" ? [files] : files) : [];
    }
    return app.pickFiles({ directories, multiple, filesOnly: !directories });
  }
  async function upload(directories: boolean) {
    const files = await choose(directories, true);
    if (!files.length) return;
    const result = await request<{ id: string }>("job_start", {
      op: "remote_transfer",
      args: { ...conn, direction: "upload", path, local_paths: files },
    });
    setOutcome(null);
    setTransfer({ id: result.id, uploadPath: path });
  }
  async function download(paths: string[]) {
    const [directory] = await choose(true);
    if (!directory) return;
    const result = await request<{ id: string }>("job_start", {
      op: "remote_transfer",
      args: { ...conn, direction: "download", paths, local_dir: directory },
    });
    setOutcome(null);
    setTransfer(result);
  }
  async function mkdir() {
    const name = await app.prompt(t("新建文件夹"));
    if (!name) return;
    await request("remote_mkdir", { ...conn, path, name });
    setSelected([]);
    await refresh();
  }
  async function remove(entry: RemoteEntry) {
    if (
      !(await app.confirm(
        t("删除远程项目"),
        t("永久删除远程项目「{0}」？此操作无法从本地回收站恢复。", entry.name),
        true,
      ))
    )
      return;
    await request("remote_delete", { ...conn, path: entry.path });
    setSelected((old) => old.filter((p) => p !== entry.path));
    await refresh();
  }
  function toggle(path: string) {
    setSelected((old) =>
      old.includes(path)
        ? old.filter((p) => p !== path)
        : old.length < 1000
          ? [...old, path]
          : old,
    );
  }
  const pieces = path ? path.slice(0, -1).split("/") : [];
  const date = new Intl.DateTimeFormat(getLocale(), {
    dateStyle: "short",
    timeStyle: "short",
  });
  const pagination = (
    <div className="remote-pagination">
      <Button
        disabled={!data.hasPrevious || data.loading}
        onClick={data.previous}
      >
        {t("上一页")}
      </Button>
      <span aria-live="polite">{t("第 {0} 页", data.number)}</span>
      <Button
        disabled={!data.hasNext || data.loading || Boolean(data.error)}
        onClick={() => void data.next()}
      >
        {t("下一页")}
      </Button>
    </div>
  );
  return (
    <>
      <div className="remote-toolbar">
        <nav aria-label={t("远程目录")} className="remote-breadcrumbs">
          <IconButton
            label={t("上级目录")}
            disabled={!path || busy}
            onClick={() =>
              enter(
                pieces.length > 1 ? pieces.slice(0, -1).join("/") + "/" : "",
              )
            }
          >
            <ArrowLeft size={16} />
          </IconButton>
          <button
            type="button"
            onClick={() => enter("")}
            aria-current={!path ? "location" : undefined}
          >
            {t("起始目录")}
          </button>
          {pieces.map((piece, i) => (
            <span key={i}>
              <span aria-hidden="true"> / </span>
              <button
                type="button"
                title={piece}
                aria-current={i === pieces.length - 1 ? "location" : undefined}
                onClick={() => enter(pieces.slice(0, i + 1).join("/") + "/")}
              >
                {piece}
              </button>
            </span>
          ))}
        </nav>
        <div className="remote-actions">
          <IconButton
            label={t("刷新")}
            disabled={data.loading}
            onClick={() => {
              setSelected([]);
              void refresh();
            }}
          >
            <RefreshCw size={16} />
          </IconButton>
          <Button disabled={locked || pending} onClick={() => setSync(true)}>
            <ArrowLeftRight size={15} aria-hidden="true" />
            {t("同步预览")}
          </Button>
          {!profile.config.read_only && (
            <>
              <Button
                disabled={locked || pending}
                onClick={() => void run(mkdir)}
              >
                <FolderPlus size={15} aria-hidden="true" />
                {t("新建文件夹")}
              </Button>
              <Button
                disabled={locked || pending}
                onClick={() => void run(() => upload(true))}
              >
                <Folder size={15} aria-hidden="true" />
                {t("上传文件夹")}
              </Button>
              <Button
                tone="primary"
                disabled={locked || pending}
                onClick={() => void run(() => upload(false))}
              >
                <Upload size={15} aria-hidden="true" />
                {t("上传文件")}
              </Button>
            </>
          )}
        </div>
      </div>
      {transfer && (
        <div className="remote-transfer" role="status">
          <div>
            <strong>
              {t(transfer.uploadPath !== undefined ? "远程上传" : "远程下载")}
            </strong>
            <span>
              {pending
                ? job?.status === "running"
                  ? t(
                      "已处理 {0}/{1} 项 · {2}",
                      job.completed,
                      job.total || "…",
                      formatBytes(job.bytes),
                    )
                  : t("等待中")
                : outcome
                  ? t(
                      "完成 {0} 项，失败 {1} 项，未处理 {2} 项",
                      outcome.completed,
                      outcome.failed,
                      outcome.pending,
                    )
                  : job?.error
                    ? t(job.error)
                    : t(jobStatus === "cancelled" ? "已取消" : "传输完成")}
            </span>
          </div>
          {pending && (
            <progress
              aria-label={t("传输进度")}
              max={job?.total || undefined}
              value={
                job?.total ? Math.min(job.total, job.completed) : undefined
              }
            />
          )}
          {outcome?.report_id && (
            <Button onClick={() => setReport(outcome.report_id)}>
              {t("查看逐项结果")}
            </Button>
          )}
          <IconButton
            label={t(pending ? "取消任务" : "关闭")}
            onClick={() =>
              pending
                ? void request("job_cancel", { id: transfer.id }).catch((e) =>
                    app.notify(t(errorText(e)), true),
                  )
                : setTransfer(null)
            }
          >
            <X size={15} />
          </IconButton>
        </div>
      )}
      <div className="remote-list-filter">
        <input
          type="search"
          aria-label={t("筛选本页")}
          placeholder={t("筛选本页")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <span className="muted">{t("本页 {0} 项", visible.length)}</span>
      </div>
      <div className="remote-selection-bar">
        <span>{t("已选择 {0} 项", selected.length)}</span>
        <Button
          disabled={!selected.length || busy || pending}
          onClick={() => void run(() => download(selected))}
        >
          <Download size={15} aria-hidden="true" />
          {t("下载所选")}
        </Button>
        <Button disabled={!selected.length} onClick={() => setSelected([])}>
          {t("清除选择")}
        </Button>
        <small className="muted">{t("目录会递归传输，同名文件保留。")}</small>
      </div>
      {selected.length >= 1000 && (
        <p className="remote-note" role="status">
          {t("已达到 1,000 项选择上限。")}
        </p>
      )}
      {(data.hasNext || data.number > 1) && pagination}
      {data.error && (
        <ErrorState error={t(data.error)} retry={() => void refresh()} />
      )}
      {data.loading ? (
        <Loading label={t("正在读取远程目录…")} />
      ) : (
        listing && (
          <>
            {listing.skipped > 0 && (
              <p className="muted remote-note">
                {t("有 {0} 个条目无法显示。", listing.skipped)}
              </p>
            )}
            {!visible.length ? (
              <Empty
                icon={<Folder size={24} aria-hidden="true" />}
                title={t(
                  query
                    ? "本页没有匹配项目"
                    : listing.next_cursor
                      ? "本页没有可显示项目"
                      : "此页为空",
                )}
              />
            ) : (
              <div className="remote-table-wrap">
                <table className="remote-table remote-selectable">
                  <colgroup>
                    <col className="remote-col-check" />
                    <col />
                    <col className="remote-col-size" />
                    <col className="remote-col-date" />
                    <col className="remote-col-actions" />
                  </colgroup>
                  <thead>
                    <tr>
                      <th scope="col">
                        <input
                          type="checkbox"
                          aria-label={t("选择本页")}
                          checked={
                            visible.length > 0 &&
                            visible.every((e) => selected.includes(e.path))
                          }
                          onChange={(e) =>
                            setSelected((old) =>
                              e.target.checked
                                ? [
                                    ...new Set([
                                      ...old,
                                      ...visible.map((e) => e.path),
                                    ]),
                                  ].slice(0, 1000)
                                : old.filter(
                                    (p) => !visible.some((e) => e.path === p),
                                  ),
                            )
                          }
                        />
                      </th>
                      <th scope="col">{t("名称")}</th>
                      <th scope="col">{t("大小")}</th>
                      <th scope="col">{t("修改时间")}</th>
                      <th scope="col">{t("操作")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visible.map((entry) => {
                      const Icon = entry.is_dir ? Folder : File;
                      return (
                        <tr
                          key={entry.path}
                          aria-selected={selected.includes(entry.path)}
                        >
                          <td>
                            <input
                              type="checkbox"
                              aria-label={t("选择 {0}", entry.name)}
                              checked={selected.includes(entry.path)}
                              onChange={() => toggle(entry.path)}
                            />
                          </td>
                          <td>
                            {entry.is_dir ? (
                              <button
                                className="remote-entry"
                                type="button"
                                title={entry.name}
                                onClick={() => enter(entry.path)}
                              >
                                <Icon size={16} aria-hidden="true" />
                                <span>{entry.name}</span>
                              </button>
                            ) : (
                              <div className="remote-entry" title={entry.name}>
                                <Icon size={16} aria-hidden="true" />
                                <span>{entry.name}</span>
                              </div>
                            )}
                          </td>
                          <td>
                            {entry.size === null
                              ? "—"
                              : formatBytes(entry.size)}
                          </td>
                          <td>
                            {entry.modified === null ||
                            Math.abs(entry.modified) > 8.64e15
                              ? "—"
                              : date.format(entry.modified)}
                          </td>
                          <td>
                            <div className="remote-row-actions">
                              <IconButton
                                label={t("下载 {0}", entry.name)}
                                disabled={busy || pending}
                                onClick={() =>
                                  void run(() => download([entry.path]))
                                }
                              >
                                <Download size={15} />
                              </IconButton>
                              {!profile.config.read_only && (
                                <IconButton
                                  label={t("删除 {0}", entry.name)}
                                  disabled={busy || pending}
                                  onClick={() => void run(() => remove(entry))}
                                >
                                  <Trash2 size={15} />
                                </IconButton>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )
      )}
      {pagination}
      {sync && (
        <SyncPreview
          profile={profile}
          bucket={bucket}
          path={path}
          choose={() => choose(true)}
          close={() => setSync(false)}
        />
      )}
      {report && (
        <TransferReport
          id={profile.id}
          bucket={bucket}
          reportId={report}
          close={() => setReport(null)}
        />
      )}
    </>
  );
}
