import { t } from "../i18n";
import { useEffect, useState } from "react";
import {
  Activity,
  ArrowLeft,
  ArrowUpRight,
  Bot,
  Check,
  Code2,
  Copy,
  ExternalLink,
  FileCode2,
  Folder,
  Globe2,
  Layers,
  MoreHorizontal,
  Play,
  Plug,
  Plus,
  RefreshCw,
  Search,
  Square,
  Terminal,
  X,
} from "lucide-react";
import { useApp } from "../context";
import { desktop, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Revision } from "../types";
import {
  Badge,
  Button,
  Empty,
  ErrorState,
  Field,
  IconButton,
  Loading,
  Modal,
  Select,
} from "../components/ui";
type Config = {
  path: string;
  name: string;
  tool: string;
  personal: boolean;
  is_dir: boolean;
};
type Project = {
  path: string;
  name: string;
  global: boolean;
  tools: string[];
  configs: Config[];
  git: boolean;
};
const tools: Record<string, { id: string; color: string; url: string }> = {
  Claude: {
    id: "claude",
    color: "#d69473",
    url: "https://code.claude.com/docs/en/setup",
  },
  Codex: {
    id: "codex",
    color: "#9eb6aa",
    url: "https://developers.openai.com/codex/cli",
  },
  Gemini: {
    id: "gemini",
    color: "#8aa4ee",
    url: "https://geminicli.com/docs/get-started/installation/",
  },
  Copilot: {
    id: "copilot",
    color: "#b29bdf",
    url: "https://docs.github.com/en/copilot/concepts/agents/about-copilot-cli",
  },
  OpenCode: {
    id: "opencode",
    color: "#b5b1ca",
    url: "https://opencode.ai/docs/",
  },
  Cursor: { id: "cursor", color: "#a5a8b3", url: "https://cursor.com/docs" },
};
export function Agents() {
  const app = useApp();
  const [tab, setTab] = useState("projects");
  const [query, setQuery] = useState("");
  const [tool, setTool] = useState("all");
  const [project, setProject] = useState<Project | null>(null);
  const load = useAsync(
    () =>
      request<{
        projects: Project[];
        apps: { id: string; path: string }[];
        truncated: boolean;
      }>("agent_projects"),
    [app.catalog.agentRoots.join("\0"), app.refresh],
  );
  const projects =
    load.data?.projects.filter(
      (p) =>
        (tool === "all" || p.tools.includes(tool)) &&
        `${p.name} ${p.path}`.toLowerCase().includes(query.toLowerCase()),
    ) || [];
  const [config, setConfig] = useState<Config | null>(null);
  return (
    <div className="feature-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>AI Agents</h2>
          <p>{t("项目配置、本地用量与开发服务，放在一起。")}</p>
        </div>
        <IconButton label={t("刷新 AI 数据")} onClick={() => app.refreshAll()}>
          <RefreshCw size={16} />
        </IconButton>
        <Button
          onClick={async () => {
            const paths = await app.pickFiles({
              directories: true,
              multiple: true,
            });
            if (paths.length)
              await app.updateCatalog((v) => ({
                ...v,
                agentRoots: [...new Set([...v.agentRoots, ...paths])],
              }));
          }}
        >
          <Plus size={14} />
          {t("添加项目目录")}
        </Button>
      </div>
      <div className="page-tabs">
        {[
          ["projects", t("项目与配置")],
          ["usage", t("用量")],
          ["servers", t("本地服务")],
          ["mcp", t("MCP 服务器")],
        ].map(([value, label]) => (
          <button
            className={tab === value ? "active" : ""}
            key={value}
            onClick={() => {
              setTab(value);
              setProject(null);
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {tab === "projects" ? (
        project ? (
          <>
            <div className="page-toolbar">
              <Button onClick={() => setProject(null)}>
                <ArrowLeft size={13} />
                {t("所有项目")}
              </Button>
              <div className="flex-1" />
              <Button
                onClick={async () => {
                  const dest = await app.pickFiles({
                    directories: true,
                    multiple: false,
                  });
                  if (!dest[0]) return;
                  if (
                    await app.confirm(
                      t("复制共享配置到此项目？"),
                      t(
                        "{0}\n将复制共享规则与设置，跳过个人设置、MCP、密钥和已有文件。",
                        dest[0],
                      ),
                    )
                  ) {
                    try {
                      const v = await request<{ count: number }>("agent_copy", {
                        source: project.path,
                        destination: dest[0],
                      });
                      app.notify(t("已复制 {0} 个配置文件", v.count));
                      app.refreshAll();
                    } catch (e) {
                      app.notify(String(e), true);
                    }
                  }
                }}
              >
                <Copy size={13} />
                {t("复制配置到项目")}
              </Button>
              <Button
                onClick={() =>
                  void desktop("terminal", {
                    path: project.path,
                    app: app.settings.terminal,
                  }).catch((e) => app.notify(String(e), true))
                }
              >
                <Terminal size={13} />
                {t("终端")}
              </Button>
            </div>
            <div
              className="workspace-detail-cover"
              style={{ "--workspace-color": "#8e80c9" } as React.CSSProperties}
            >
              <Code2 size={40} strokeWidth={1.2} />
              <div className="flex-1">
                <h2>{project.name}</h2>
                <p>{project.path}</p>
              </div>
            </div>
            <h3 className="section-title">
              {t("配置文件 ")}
              <span>{project.configs.length}</span>
            </h3>
            <div className="agent-files">
              {project.configs.map((c) => (
                <button
                  key={c.path}
                  onClick={() =>
                    c.is_dir
                      ? app.navigate({ type: "folder", path: c.path })
                      : setConfig(c)
                  }
                >
                  <FileCode2 size={16} />
                  <span>{c.name}</span>
                  <Badge color={tools[c.tool]?.color}>{c.tool}</Badge>
                  {c.personal && <Badge>{t("个人设置")}</Badge>}
                  <ArrowUpRight size={13} />
                </button>
              ))}
            </div>
            <h3 className="section-title">{t("在项目中启动")}</h3>
            <div className="manager-filter">
              {Object.entries(tools)
                .filter(
                  ([name]) =>
                    project.tools.includes(name) ||
                    load.data?.apps.some((a) => a.id === tools[name].id),
                )
                .map(([name, agentTool]) => {
                  const installed = load.data?.apps.some(
                    (a) => a.id === agentTool.id,
                  );
                  return (
                    <Button
                      key={name}
                      onClick={() =>
                        installed
                          ? void desktop("agent", {
                              path: project.path,
                              agent: agentTool.id,
                              app: app.settings.terminal,
                            }).catch((e) => app.notify(String(e), true))
                          : void desktop("open_url", { url: agentTool.url })
                      }
                    >
                      {installed ? (
                        <Play size={12} />
                      ) : (
                        <ExternalLink size={12} />
                      )}
                      {name}
                      {!installed && t(" · 安装帮助")}
                    </Button>
                  );
                })}
            </div>
          </>
        ) : (
          <>
            <div className="page-toolbar">
              <div className="search-input">
                <Search size={14} />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("搜索项目与路径…")}
                />
              </div>
              <div className="flex-1" />
              <Select
                value={tool}
                onChange={(e) => setTool(e.target.value)}
                aria-label={t("Agent 工具")}
              >
                <option value="all">{t("所有工具")}</option>
                {[
                  "Claude",
                  "Codex",
                  "Gemini",
                  "Copilot",
                  "OpenCode",
                  "Cursor",
                  "Antigravity",
                  "Agent Skills",
                ].map((localValue) => (
                  <option key={localValue}>{localValue}</option>
                ))}
              </Select>
            </div>
            {load.loading ? (
              <Loading label={t("识别项目配置…")} />
            ) : load.error ? (
              <ErrorState error={load.error} />
            ) : !projects.length ? (
              <Empty icon={<Bot size={28} />} title={t("没有发现 Agent 项目")}>
                {t("添加项目目录，识别 CLAUDE.md、AGENTS.md 等配置。")}
              </Empty>
            ) : (
              <div className="agent-projects">
                {projects.map((p) => (
                  <button
                    key={p.path}
                    className="agent-project"
                    onClick={() => setProject(p)}
                  >
                    <header>
                      {p.global ? <Globe2 size={17} /> : <Folder size={17} />}
                      <h3>{p.name}</h3>
                      {p.git && <span className="muted">⑂</span>}
                    </header>
                    <p>{p.path}</p>
                    <div className="agent-tools">
                      {p.tools.map((localValue) => (
                        <Badge
                          key={localValue}
                          color={tools[localValue]?.color}
                        >
                          {localValue}
                        </Badge>
                      ))}
                      <div className="flex-1" />
                      <span className="muted" style={{ fontSize: 9 }}>
                        {p.configs.length}
                        {t(" 个文件")}
                      </span>
                    </div>
                  </button>
                ))}
              </div>
            )}
            {load.data?.truncated && (
              <p className="inline-warning">
                {t("已达到搜索上限，可添加更具体的项目目录。")}
              </p>
            )}
          </>
        )
      ) : tab === "usage" ? (
        <Usage />
      ) : tab === "servers" ? (
        <Servers />
      ) : (
        <Mcp projects={load.data?.projects || []} />
      )}
      {config && <ConfigEditor config={config} close={() => setConfig(null)} />}
    </div>
  );
}
function ConfigEditor({
  config,
  close,
}: {
  config: Config;
  close: () => void;
}) {
  const app = useApp();
  const load = useAsync(
    () =>
      request<{ text: string; revision: string }>("read_text", {
        path: config.path,
      }),
    [config.path],
  );
  const [text, setText] = useState("");
  const [revision, setRevision] = useState("");
  const [original, setOriginal] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (load.data) {
      setText(load.data.text);
      setOriginal(load.data.text);
      setRevision(load.data.revision);
    }
  }, [load.data]);
  const leave = async () => {
    if (
      text === original ||
      (await app.confirm(t("放弃未保存的更改？"), config.name))
    )
      close();
  };
  return (
    <Modal
      title={config.name}
      wide
      onClose={() => void leave()}
      footer={
        <>
          <span className="muted flex-1">
            {text === original ? t("已保存") : t("尚未保存")}
            {t(" · 可撤销保存")}
          </span>
          <Button onClick={() => setText(original)}>{t("还原")}</Button>
          <Button
            disabled={busy || !load.data || text === original}
            tone="primary"
            onClick={async () => {
              setBusy(true);
              try {
                const v = await request<{ revision: string }>("write_text", {
                  path: config.path,
                  text,
                  revision,
                });
                setRevision(v.revision);
                setOriginal(text);
                app.notify(t("配置已保存，并保留原始备份"));
                app.refreshAll();
              } catch (e) {
                app.notify(String(e), true);
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("保存")}
          </Button>
        </>
      }
    >
      <p className="agent-code-warning">
        {config.path}
        {config.personal && t(" · 此文件属于个人配置，跨项目复制时会跳过。")}
      </p>
      {load.loading ? (
        <Loading />
      ) : load.error ? (
        <ErrorState error={load.error} />
      ) : (
        <textarea
          className="agent-file-editor"
          value={text}
          onChange={(e) => setText(e.target.value)}
          spellCheck={false}
        />
      )}
    </Modal>
  );
}
function Usage() {
  const app = useApp();
  const [period, setPeriod] = useState(30);
  const [provider, setProvider] = useState("all");
  const [project, setProject] = useState("");
  const [breakdown, setBreakdown] = useState("models");
  const from = new Date(Date.now() - (period - 1) * 86400000)
    .toISOString()
    .slice(0, 10);
  const load = useAsync(
    () => request<any>("agent_usage", { from, provider, project }),
    [period, provider, project, app.refresh],
  );
  const v = load.data;
  const max = Math.max(
    1,
    ...(v?.days || []).map((d: any) => d.input + d.output),
  );
  return (
    <>
      <div className="page-toolbar">
        <Select
          value={period}
          onChange={(e) => setPeriod(Number(e.target.value))}
          aria-label={t("统计周期")}
        >
          <option value={1}>{t("今天")}</option>
          <option value={7}>{t("最近 7 天")}</option>
          <option value={30}>{t("最近 30 天")}</option>
          <option value={36500}>{t("全部记录")}</option>
        </Select>
        <Select
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
          aria-label={t("工具筛选")}
        >
          <option value="all">{t("所有工具")}</option>
          {["Claude", "Codex", "Gemini", "Copilot", "OpenCode"].map((p) => (
            <option key={p}>{p}</option>
          ))}
        </Select>
        <div className="flex-1" />
        <Button
          onClick={() =>
            app.showModal(
              <Prices
                models={v?.models || []}
                close={() => app.showModal(null)}
              />,
            )
          }
        >
          {t("模型价格")}
        </Button>
      </div>
      {load.loading ? (
        <Loading label={t("汇总本地日志，不读取提示词…")} />
      ) : load.error ? (
        <ErrorState error={load.error} />
      ) : !v || !v.totals.sessions ? (
        <Empty icon={<Activity size={30} />} title={t("暂无可识别的用量记录")}>
          {t(
            "当前时间范围内没有匹配的本地日志。其他客户端的历史不会自动出现在这里。",
          )}
        </Empty>
      ) : (
        <>
          <div className="stat-grid">
            <div className="stat-card">
              <span>{t("总 Token")}</span>
              <strong>{Number(v.totals.total).toLocaleString()}</strong>
              <small>
                {v.totals.sessions}
                {t(" 次会话")}
              </small>
            </div>
            <div className="stat-card">
              <span>{t("缓存读取")}</span>
              <strong>{Number(v.totals.cache_read).toLocaleString()}</strong>
              <small>{t("包含在输入统计中")}</small>
            </div>
            <div className="stat-card">
              <span>{t("估算费用")}</span>
              <strong>
                {v.totals.usd == null ? "—" : `$${v.totals.usd.toFixed(2)}`}
              </strong>
              <small>
                {v.totals.usd == null
                  ? t("需配置所有模型价格")
                  : t("按自定义单价估算，非账单")}
              </small>
            </div>
          </div>
          <div className="usage-chart">
            {v.days
              .filter((d: any) => d.name !== t("未知日期"))
              .slice(-31)
              .map((d: any, i: number, arr: any[]) => (
                <div
                  key={d.name}
                  style={{
                    height: `${Math.max(2, ((d.input + d.output) / max) * 100)}%`,
                  }}
                  title={`${d.name} · ${(d.input + d.output).toLocaleString()} Token`}
                >
                  {(arr.length < 10 || i % 5 === 0) && (
                    <small>{d.name.slice(5)}</small>
                  )}
                </div>
              ))}
          </div>
          <div className="page-tabs" style={{ marginTop: 38 }}>
            {[
              ["models", t("按模型")],
              ["projects", t("按项目")],
              ["providers", t("按工具")],
            ].map(([value, label]) => (
              <button
                key={value}
                className={breakdown === value ? "active" : ""}
                onClick={() => setBreakdown(value)}
              >
                {label}
              </button>
            ))}
          </div>
          <table className="data-table">
            <thead>
              <tr>
                <th style={{ width: "40%" }}>{t("名称")}</th>
                <th>{t("输入")}</th>
                <th>{t("输出")}</th>
                <th>{t("缓存读取")}</th>
              </tr>
            </thead>
            <tbody>
              {v[breakdown].map((r: any) => (
                <tr
                  key={r.name}
                  onClick={() => {
                    if (breakdown === "projects")
                      setProject(project ? "" : r.name);
                  }}
                >
                  <td title={r.name}>{r.name}</td>
                  <td>{r.input.toLocaleString()}</td>
                  <td>{r.output.toLocaleString()}</td>
                  <td>{r.cache_read.toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {project && (
            <Button onClick={() => setProject("")} style={{ marginTop: 15 }}>
              <X size={12} />
              {t("取消项目筛选")}
            </Button>
          )}
          <p className="inline-warning" style={{ marginTop: 25 }}>
            {v.note}
            {v.malformed
              ? t(" · 已跳过 {0} 条损坏或未完成的记录。", v.malformed)
              : ""}
            {v.truncated ? t(" · 本次达到扫描上限，结果不完整。") : ""}
          </p>
        </>
      )}
    </>
  );
}
function Prices({
  models,
  close,
}: {
  models: { name: string }[];
  close: () => void;
}) {
  const app = useApp();
  const load = useAsync(
    () =>
      request<Revision<Record<string, any>>>("catalog_get", {
        namespace: "pricing",
      }),
    [],
  );
  const [prices, setPrices] = useState<Record<string, any>>({});
  useEffect(() => {
    if (load.data) setPrices(load.data.data || {});
  }, [load.data]);
  return (
    <Modal
      title={t("模型价格 · USD / 百万 Token")}
      wide
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            onClick={async () => {
              await request("catalog_save", {
                namespace: "pricing",
                data: prices,
                revision: load.data?.revision,
              });
              app.refreshAll();
              close();
            }}
          >
            {t("保存")}
          </Button>
        </>
      }
    >
      <p className="muted">
        {t(
          "根据你实际使用的服务填写单价。未填写的模型不会产生虚假的费用估算。",
        )}
      </p>
      <table className="data-table">
        <thead>
          <tr>
            <th style={{ width: "35%" }}>{t("模型")}</th>
            <th>{t("输入")}</th>
            <th>{t("输出")}</th>
            <th>{t("缓存读取")}</th>
            <th>{t("缓存写入")}</th>
          </tr>
        </thead>
        <tbody>
          {[
            ...new Set([...models.map((m) => m.name), ...Object.keys(prices)]),
          ].map((model) => (
            <tr key={model}>
              <td title={model}>{model}</td>
              {["input", "output", "cache_read", "cache_write"].map((key) => (
                <td key={key}>
                  <input
                    style={{ width: "100%", padding: 5 }}
                    type="number"
                    min={0}
                    step="0.01"
                    value={prices[model]?.[key] ?? ""}
                    onChange={(e) =>
                      setPrices((v) => ({
                        ...v,
                        [model]: {
                          ...v[model],
                          [key]:
                            e.target.value === ""
                              ? undefined
                              : Number(e.target.value),
                          updated: new Date().toISOString().slice(0, 10),
                        },
                      }))
                    }
                  />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </Modal>
  );
}
function Servers() {
  const app = useApp();
  const data = useAsync(() => request<any[]>("local_servers"), [app.refresh]);
  return (
    <>
      <div className="page-toolbar">
        <p className="muted" style={{ fontSize: 11 }}>
          {t("当前用户的开发进程与项目监听端口")}
        </p>
        <div className="flex-1" />
        <Button onClick={() => void data.refresh()}>
          <RefreshCw size={13} />
          {t("刷新")}
        </Button>
      </div>
      {data.loading ? (
        <Loading />
      ) : data.error ? (
        <ErrorState error={data.error} />
      ) : !data.data?.length ? (
        <Empty
          icon={<Activity size={28} />}
          title={t("没有发现本地开发服务")}
        />
      ) : (
        data.data.map((s, i) => (
          <div className="server-row" key={`${s.pid}-${s.port}-${i}`}>
            <span className="status-dot" />
            <div className="flex-1">
              <strong>
                {s.name} <Badge>:{s.port}</Badge>
              </strong>
              <p>
                {s.cwd} · PID {s.pid}
              </p>
            </div>
            <Button onClick={() => void desktop("open_url", { url: s.url })}>
              <ExternalLink size={12} />
              {t("打开")}
            </Button>
            <IconButton
              label={t("停止此进程")}
              disabled={!s.can_stop}
              onClick={async () => {
                if (
                  await app.confirm(
                    t("停止本地服务？"),
                    t(
                      "向 {0}（PID {1}）发送 SIGINT。这个进程的其他端口也可能停止。",
                      s.name,
                      s.pid,
                    ),
                  )
                ) {
                  try {
                    await request("stop_server", {
                      pid: s.pid,
                      start_time: s.start_time,
                      confirmed: true,
                    });
                    await data.refresh();
                  } catch (e) {
                    app.notify(String(e), true);
                  }
                }
              }}
            >
              <Square size={13} />
            </IconButton>
          </div>
        ))
      )}
    </>
  );
}
function Mcp({ projects }: { projects: Project[] }) {
  const app = useApp();
  const load = useAsync(() => request<any>("mcp_list"), [app.refresh]);
  const [editing, setEditing] = useState<any>(null);
  const targets = projects.flatMap((p) =>
    p.configs
      .filter(
        (c) =>
          !c.is_dir &&
          /(?:mcp.*\.json|config\.toml|settings\.json|opencode\.jsonc?)$/.test(
            c.name,
          ),
      )
      .map((c) => ({ ...c, project: p.name })),
  );
  return (
    <>
      <div className="page-toolbar">
        <p className="muted" style={{ fontSize: 11 }}>
          {t("按工具查看全局和项目 MCP 配置")}
        </p>
        <div className="flex-1" />
        <Button onClick={() => setEditing({})}>
          <Plus size={13} />
          {t("添加服务器")}
        </Button>
      </div>
      {load.loading ? (
        <Loading />
      ) : load.error ? (
        <ErrorState error={load.error} />
      ) : !load.data?.items.length ? (
        <Empty icon={<Plug size={29} />} title={t("没有发现 MCP 服务器")}>
          {t("选择项目配置后，添加一个服务器。")}
        </Empty>
      ) : (
        load.data.items.map((m: any, i: number) => (
          <article className="mcp-card" key={`${m.path}-${m.name}-${i}`}>
            <header>
              <Plug size={18} />
              <h3>{m.name}</h3>
              <Badge>{m.tool}</Badge>
              {m.installed === false && (
                <Badge color="#d69b67">{t("缺少程序")}</Badge>
              )}
              {m.has_secrets && <Badge>{t("密钥已隐藏")}</Badge>}
            </header>
            <p className="muted" style={{ fontSize: 10, marginBottom: 10 }}>
              {m.project} · {m.path}
            </p>
            <pre>{JSON.stringify(m.config, null, 2)}</pre>
            <footer>
              <Button onClick={() => setEditing(m)}>{t("编辑")}</Button>
              <Button
                onClick={() => {
                  const clean = { ...m.config };
                  delete clean.env;
                  delete clean.headers;
                  setEditing({ ...m, path: "", config: clean, copy: true });
                }}
              >
                <Copy size={12} />
                {t("复制到其他工具")}
              </Button>
            </footer>
          </article>
        ))
      )}
      {load.data?.errors.length > 0 && (
        <details>
          <summary className="muted">
            {t("配置读取问题（")}
            {load.data.errors.length}）
          </summary>
          <pre className="code-preview">
            {JSON.stringify(load.data.errors, null, 2)}
          </pre>
        </details>
      )}
      {editing && (
        <McpEditor
          value={editing}
          targets={targets}
          close={() => setEditing(null)}
        />
      )}
    </>
  );
}
function McpEditor({
  value,
  targets,
  close,
}: {
  value: any;
  targets: (Config & { project: string })[];
  close: () => void;
}) {
  const app = useApp();
  const [path, setPath] = useState(value.path || targets[0]?.path || "");
  const [name, setName] = useState(value.name || "");
  const [text, setText] = useState(
    JSON.stringify(
      value.config || { command: "npx", args: ["-y", "your-mcp-server"] },
      null,
      2,
    ),
  );
  const [error, setError] = useState("");
  const [create, setCreate] = useState(false);
  const baseline = useAsync(async () => {
    if (!path || create) return null;
    return request<{ text: string; revision: string }>("read_text", { path });
  }, [path, create]);
  return (
    <Modal
      title={value.copy ? t("复制 MCP 服务器") : t("MCP 服务器")}
      wide
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!path || !name}
            onClick={async () => {
              try {
                const config = JSON.parse(text);
                if (!create && (baseline.loading || !baseline.data))
                  throw new Error(
                    baseline.error || t("正在读取配置版本，请稍后重试"),
                  );
                await request("mcp_save", {
                  path,
                  name,
                  config,
                  revision:
                    !value.copy && value.path === path
                      ? value.revision
                      : baseline.data?.revision,
                  create,
                });
                app.refreshAll();
                close();
              } catch (e) {
                setError(String(e));
              }
            }}
          >
            {t("保存并备份")}
          </Button>
        </>
      }
    >
      <Field label={t("目标配置")}>
        <Select
          value={path}
          onChange={(e) => {
            setCreate(false);
            setPath(e.target.value);
          }}
        >
          <option value="">{t("选择已识别的配置文件")}</option>
          {create && <option value={path}>{path}</option>}
          {targets.map((localValue) => (
            <option key={localValue.path} value={localValue.path}>
              {localValue.project} · {localValue.tool} · {localValue.name}
            </option>
          ))}
        </Select>
      </Field>
      <Button
        onClick={async () => {
          const directories = await app.pickFiles({ directories: true });
          if (!directories[0]) return;
          const name = await app.prompt(
            t("新建 MCP 配置"),
            ".mcp.json",
            t("在所选目录建立 JSON / JSONC 或 TOML 配置。"),
          );
          if (!name || name.includes("/") || name === "." || name === "..")
            return;
          setPath(`${directories[0]}/${name}`);
          setCreate(true);
        }}
      >
        {t("新建配置文件…")}
      </Button>
      <Field label={t("服务器名称")}>
        <input
          disabled={!!value.path && !value.copy}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <Field label={t("JSON 配置")}>
        <textarea
          className="agent-file-editor"
          rows={10}
          value={text}
          onChange={(e) => setText(e.target.value)}
          spellCheck={false}
        />
      </Field>
      {value.copy && (
        <p className="inline-warning">
          {t("环境变量和请求头未自动复制，请按目标工具的需求填写。")}
        </p>
      )}
      {value.has_secrets && !value.copy && (
        <p className="muted">
          {t(
            "以圆点隐藏的值将保留原值；填写新值可替换。保存时检查编辑开始时的文件版本。",
          )}
        </p>
      )}
      {error && <p className="inline-warning">{error}</p>}
    </Modal>
  );
}
