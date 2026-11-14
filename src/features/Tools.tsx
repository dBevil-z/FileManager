import { t } from "../i18n";
import { useEffect, useMemo, useState } from "react";
import {
  Archive,
  ArrowDownToLine,
  ArrowLeft,
  BatteryMedium,
  Check,
  ChevronLeft,
  ChevronRight,
  Code2,
  Copy,
  Cpu,
  Download,
  Files,
  HardDrive,
  Layers,
  LoaderCircle,
  Mail,
  Monitor,
  Package,
  RefreshCw,
  Search,
  ShieldCheck,
  Sparkles,
  Trash2,
  Video,
  X,
} from "lucide-react";
import { useApp } from "../context";
import { request, desktop } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Entry, Job } from "../types";
import { formatBytes, shortDate } from "../lib/files";
import { FileIcon } from "../components/FileIcon";
import {
  Badge,
  Button,
  Empty,
  ErrorState,
  IconButton,
  Loading,
  Select,
} from "../components/ui";
const catIcons = [
  Code2,
  Files,
  Download,
  Mail,
  Package,
  HardDrive,
  Archive,
  Video,
  Download,
];
export function Clean({
  scope,
  initialCategory,
}: {
  scope?: string;
  initialCategory?: string;
}) {
  const app = useApp();
  const [result, setResult] = useState<any>(null);
  const [job, setJob] = useState<string | null>(null);
  const [progress, setProgress] = useState<Job | null>(null);
  const [category, setCategory] = useState(initialCategory || "duplicates");
  const [offset, setOffset] = useState(0);
  const pageSize = 150;
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const sys = useAsync(
    () => request<any>("system_info", { hide_names: true }),
    [],
  );
  useEffect(() => {
    if (!job) return;
    const localValue = setInterval(
      () =>
        void request<Job[]>("jobs", { result_id: job }).then((v) => {
          const current = v.find((j) => j.id === job);
          if (!current) return;
          setProgress(current);
          if (current.status === "done") {
            if (current.result_expired)
              setError(t("扫描结果已释放，请重新扫描"));
            setResult(current.result);
            setChosen(new Set());
            setOffset(0);
            setJob(null);
          } else if (["failed", "cancelled"].includes(current.status)) {
            setError(current.error || t("扫描已取消"));
            setJob(null);
          }
        }),
      700,
    );
    return () => clearInterval(localValue);
  }, [job]);
  const cats: any[] =
    result?.categories ||
    [
      t("开发与应用缓存"),
      t("重复文件"),
      t("安装包"),
      t("邮件附件"),
      t("应用残留"),
      t("大文件"),
      t("闲置应用"),
      t("录屏"),
      t("旧下载"),
    ].map((name, i) => ({
      id: [
        "caches",
        "duplicates",
        "installers",
        "mail",
        "leftovers",
        "large",
        "idle",
        "recordings",
        "old",
      ][i],
      name,
      items: [],
      reclaimable: null,
    }));
  const current = cats.find((c) => c.id === category);
  const disk = sys.data?.home_disk;
  const currentChosen =
    current?.items.filter((i: any) => chosen.has(i.entry.path)) || [];
  const bytes = currentChosen.reduce(
    (n: number, i: any) => n + (i.entry.size || 0),
    0,
  );
  const scan = async (custom = false) => {
    setError("");
    try {
      let roots: string[] | undefined = scope ? [scope] : undefined;
      if (custom) {
        roots = await app.pickFiles({ directories: true, multiple: true });
        if (!roots.length) return;
      }
      const result = await request<{ id: string }>("job_start", {
        op: "clean_scan",
        args: { roots },
      });
      setJob(result.id);
    } catch (e) {
      setError(String(e));
    }
  };
  useEffect(() => {
    if (scope) void scan();
  }, [scope]);
  return (
    <div className="feature-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>{t("给空间，留点空间。")}</h2>
          <p>{t("先了解存储，再决定什么值得保留。")}</p>
        </div>
        <Button disabled={!!job} onClick={() => void scan(true)}>
          {t("选择扫描目录")}
        </Button>
        <Button tone="primary" disabled={!!job} onClick={() => void scan()}>
          {job ? (
            <LoaderCircle className="spin" size={14} />
          ) : (
            <Sparkles size={14} />
          )}
          {job ? t("正在扫描…") : result ? t("重新扫描") : t("开始扫描")}
        </Button>
      </div>
      <div className="clean-storage">
        <div className="storage-icon">
          <HardDrive size={31} strokeWidth={1.4} />
        </div>
        <div>
          <h3>{t("本地存储")}</h3>
          <p>
            {disk
              ? t(
                  "{0} 已使用 · {1} 可用，共 {2}",
                  formatBytes(disk.used),
                  formatBytes(disk.free),
                  formatBytes(disk.total),
                )
              : t("正在读取磁盘容量…")}
          </p>
          <div className="storage-bar">
            <span
              style={{
                width: disk ? `${(disk.used / disk.total) * 100}%` : "0%",
                background: "linear-gradient(90deg,#a28bed,#7a9dda)",
              }}
            />
          </div>
        </div>
      </div>
      {job && (
        <div
          className="inline-warning"
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            marginBottom: 20,
          }}
        >
          <LoaderCircle size={16} className="spin" />
          <span className="flex-1">
            {progress?.current || t("正在检查文件…")}
            {t(" · 已检查")} {progress?.completed || 0}
            {t(" 项")}
          </span>
          <Button onClick={() => void request("job_cancel", { id: job })}>
            {t("取消")}
          </Button>
        </div>
      )}
      {error && (
        <p className="inline-warning" style={{ marginBottom: 20 }}>
          {error}
        </p>
      )}
      <div className="clean-categories">
        {cats.map((c, i) => {
          const Icon = catIcons[i];
          return (
            <button
              key={c.id}
              className={`clean-category ${result && c.id === category ? "active" : ""}`}
              onClick={() => {
                setCategory(c.id);
                setChosen(new Set());
                setOffset(0);
              }}
            >
              <div>
                <Icon size={20} />
                <span>{t(c.name)}</span>
              </div>
              <strong>
                {c.reclaimable == null ? "—" : formatBytes(c.reclaimable)}
              </strong>
              <span>
                {result
                  ? t("{0} 个待核对项目", c.items.length)
                  : t("扫描后查看")}
              </span>
            </button>
          );
        })}
      </div>
      {result && current && (
        <section className="clean-review">
          <header>
            <h3>{t(current.name)}</h3>
            <span className="muted" style={{ fontSize: 10 }}>
              {t("已选 ")}
              {currentChosen.length}
              {t(" 项 · ")}
              {formatBytes(bytes)}
            </span>
            <Button
              tone="danger"
              disabled={!currentChosen.length}
              onClick={async () => {
                if (
                  !(await app.confirm(
                    t("移到回收站？"),
                    t(
                      "将所选 {0} 个文件移到回收站。可以撤销本次操作。",
                      currentChosen.length,
                    ),
                    true,
                  ))
                )
                  return;
                try {
                  await request("job_start", {
                    op: "clean_apply",
                    args: {
                      items: currentChosen,
                      groups: current.groups,
                      category,
                    },
                  });
                  setChosen(new Set());
                  app.notify(t("清理任务已加入队列"));
                } catch (e) {
                  app.notify(String(e), true);
                }
              }}
            >
              <Trash2 size={13} />
              {t("移到回收站")}
            </Button>
          </header>
          {current.note && <p className="muted">{t(current.note)}</p>}
          {category === "leftovers" && (
            <Button onClick={() => app.navigate({ type: "packages" })}>
              {t("在软件包中查看残留")}
            </Button>
          )}
          {!current.items.length ? (
            <Empty icon={<Check size={24} />} title={t("没有发现待处理项目")}>
              {current.note
                ? t(current.note)
                : t("当前扫描范围内没有匹配的文件。")}
            </Empty>
          ) : (
            <div className="table-scroll">
              <table className="data-table">
                <thead>
                  <tr>
                    <th style={{ width: 35 }} />
                    <th style={{ width: "37%" }}>{t("名称")}</th>
                    <th>{t("建议与原因")}</th>
                    <th style={{ width: 90 }}>{t("大小")}</th>
                  </tr>
                </thead>
                <tbody>
                  {current.items
                    .slice(offset, offset + pageSize)
                    .map((i: any) => (
                      <tr
                        key={i.entry.path}
                        onClick={() => {
                          if (!i.keeper)
                            setChosen((v) => {
                              const next = new Set(v);
                              next.has(i.entry.path)
                                ? next.delete(i.entry.path)
                                : next.add(i.entry.path);
                              return next;
                            });
                        }}
                      >
                        <td>
                          <input
                            aria-label={t("选择 {0}", i.entry.name)}
                            type="checkbox"
                            checked={chosen.has(i.entry.path)}
                            disabled={i.keeper}
                            readOnly
                          />
                        </td>
                        <td>
                          <div>
                            <FileIcon entry={i.entry} />
                            <div
                              style={{
                                minWidth: 0,
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                              }}
                              title={i.entry.path}
                            >
                              {i.entry.name}
                              <small>{i.entry.path}</small>
                            </div>
                          </div>
                        </td>
                        <td title={i.reason}>
                          {i.keeper ? (
                            <Badge color="#6bb99c">{t("保留此副本")}</Badge>
                          ) : (
                            t(i.reason)
                          )}
                        </td>
                        <td>{formatBytes(i.entry.size)}</td>
                      </tr>
                    ))}
                </tbody>
              </table>
              {current.items.length > pageSize && (
                <div className="package-pagination">
                  <IconButton
                    label={t("上一页")}
                    disabled={offset === 0}
                    onClick={() => setOffset(Math.max(0, offset - pageSize))}
                  >
                    <ChevronLeft size={15} />
                  </IconButton>
                  <span>
                    {offset + 1}–
                    {Math.min(current.items.length, offset + pageSize)} /{" "}
                    {current.items.length}
                  </span>
                  <IconButton
                    label={t("下一页")}
                    disabled={offset + pageSize >= current.items.length}
                    onClick={() => setOffset(offset + pageSize)}
                  >
                    <ChevronRight size={15} />
                  </IconButton>
                </div>
              )}
            </div>
          )}
        </section>
      )}
      {result?.truncated && (
        <p className="inline-warning" style={{ marginTop: 20 }}>
          {t(
            "已达到本次扫描的数量、时长或读取上限，结果仅包含已检查的文件。可选择更小的目录继续扫描。",
          )}
        </p>
      )}
      {result?.errors?.length > 0 && (
        <details style={{ marginTop: 20 }}>
          <summary className="muted">
            {result.errors.length}
            {t(" 个位置无法读取")}
          </summary>
          <pre className="code-preview">{result.errors.join("\n")}</pre>
        </details>
      )}
    </div>
  );
}
export function Packages() {
  const app = useApp();
  const load = useAsync(() => request<any>("packages"), [app.refresh]);
  const [query, setQuery] = useState("");
  const [manager, setManager] = useState("all");
  const [reason, setReason] = useState("all");
  const [sort, setSort] = useState("name");
  const [selected, setSelected] = useState<any>(null);
  const [page, setPage] = useState(0);
  const [tab, setTab] = useState("installed");
  const items = useMemo(
    () =>
      (load.data?.items || [])
        .filter(
          (p: any) =>
            (manager === "all" || p.manager === manager) &&
            (reason === "all" || p.reason === reason) &&
            `${p.name} ${p.description}`
              .toLowerCase()
              .includes(query.toLowerCase()),
        )
        .sort((a: any, b: any) =>
          sort === "size"
            ? (b.size || 0) - (a.size || 0)
            : a.name.localeCompare(b.name),
        ),
    [load.data, query, manager, reason, sort],
  );
  useEffect(() => setPage(0), [query, manager, reason, sort]);
  const reasons: Record<string, string> = {
    manual: t("手动安装"),
    dependency: t("依赖"),
    residual: t("残留配置"),
    old_version: t("旧版本"),
    unknown: t("未知"),
  };
  return (
    <div className="tool-layout">
      <div className="tool-main">
        <div className="page-heading">
          <div className="heading-copy">
            <h2>{t("软件包")}</h2>
            <p>{t("看清本机安装了什么，以及它们来自哪里。")}</p>
          </div>
          <IconButton
            label={t("刷新软件包")}
            onClick={() => void load.refresh()}
          >
            <RefreshCw size={16} />
          </IconButton>
          <Button
            disabled={!items.length}
            onClick={() =>
              void exportText(
                "filemanager-packages.sh",
                `#!/usr/bin/env bash\n# FileManager exported package list. Review before running.\n${[...new Set(items.map((p: any) => p.install_command))].join("\n")}\n`,
                app,
              )
            }
          >
            <ArrowDownToLine size={13} />
            {t("导出清单")}
          </Button>
        </div>
        <div className="stat-grid">
          <Stat
            label={t("已安装软件包")}
            value={load.data ? String(load.data.total) : "—"}
            icon={<Package size={14} />}
          />
          <Stat
            label={t("已知磁盘占用")}
            value={formatBytes(load.data?.total_size)}
            icon={<HardDrive size={14} />}
            note={t("未知大小不计入")}
          />
          <Stat
            label={t("已识别来源")}
            value={load.data ? String(load.data.managers.length) : "—"}
            icon={<Layers size={14} />}
          />
        </div>
        <div className="page-tabs">
          <button
            className={tab === "installed" ? "active" : ""}
            onClick={() => setTab("installed")}
          >
            {t("已安装")}
          </button>
          <button
            className={tab === "caches" ? "active" : ""}
            onClick={() => setTab("caches")}
          >
            {t("软件缓存")}
          </button>
          <button
            className={tab === "versions" ? "active" : ""}
            onClick={() => {
              setTab("versions");
              setReason("old_version");
            }}
          >
            {t("多个版本")}
          </button>
        </div>
        {tab === "caches" ? (
          <>
            {load.data?.caches.map((c: any) => (
              <div className="server-row" key={t(c.name)}>
                <Archive size={19} />
                <div className="flex-1">
                  <strong>{t(c.name)}</strong>
                  <p>{c.path}</p>
                </div>
                <Button onClick={() => void app.saveText(c.command)}>
                  <Copy size={12} />
                  {t("复制清理命令")}
                </Button>
              </div>
            ))}
            <p className="muted" style={{ fontSize: 11 }}>
              {t("清理命令仅复制。执行前请在终端核对。")}
            </p>
          </>
        ) : (
          <>
            <div className="page-toolbar">
              <div className="search-input">
                <Search size={14} />
                <input
                  placeholder={t("搜索软件包…")}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
              </div>
              <Select
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                aria-label={t("安装原因")}
              >
                <option value="all">{t("所有类型")}</option>
                {Object.entries(reasons).map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </Select>
              <div className="flex-1" />
              <Select
                value={sort}
                onChange={(e) => setSort(e.target.value)}
                aria-label={t("软件包排序")}
              >
                <option value="name">{t("名称")}</option>
                <option value="size">{t("大小")}</option>
              </Select>
            </div>
            <div className="manager-filter">
              <button
                className={manager === "all" ? "active" : ""}
                onClick={() => setManager("all")}
              >
                {t("全部")}
              </button>
              {load.data?.managers.map((m: string) => (
                <button
                  key={m}
                  className={manager === m ? "active" : ""}
                  onClick={() => setManager(m)}
                >
                  {m}
                </button>
              ))}
            </div>
            {load.loading && !load.data ? (
              <Loading label={t("正在读取本机软件包…")} />
            ) : load.error ? (
              <ErrorState
                error={load.error}
                retry={() => void load.refresh()}
              />
            ) : !items.length ? (
              <Empty
                icon={<Package size={27} />}
                title={t("没有匹配的软件包")}
              />
            ) : (
              <>
                <table className="data-table">
                  <thead>
                    <tr>
                      <th style={{ width: "40%" }}>{t("名称")}</th>
                      <th>{t("版本")}</th>
                      <th style={{ width: 80 }}>{t("来源")}</th>
                      <th style={{ width: 85 }}>{t("大小")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items.slice(page * 100, page * 100 + 100).map((p: any) => (
                      <tr
                        key={p.id}
                        className={selected?.id === p.id ? "selected" : ""}
                        onClick={() => setSelected(p)}
                      >
                        <td>
                          <div>
                            <span className="app-icon">
                              {p.name.charAt(0).toUpperCase()}
                            </span>
                            <div
                              style={{
                                overflow: "hidden",
                                textOverflow: "ellipsis",
                              }}
                            >
                              {p.name}
                              <small>{reasons[p.reason]}</small>
                            </div>
                          </div>
                        </td>
                        <td title={p.version}>{p.version}</td>
                        <td>
                          <Badge>{p.manager}</Badge>
                        </td>
                        <td>{formatBytes(p.size)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div className="page-toolbar" style={{ marginTop: 15 }}>
                  <span className="muted" style={{ fontSize: 10 }}>
                    {items.length}
                    {t(" 个软件包 · 第 ")}
                    {page + 1} / {Math.ceil(items.length / 100)}
                    {t(" 页")}
                  </span>
                  <div className="flex-1" />
                  <IconButton
                    label={t("上一页")}
                    disabled={!page}
                    onClick={() => setPage((v) => v - 1)}
                  >
                    <ChevronLeft size={15} />
                  </IconButton>
                  <IconButton
                    label={t("下一页")}
                    disabled={(page + 1) * 100 >= items.length}
                    onClick={() => setPage((v) => v + 1)}
                  >
                    <ChevronRight size={15} />
                  </IconButton>
                </div>
              </>
            )}
          </>
        )}
        {load.data?.errors.length > 0 && (
          <details>
            <summary className="muted" style={{ fontSize: 10 }}>
              {t("部分来源读取失败（")}
              {load.data.errors.length}）
            </summary>
            <pre className="code-preview">
              {load.data.errors
                .map((e: any) => `${e.manager}: ${e.error}`)
                .join("\n")}
            </pre>
          </details>
        )}
      </div>
      <aside className="tool-detail">
        {selected ? (
          <>
            <span
              className="app-icon"
              style={{ width: 58, height: 58, borderRadius: 14, fontSize: 26 }}
            >
              {selected.name.charAt(0).toUpperCase()}
            </span>
            <h3>{selected.name}</h3>
            <p>{selected.description}</p>
            <div className="properties">
              <h4>{t("软件信息")}</h4>
              {[
                [t("版本"), selected.version],
                [t("来源"), selected.manager],
                [t("安装类型"), reasons[selected.reason]],
                [t("占用空间"), formatBytes(selected.size)],
                [t("上次使用"), t("未知")],
              ].map(([l, v]) => (
                <div key={l}>
                  <span>{l}</span>
                  <b>{v}</b>
                </div>
              ))}
            </div>
            <Button
              onClick={() => void app.saveText(selected.uninstall_command)}
            >
              <Copy size={13} />
              {t("复制卸载命令")}
            </Button>
            <Button onClick={() => void app.saveText(selected.install_command)}>
              <Copy size={13} />
              {t("复制安装命令")}
            </Button>
            <p className="inline-warning" style={{ marginTop: 20 }}>
              {t("请在终端检查命令及其影响后执行。")}
            </p>
          </>
        ) : (
          <Empty icon={<Package size={28} />} title={t("软件包信息")}>
            {t("选择一个软件包查看详情。")}
          </Empty>
        )}
      </aside>
    </div>
  );
}
export function SystemPage() {
  const app = useApp();
  const [hide, setHide] = useState(true);
  const data = useAsync(
    () => request<any>("system_info", { hide_names: hide }),
    [hide, app.refresh],
  );
  const s = data.data;
  return (
    <div className="feature-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>{t("关于本机")}</h2>
          <p>{t("你的电脑，一目了然。")}</p>
        </div>
        <label className="check-label">
          <input
            type="checkbox"
            checked={hide}
            onChange={(e) => setHide(e.target.checked)}
          />
          {t("隐藏名称")}
        </label>
        <IconButton
          label={t("刷新系统信息")}
          onClick={() => void data.refresh()}
        >
          <RefreshCw size={16} />
        </IconButton>
        <Button
          disabled={!s}
          onClick={() => void app.saveText(JSON.stringify(s, null, 2))}
        >
          <Copy size={13} />
          {t("复制信息")}
        </Button>
      </div>
      {data.error ? (
        <ErrorState error={data.error} />
      ) : !s ? (
        <Loading />
      ) : (
        <>
          <div className="system-banner">
            <div className="computer-symbol">
              <Monitor size={86} strokeWidth={1.1} />
            </div>
            <div>
              <h2>{hide ? t("我的电脑") : s.hostname || t("我的电脑")}</h2>
              <p>
                {s.os} · {s.kernel}
              </p>
              <p>{s.cpu}</p>
            </div>
          </div>
          <div className="system-grid">
            <InfoCard
              title={t("处理器")}
              icon={<Cpu size={17} />}
              rows={[
                [t("型号"), s.cpu || t("未知")],
                [t("逻辑核心"), String(s.threads)],
                [
                  t("已运行"),
                  s.uptime_seconds
                    ? t("{0} 小时", Math.floor(s.uptime_seconds / 3600))
                    : t("未知"),
                ],
              ]}
            />
            <InfoCard
              title={t("内存")}
              icon={<Layers size={17} />}
              rows={[
                [t("物理内存"), formatBytes(s.memory.total)],
                [t("当前可用"), formatBytes(s.memory.available)],
                [t("交换空间"), formatBytes(s.memory.swap_total)],
              ]}
            />
            {s.disks.map((d: any) => (
              <div className="info-card" key={d.path}>
                <h3>
                  <HardDrive size={17} />
                  {d.path === "/" ? t("系统磁盘") : d.name}
                </h3>
                <div>
                  <span>{t("文件系统")}</span>
                  <strong>{d.filesystem}</strong>
                </div>
                <div>
                  <span>{t("容量")}</span>
                  <strong>{formatBytes(d.disk?.total)}</strong>
                </div>
                <div className="storage-bar">
                  <span
                    style={{
                      width: d.disk
                        ? `${(d.disk.used / d.disk.total) * 100}%`
                        : "0%",
                      background: "var(--accent)",
                    }}
                  />
                </div>
                <div>
                  <span>{t("可用空间")}</span>
                  <strong>{formatBytes(d.disk?.free)}</strong>
                </div>
              </div>
            ))}
            <InfoCard
              title={t("安全状态")}
              icon={<ShieldCheck size={17} />}
              rows={[
                [
                  "Secure Boot",
                  s.security.secure_boot == null
                    ? t("未能读取")
                    : s.security.secure_boot
                      ? t("已启用")
                      : t("未启用"),
                ],
                [
                  "AppArmor",
                  s.security.apparmor == null
                    ? t("未知")
                    : s.security.apparmor
                      ? t("已启用")
                      : t("未启用"),
                ],
                [
                  t("磁盘加密"),
                  containsCrypt(s.security.disk_encryption)
                    ? t("检测到加密卷")
                    : t("当前信息无法确定"),
                ],
              ]}
            />
            <InfoCard
              title={t("显示器")}
              icon={<Monitor size={17} />}
              rows={
                s.displays?.length
                  ? s.displays.map((d: string, i: number) => [
                      t("显示器 {0}", i + 1),
                      d,
                    ])
                  : [[t("显示信息"), t("当前会话未提供")]]
              }
            />
            {s.batteries.length ? (
              s.batteries.map((b: any) => (
                <InfoCard
                  key={b.name}
                  title={t("电池")}
                  icon={<BatteryMedium size={17} />}
                  rows={[
                    [b.name, b.capacity == null ? t("未知") : `${b.capacity}%`],
                    [t("状态"), b.status || t("未知")],
                  ]}
                />
              ))
            ) : (
              <InfoCard
                title={t("电源")}
                icon={<BatteryMedium size={17} />}
                rows={[[t("电池"), t("未检测到电池")]]}
              />
            )}
          </div>
        </>
      )}
    </div>
  );
}
function containsCrypt(v: unknown): boolean {
  return JSON.stringify(v || {}).includes('"crypt"');
}
export function TrashPage() {
  const app = useApp();
  const data = useAsync(() => request<any>("list_trash"), [app.refresh]);
  const [chosen, setChosen] = useState<string[]>([]);
  return (
    <div className="feature-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>{t("回收站")}</h2>
          <p>{t("误删的文件，还有回来的一条路。")}</p>
        </div>
        <Button
          disabled={!chosen.length}
          onClick={async () => {
            for (const id of chosen) {
              try {
                await request("restore_trash", { info: id });
              } catch (e) {
                app.notify(String(e), true);
              }
            }
            setChosen([]);
            app.refreshAll();
          }}
        >
          <ArrowLeft size={13} />
          {t("恢复所选")}
        </Button>
        <Button
          disabled={!chosen.length}
          onClick={async () => {
            const [destination] = await app.pickFiles({ directories: true });
            if (!destination) return;
            let retained = false;
            for (const info of chosen) {
              try {
                const result = await request<{ trash_copy_retained?: boolean }>(
                  "restore_trash",
                  { info, destination },
                );
                retained ||= Boolean(result.trash_copy_retained);
              } catch (e) {
                app.notify(String(e), true);
              }
            }
            if (retained) app.notify(t("已恢复到其他磁盘，回收站中保留了原件"));
            setChosen([]);
            app.refreshAll();
          }}
        >
          {t("恢复到…")}
        </Button>
        <Button
          tone="danger"
          disabled={!data.data?.items.length}
          onClick={async () => {
            if (
              await app.confirm(
                t("永久清空回收站？"),
                t("此操作将永久删除回收站中的文件，无法撤销。"),
                true,
              )
            ) {
              try {
                await request("empty_trash", { confirmation: "EMPTY" });
                app.refreshAll();
              } catch (e) {
                app.notify(String(e), true);
              }
            }
          }}
        >
          <Trash2 size={13} />
          {t("清空回收站")}
        </Button>
      </div>
      {data.loading ? (
        <Loading />
      ) : data.error ? (
        <ErrorState error={data.error} />
      ) : !data.data?.items.length ? (
        <Empty icon={<Trash2 size={30} />} title={t("回收站是空的")}>
          {t("移到回收站的文件会显示在这里。")}
        </Empty>
      ) : (
        <table className="data-table">
          <thead>
            <tr>
              <th style={{ width: 35 }} />
              <th style={{ width: "32%" }}>{t("名称")}</th>
              <th>{t("原位置")}</th>
              <th style={{ width: 100 }}>{t("删除时间")}</th>
            </tr>
          </thead>
          <tbody>
            {data.data.items.map((v: any) => {
              const id = v.item.info;
              return (
                <tr
                  key={id}
                  onClick={() =>
                    setChosen((old) =>
                      old.includes(id)
                        ? old.filter((x) => x !== id)
                        : [...old, id],
                    )
                  }
                >
                  <td>
                    <input
                      type="checkbox"
                      checked={chosen.includes(id)}
                      readOnly
                    />
                  </td>
                  <td>
                    <div>
                      <FileIcon entry={v.entry} />
                      <span>{v.entry.name}</span>
                    </div>
                  </td>
                  <td title={v.item.original}>{v.item.original}</td>
                  <td>{v.item.deleted}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
function Stat({
  label,
  value,
  icon,
  note,
}: {
  label: string;
  value: string;
  icon: React.ReactNode;
  note?: string;
}) {
  return (
    <div className="stat-card">
      <span>
        {icon}
        {label}
      </span>
      <strong>{value}</strong>
      {note && <small>{note}</small>}
    </div>
  );
}
function InfoCard({
  title,
  icon,
  rows,
}: {
  title: string;
  icon: React.ReactNode;
  rows: [string, string][];
}) {
  return (
    <div className="info-card">
      <h3>
        {icon}
        {title}
      </h3>
      {rows.map(([l, v], i) => (
        <div key={i}>
          <span>{l}</span>
          <strong>{v}</strong>
        </div>
      ))}
    </div>
  );
}
async function exportText(
  name: string,
  text: string,
  app: ReturnType<typeof useApp>,
) {
  const dirs = await app.pickFiles({ directories: true, multiple: false });
  if (!dirs[0]) return;
  const path = `${dirs[0]}/${name}`;
  let revision: string | undefined;
  try {
    const existing = await request<{ revision: string }>("read_text", { path });
    if (!(await app.confirm(t("替换导出文件？"), path))) return;
    revision = existing.revision;
  } catch {}
  try {
    await request("write_text", { path, text, revision });
    app.notify(t("已导出 {0}", name));
  } catch (e) {
    app.notify(String(e), true);
  }
}
