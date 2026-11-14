import { useEffect, useRef, useState } from "react";
import {
  Puzzle,
  Download,
  Upload,
  ShieldCheck,
  Play,
  Search,
  Trash2,
  FileText,
  Hash,
  FolderTree,
  ScanText,
  Copy,
  ExternalLink,
  CheckCircle2,
} from "lucide-react";
import { t } from "../i18n";
import { desktop, errorText, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import { useApp } from "../context";
import type { Job } from "../types";
import {
  Badge,
  Button,
  Empty,
  ErrorState,
  Field,
  Loading,
  Modal,
  Select,
  Toggle,
} from "../components/ui";
import { formatBytes } from "../lib/files";
export type PluginManifest = {
  schema_version: number;
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  category: string;
  permissions: string[];
  placements: string[];
  inputs: {
    id: string;
    label: string;
    type: "text" | "select";
    options: string[];
    default: string;
  }[];
};
export type PluginItem = {
  manifest: PluginManifest;
  official: boolean;
  installed: boolean;
  enabled: boolean;
  error?: string;
  update: boolean;
  available_version?: string;
};
export type PluginCatalog = {
  plugins: PluginItem[];
  api_version: number;
  runtime: string;
  remote_enabled: boolean;
};
type Report = { name: string; content: string };
type RunResult = {
  output: {
    title: string;
    summary?: string;
    columns?: string[];
    rows?: (string | number | null | boolean)[][];
    reports?: Report[];
  };
  elapsed_ms: number;
  runtime: string;
};
export const permissions: Record<
  string,
  { name: string; description: string }
> = {
  "context:metadata": {
    name: "读取选择与目录清单",
    description: "读取本次选择的名称、大小、时间及当前目录的一层清单。",
  },
  "files:read": {
    name: "读取所选文件",
    description: "读取本次选中文件或选中文件夹内的内容，用于校验、文本分析。",
  },
  "directories:scan": {
    name: "扫描指定目录",
    description: "递归统计当前或选中目录；不跟随符号链接，不跨文件系统。",
  },
};
const icons: Record<string, typeof Puzzle> = {
  "official.checksums": Hash,
  "official.inventory": FileText,
  "official.text-inspector": ScanText,
  "official.space-analyzer": FolderTree,
};
export function PluginsPage() {
  const app = useApp();
  const list = useAsync(
    () => request<PluginCatalog>("plugins_list"),
    [app.refresh],
  );
  const [query, setQuery] = useState("");
  const [tab, setTab] = useState<"discover" | "installed">("discover");
  const [busy, setBusy] = useState(false);
  async function review(source: Record<string, string>) {
    setBusy(true);
    try {
      const inspected = await request<{
        manifest: PluginManifest;
        sha256: string;
        script_bytes: number;
      }>("plugin_inspect", source);
      app.showModal(
        <InstallReview
          info={inspected}
          source={source}
          close={() => app.showModal(null)}
          done={() => {
            app.showModal(null);
            app.refreshAll();
          }}
        />,
      );
    } catch (e) {
      app.notify(errorText(e), true);
    } finally {
      setBusy(false);
    }
  }
  async function importPackage() {
    const files = await app.pickFiles({ multiple: false });
    if (files[0]) await review({ path: files[0] });
  }
  const all = list.data?.plugins || [];
  const visible = all.filter(
    (p) =>
      (tab === "discover" || p.installed) &&
      `${p.manifest.name} ${p.manifest.description} ${p.manifest.category} ${p.manifest.id}`
        .toLowerCase()
        .includes(query.toLowerCase()),
  );
  return (
    <div className="feature-page plugins-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>{t("插件中心")}</h2>
          <p>{t("让文件管理，适合你的工作方式。")}</p>
        </div>
        <Button disabled={busy} onClick={() => void importPackage()}>
          <Upload size={14} />
          {t("导入插件包")}
        </Button>
      </div>
      <section className="plugin-hero">
        <div className="plugin-hero-icon">
          <Puzzle size={34} strokeWidth={1.4} />
        </div>
        <div>
          <Badge>EXTENSIONS · API {list.data?.api_version || 1}</Badge>
          <h3>{t("小工具，大一点的可能性。")}</h3>
          <p>
            {t(
              "按需运行，关闭后释放。插件权限由宿主管理，结果使用统一界面呈现。",
            )}
          </p>
        </div>
        <div className="plugin-hero-stat">
          <b>{all.filter((p) => p.enabled).length}</b>
          <span>{t("个已启用")}</span>
        </div>
      </section>
      <div className="plugin-filter">
        <div className="feature-tabs">
          <button
            className={tab === "discover" ? "active" : ""}
            onClick={() => setTab("discover")}
          >
            {t("发现插件")}
          </button>
          <button
            className={tab === "installed" ? "active" : ""}
            onClick={() => setTab("installed")}
          >
            {t("已安装")} · {all.filter((p) => p.installed).length}
          </button>
        </div>
        <label className="plugin-search">
          <Search size={14} />
          <input
            aria-label={t("搜索插件")}
            placeholder={t("搜索名称、分类…")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
      </div>
      <p className="catalog-note">
        <ShieldCheck size={13} />
        {t("内置官方目录 · 本地安装与更新 · 联网市场尚未启用")}
      </p>
      {list.error ? (
        <ErrorState error={list.error} retry={() => void list.refresh()} />
      ) : list.loading && !list.data ? (
        <Loading />
      ) : !visible.length ? (
        <Empty icon={<Puzzle size={28} />} title={t("没有匹配的插件")}>
          {t("可以从发现页安装，或导入 .fmp / .zip 插件包。")}
        </Empty>
      ) : (
        <div className="plugin-grid">
          {visible.map((item) => {
            const m = item.manifest;
            const Icon = icons[m.id] || Puzzle;
            return (
              <article className="plugin-card" key={m.id}>
                <div className="plugin-card-top">
                  <span className="plugin-icon">
                    <Icon size={24} strokeWidth={1.5} />
                  </span>
                  <div>
                    <h3>{m.name}</h3>
                    <p>
                      {m.author} · v{m.version}
                    </p>
                  </div>
                  <Badge>{m.category}</Badge>
                </div>
                <p className="plugin-description">{m.description}</p>
                <div className="plugin-permissions">
                  {m.permissions.map((p) => (
                    <span key={p} title={permissions[p]?.description}>
                      {permissions[p]?.name || p}
                    </span>
                  ))}
                </div>
                {item.error && <p className="inline-warning">{item.error}</p>}
                <div className="plugin-card-bottom">
                  {item.installed ? (
                    <>
                      <Toggle
                        label={t("启用 {0}", m.name)}
                        checked={item.enabled}
                        onChange={(enabled) =>
                          void request("plugin_manage", {
                            id: m.id,
                            action: "enable",
                            enabled,
                          })
                            .then(() => app.refreshAll())
                            .catch((e) => app.notify(errorText(e), true))
                        }
                      />
                      <span className="muted">
                        {item.enabled ? t("已启用") : t("已停用")}
                      </span>
                      <span className="flex-1" />
                      <button
                        className="plugin-remove"
                        title={t("卸载插件")}
                        aria-label={t("卸载 {0}", m.name)}
                        onClick={async () => {
                          if (
                            await app.confirm(
                              t("卸载插件"),
                              t("卸载 {0}？原文件和导出的报告会保留。", m.name),
                              true,
                            )
                          )
                            try {
                              await request("plugin_manage", {
                                id: m.id,
                                action: "remove",
                              });
                              app.refreshAll();
                            } catch (e) {
                              app.notify(errorText(e), true);
                            }
                        }}
                      >
                        <Trash2 size={14} />
                      </button>
                      {item.update && (
                        <Button onClick={() => void review({ builtin: m.id })}>
                          {t("更新")}
                        </Button>
                      )}
                      <Button
                        disabled={!item.enabled}
                        onClick={() => app.runPlugin(m.id)}
                      >
                        <Play size={12} />
                        {t("运行")}
                      </Button>
                    </>
                  ) : (
                    <>
                      <Badge>
                        {item.official ? t("随应用提供") : t("本地插件")}
                      </Badge>
                      <span className="flex-1" />
                      <Button
                        tone="primary"
                        disabled={busy}
                        onClick={() => void review({ builtin: m.id })}
                      >
                        <Download size={12} />
                        {t("安装")}
                      </Button>
                    </>
                  )}
                </div>
              </article>
            );
          })}
        </div>
      )}
      <section className="plugin-developer">
        <div>
          <h3>{t("开发自己的插件")}</h3>
          <p>
            {t(
              "manifest.json + main.js，无需重新编译主程序。SDK v1 提供选择上下文、流式校验、文本读取、目录统计与报告输出。",
            )}
          </p>
        </div>
        <Button
          onClick={() =>
            app.showModal(<PluginHelp close={() => app.showModal(null)} />)
          }
        >
          <FileText size={14} />
          {t("开发指南")}
        </Button>
      </section>
    </div>
  );
}
function InstallReview({
  info,
  source,
  close,
  done,
}: {
  info: { manifest: PluginManifest; sha256: string; script_bytes: number };
  source: Record<string, string>;
  close: () => void;
  done: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  return (
    <Modal
      title={t("审核并安装插件")}
      onClose={() => {
        if (!busy) close();
      }}
      footer={
        <>
          <Button disabled={busy} onClick={close}>
            {t("取消")}
          </Button>
          <Button
            disabled={busy}
            tone="primary"
            onClick={async () => {
              setBusy(true);
              try {
                await request("plugin_install", {
                  ...source,
                  sha256: info.sha256,
                  grants: info.manifest.permissions,
                });
                done();
              } catch (e) {
                setError(errorText(e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {busy ? t("正在安装…") : t("授权并安装")}
          </Button>
        </>
      }
    >
      <h3>
        {info.manifest.name} <Badge>v{info.manifest.version}</Badge>
      </h3>
      <p className="plugin-review-description">{info.manifest.description}</p>
      <p className="muted">
        {info.manifest.author} · {formatBytes(info.script_bytes)}
      </p>
      <div className="permission-review">
        {info.manifest.permissions.map((p) => (
          <div key={p}>
            <ShieldCheck size={18} />
            <div>
              <strong>{permissions[p]?.name || p}</strong>
              <p>{permissions[p]?.description}</p>
            </div>
          </div>
        ))}
      </div>
      <p className="muted">
        {t(
          "插件无法直接访问网络、启动进程或修改文件。报告仅在你点击导出后保存。每次运行限时 30 秒，JavaScript 堆限额 32 MiB。",
        )}
      </p>
      <details className="plugin-integrity">
        <summary>SHA-256</summary>
        <code>{info.sha256}</code>
      </details>
      {error && <ErrorState error={error} />}
    </Modal>
  );
}
export function PluginRunner({
  plugin,
  directory,
  selection,
  close,
}: {
  plugin: PluginItem;
  directory: string;
  selection: string[];
  close: () => void;
}) {
  const app = useApp();
  const m = plugin.manifest;
  const [chosenPaths, setChosenPaths] = useState(selection);
  const [inputs, setInputs] = useState<Record<string, string>>(
    Object.fromEntries(m.inputs.map((f) => [f.id, f.default])),
  );
  const [job, setJob] = useState<Job | null>(null);
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const [page, setPage] = useState(0);
  const [report, setReport] = useState<Report | null>(null);
  const busy =
    starting || job?.status === "queued" || job?.status === "running";
  const alive = useRef(true);
  useEffect(
    () => () => {
      alive.current = false;
    },
    [],
  );
  useEffect(() => {
    if (!job?.id || !["queued", "running"].includes(job.status)) return;
    let done = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const jobs = await request<Job[]>("jobs", { result_id: job.id });
        const latest = jobs.find((j) => j.id === job.id);
        if (!done && latest) {
          setJob(latest);
          if (latest.result_expired) setError(t("结果已释放，请重新运行插件"));
        }
      } catch (e) {
        if (!done) setError(errorText(e));
      }
      if (!done) timer = setTimeout(() => void poll(), 600);
    };
    void poll();
    return () => {
      done = true;
      clearTimeout(timer);
    };
  }, [job?.id, job?.status]);
  const result = job?.status === "done" ? (job.result as RunResult) : undefined;
  async function save(report: Report) {
    const dirs = await app.pickFiles({
      directories: true,
      multiple: false,
      initial: directory,
    });
    if (!dirs[0]) return;
    const name = await app.prompt(
      t("导出报告"),
      report.name,
      t("同名文件不会被覆盖。"),
    );
    if (!name) return;
    if (/[\/\\\0]/.test(name) || name === "." || name === "..") {
      app.notify(t("请输入有效的文件名"), true);
      return;
    }
    try {
      await request("create_text", {
        directory: dirs[0],
        name,
        text: report.content,
      });
      app.refreshAll();
      app.notify(t("报告已保存，可在操作历史中撤销"));
    } catch (e) {
      app.notify(errorText(e), true);
    }
  }
  return (
    <Modal
      title={m.name}
      wide
      onClose={() => {
        if (job && busy)
          void request("job_cancel", { id: job.id }).catch(() => {});
        close();
      }}
      footer={
        <>
          <span className="muted">
            {t("所选 {0} 项", chosenPaths.length)} ·{" "}
            {result ? `${result.elapsed_ms} ms` : "QuickJS · 按需运行"}
          </span>
          <span className="flex-1" />
          {busy ? (
            <Button
              tone="danger"
              disabled={!job}
              onClick={() =>
                void request("job_cancel", { id: job?.id }).catch((e) =>
                  setError(errorText(e)),
                )
              }
            >
              {t("取消运行")}
            </Button>
          ) : (
            <Button onClick={close}>{t("关闭")}</Button>
          )}
          <Button
            tone="primary"
            disabled={busy}
            onClick={async () => {
              setStarting(true);
              setError("");
              setJob(null);
              setPage(0);
              setReport(null);
              try {
                const next = await request<{ id: string }>("job_start", {
                  op: "plugin_run",
                  args: { id: m.id, directory, selection: chosenPaths, inputs },
                });
                if (alive.current)
                  setJob({
                    id: next.id,
                    op: "plugin_run",
                    status: "queued",
                    completed: 0,
                    total: 0,
                    bytes: 0,
                    current: "",
                  });
                else
                  void request("job_cancel", { id: next.id }).catch(() => {});
              } catch (e) {
                setError(errorText(e));
              } finally {
                if (alive.current) setStarting(false);
              }
            }}
          >
            <Play size={13} />
            {result ? t("重新运行") : t("运行插件")}
          </Button>
        </>
      }
    >
      <p className="plugin-review-description">{m.description}</p>
      <div className="plugin-inputs">
        {m.inputs.map((f) => (
          <Field key={f.id} label={f.label}>
            {f.type === "select" ? (
              <Select
                value={inputs[f.id]}
                disabled={busy}
                onChange={(e) =>
                  setInputs({ ...inputs, [f.id]: e.target.value })
                }
              >
                {f.options.map((o) => (
                  <option key={o}>{o}</option>
                ))}
              </Select>
            ) : (
              <input
                value={inputs[f.id]}
                disabled={busy}
                maxLength={4096}
                onChange={(e) =>
                  setInputs({ ...inputs, [f.id]: e.target.value })
                }
              />
            )}
          </Field>
        ))}
      </div>
      <p className="plugin-context">{directory}</p>
      <div className="plugin-scope-actions">
        <Button
          disabled={busy}
          onClick={async () => {
            const paths = await app.pickFiles({
              multiple: true,
              directories:
                m.permissions.includes("directories:scan") &&
                !m.permissions.includes("files:read"),
              initial: directory,
            });
            if (paths.length) {
              setChosenPaths(paths);
              setJob(null);
              setError("");
              setReport(null);
            }
          }}
        >
          {t("选择处理项目")}
        </Button>
        <span className="muted">
          {chosenPaths.length
            ? t("已选择 {0} 项", chosenPaths.length)
            : t("未选择文件；目录工具使用当前目录")}
        </span>
      </div>
      {busy && <Loading label={job?.current || t("正在处理…")} />}
      {(error || job?.error) && (
        <ErrorState error={error || job?.error || ""} />
      )}
      {result && (
        <section className="plugin-result">
          <h3>
            <CheckCircle2 size={17} />
            {result.output.title}
          </h3>
          <p className="muted">{result.output.summary}</p>
          {result.output.columns && (
            <>
              <div className="plugin-table">
                <table>
                  <thead>
                    <tr>
                      {result.output.columns.map((c, i) => (
                        <th key={i}>{c}</th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {result.output.rows
                      ?.slice(page * 100, (page + 1) * 100)
                      .map((row, i) => (
                        <tr key={i}>
                          {row.map((cell, j) => (
                            <td key={j}>{String(cell ?? "")}</td>
                          ))}
                        </tr>
                      ))}
                  </tbody>
                </table>
              </div>
              {(result.output.rows?.length || 0) > 100 && (
                <div className="plugin-pagination">
                  <Button
                    disabled={!page}
                    onClick={() => setPage((v) => v - 1)}
                  >
                    {t("上一页")}
                  </Button>
                  <span>
                    {page + 1} /{" "}
                    {Math.ceil((result.output.rows?.length || 0) / 100)}
                  </span>
                  <Button
                    disabled={
                      (page + 1) * 100 >= (result.output.rows?.length || 0)
                    }
                    onClick={() => setPage((v) => v + 1)}
                  >
                    {t("下一页")}
                  </Button>
                </div>
              )}
            </>
          )}
          {!!result.output.reports?.length && (
            <div className="plugin-reports">
              {result.output.reports.map((r) => (
                <div key={r.name}>
                  <FileText size={14} />
                  <strong>{r.name}</strong>
                  <Button onClick={() => setReport(report === r ? null : r)}>
                    {t("预览")}
                  </Button>
                  <Button onClick={() => void app.saveText(r.content)}>
                    <Copy size={12} />
                    {t("复制")}
                  </Button>
                  <Button onClick={() => void save(r)}>
                    <Download size={12} />
                    {t("另存为")}
                  </Button>
                </div>
              ))}
            </div>
          )}
          {report && (
            <pre className="plugin-report-preview">
              {report.content.slice(0, 24000)}
              {report.content.length > 24000
                ? "\n… 预览已截断；导出包含完整内容。"
                : ""}
            </pre>
          )}
        </section>
      )}
    </Modal>
  );
}
function PluginHelp({ close }: { close: () => void }) {
  const app = useApp();
  return (
    <Modal title={t("插件开发指南")} wide onClose={close}>
      <p>
        {t(
          "插件包是仅包含 manifest.json 和 main.js 的 ZIP 文件，扩展名为 .fmp。模板与完整 API 说明位于项目 plugins/template 和 docs/plugins.md。",
        )}
      </p>
      <pre className="plugin-report-preview">{`function main(context, api) {\n  return {\n    title: '我的插件',\n    columns: ['名称', '字节'],\n    rows: context.selection.map(f => [f.name, f.size])\n  };\n}`}</pre>
      <p className="muted">
        {t(
          "API v1 为同步任务模型。可返回表格、摘要和报告；UI、导出和文件授权由宿主管理。JS 无 DOM、Node.js、网络与系统命令。",
        )}
      </p>
      <Button
        onClick={() =>
          void desktop("open_url", { url: app.settings.repositoryUrl }).catch(
            (e) => app.notify(errorText(e), true),
          )
        }
      >
        <ExternalLink size={14} />
        GitHub
      </Button>
    </Modal>
  );
}
