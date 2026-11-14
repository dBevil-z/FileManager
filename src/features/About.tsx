import { useState } from "react";
import {
  Folder,
  Github,
  ExternalLink,
  Bug,
  BookOpen,
  Cpu,
  ShieldCheck,
  Sparkles,
  Copy,
  Check,
  ChevronRight,
} from "lucide-react";
import { t } from "../i18n";
import { useApp } from "../context";
import { desktop, errorText, native, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import { Badge, Button, Modal } from "../components/ui";
import { shortcutDefaults, shortcutLabels } from "../lib/shortcuts";
export { repository } from "../lib/project";
import { project, repository, migrateRepositoryUrl } from "../lib/project";
import { UpdatePanel } from "./Updates";
export function AboutPage() {
  const app = useApp();
  const [notes, setNotes] = useState<"help" | "release" | "privacy" | null>(
    null,
  );
  const repo = repository(migrateRepositoryUrl(app.settings.repositoryUrl))!;
  const resources = useAsync(
    () =>
      request<{ pss: number | null; rss: number; processes: number }>(
        "resource_usage",
      ),
    [],
  );
  const open = (url: string) =>
    void desktop("open_url", { url }).catch((e) =>
      app.notify(errorText(e), true),
    );
  return (
    <div className="feature-page about-page">
      <section className="about-hero">
        <div className="about-mark">
          <Folder size={44} strokeWidth={1.5} />
        </div>
        <Badge>
          LINUX · {native ? "NATIVE DESKTOP" : "DEVELOPMENT PREVIEW"}
        </Badge>
        <h2>FileManager</h2>
        <p>{t("文件、项目和灵感，各得其所。")}</p>
        <div className="about-version">
          <Badge>v{app.boot.version}</Badge>
          <span>Tauri + Rust</span>
          <span>amd64 · Linux</span>
        </div>
        <div className="about-links">
          <Button tone="primary" onClick={() => open(repo.url)}>
            <Github size={15} />
            GitHub
          </Button>
          <Button onClick={() => setNotes("release")}>
            <Sparkles size={14} />
            {t("更新日志")}
          </Button>
          <Button onClick={() => setNotes("help")}>
            <BookOpen size={14} />
            {t("使用指南")}
          </Button>
          <Button onClick={() => open(project.guide)}>
            <ExternalLink size={14} />
            {t("项目文档")}
          </Button>
          <Button onClick={() => open(project.author)}>
            <Github size={14} />
            dBevil-z
          </Button>
        </div>
      </section>
      <UpdatePanel />
      <div className="about-grid">
        <article>
          <Cpu size={21} />
          <h3>{t("为本地工作而构建")}</h3>
          <p>
            {t(
              "Rust 处理文件、后台任务与撤销；Tauri 2 使用系统 WebKit；React 19 + TypeScript 构建界面，SQLite 保存本地状态。",
            )}
          </p>
          <div className="about-tech">
            {[
              "Rust",
              "Tauri 2",
              "React 19",
              "TypeScript",
              "SQLite",
              "QuickJS",
            ].map((v) => (
              <Badge key={v}>{v}</Badge>
            ))}
          </div>
        </article>
        <article>
          <ShieldCheck size={21} />
          <h3>{t("数据留在你的设备")}</h3>
          <p>
            {t(
              "没有账号系统与遥测。插件通过限定权限访问本次上下文；不提供网络、系统命令或直接写文件能力。",
            )}
          </p>
          <Button onClick={() => setNotes("privacy")}>
            {t("隐私与数据")}
            <ChevronRight size={12} />
          </Button>
        </article>
        <article>
          <Sparkles size={21} />
          <h3>{t("一处管理日常工作")}</h3>
          <p>
            {t(
              "多标签、双栏、预览、Markdown、工作空间、书签、清理、软件包与 AI 工具；按需安装插件，使用主题表达自己的偏好。",
            )}
          </p>
          <Button onClick={() => app.navigate({ type: "plugins" })}>
            {t("探索插件")}
            <ChevronRight size={12} />
          </Button>
        </article>
        <article>
          <Github size={21} />
          <h3>{t("一起完善 FileManager")}</h3>
          <p>
            {repo.isRepo
              ? t("源码、问题反馈与发行版集中在 GitHub。")
              : t(
                  "当前配置指向作者主页。可在设置中填写仓库地址，启用问题反馈与发行版入口。",
                )}
          </p>
          <div className="about-links">
            <Button
              disabled={!repo.isRepo}
              onClick={() => open(repo.url + "/issues/new/choose")}
            >
              <Bug size={13} />
              {t("问题反馈")}
            </Button>
            <Button
              onClick={() =>
                void app.saveText(
                  JSON.stringify(
                    {
                      app: "FileManager",
                      version: app.boot.version,
                      platform: "linux",
                      runtime: "Tauri + WebKit",
                      native,
                      theme: app.settings.theme,
                      skin: app.settings.skin,
                      pss_bytes: resources.data?.pss,
                      rss_bytes: resources.data?.rss,
                      processes: resources.data?.processes,
                    },
                    null,
                    2,
                  ),
                )
              }
            >
              <Copy size={13} />
              {t("复制诊断信息")}
            </Button>
          </div>
        </article>
      </div>
      <footer className="about-footer">
        <p>
          {t(
            "Linux 优先 · 目前已验证 KDE/X11；其他桌面与发行版按路线图逐步验收。",
          )}
        </p>
        <p>{t("项目源码许可证尚未选定；依赖保留各自许可证。")}</p>
      </footer>
      {notes && (
        <Modal
          title={
            notes === "help"
              ? t("使用指南")
              : notes === "release"
                ? t("FileManager 0.2 更新")
                : t("隐私与本地数据")
          }
          wide
          onClose={() => setNotes(null)}
        >
          {notes === "help" ? (
            <>
              <p className="plugin-review-description">
                {t(
                  "位置与收藏用于实际目录；工作空间只保存引用，不移动原文件。右键菜单包含文件操作与已启用插件。",
                )}
              </p>
              <div className="help-shortcuts">
                {Object.entries({
                  ...shortcutDefaults,
                  newTab: "Control+T",
                  closeTab: "Control+W",
                  nextTab: "Control+Tab",
                }).map(([key, value]) => (
                  <div key={key}>
                    <span>
                      {t(
                        shortcutLabels[key] ||
                          (
                            {
                              newTab: "新标签页",
                              closeTab: "关闭标签页",
                              nextTab: "下一标签页",
                            } as Record<string, string>
                          )[key],
                      )}
                    </span>
                    <kbd>{app.settings.shortcuts[key] || value}</kbd>
                  </div>
                ))}
              </div>
              <p className="muted">
                {t(
                  "Space 打开快速预览；双栏中 F5 复制、F6 移动。文件变更可从操作历史撤销。跨盘原件保留在源盘回收站，撤销前请保留它。",
                )}
              </p>
            </>
          ) : notes === "release" ? (
            <div className="release-notes">
              <h3>0.2.4 · FileManager</h3>
              <ul>
                <li>
                  {t("按需检查官方版本，下载验证签名后安装，由你选择重启。")}
                </li>
                <li>
                  {t(
                    "统一 GitHub 仓库、文档、反馈与发行版入口，提供中英发布说明。",
                  )}
                </li>
              </ul>
              <h3>0.2.3 · FileManager</h3>
              <ul>
                <li>
                  {t(
                    "切换菜单后恢复目录、筛选、分页和滚动位置，支持关闭和重置。",
                  )}
                </li>
                <li>
                  {t(
                    "按连接与 Bucket 记住远程目录，短时缓存有上限并自动释放。",
                  )}
                </li>
              </ul>
              <h3>0.2.2 · FileManager</h3>
              <ul>
                <li>
                  {t("MinIO 支持账号密码登录，Bucket 和 Region 可留空。")}
                </li>
                <li>
                  {t(
                    "支持列出有权限的所有桶、分页、直接进入桶和自动识别 Region。",
                  )}
                </li>
              </ul>
              <h3>0.2.1 · FileManager</h3>
              <ul>
                <li>
                  {t("新增 S3/OSS、SFTP、FTP/FTPS 远程存储与系统钥匙串凭据。")}
                </li>
                <li>
                  {t(
                    "支持游标分页、跨页选择、批量文件与目录传输、逐项结果和取消。",
                  )}
                </li>
                <li>
                  {t(
                    "新增按方向比较目录的同步预览，区分缺失项、大小差异和类型冲突。",
                  )}
                </li>
              </ul>
              <h3>0.2.0 · FileManager</h3>
              <ul>
                <li>
                  {t("统一项目、应用、命令和桌面集成名称，兼容已有本地数据。")}
                </li>
                <li>
                  {t(
                    "缩略图与详情分离、按字节限制缓存、收紧解码并发与后台进程生命周期。",
                  )}
                </li>
                <li>
                  {t("新增可恢复的目录标签页、六款明暗皮肤和减少动态效果。")}
                </li>
                <li>{t("新增插件中心、安装权限审核与四款独立插件。")}</li>
                <li>{t("加入 GitHub 入口、介绍、快捷键指南和诊断信息。")}</li>
              </ul>
              <h3>{t("接下来")}</h3>
              <p>
                {t(
                  "Wayland 实测、更多发行版打包与 CI、签名插件目录、主题包、压缩文件工作流和目录比较。详见项目 ROADMAP.md。",
                )}
              </p>
            </div>
          ) : (
            <div className="release-notes">
              <p>
                {t(
                  "工作空间、书签、设置、会话、插件和撤销记录保存在本机数据目录。升级沿用已有数据，不迁移真实文件。",
                )}
              </p>
              <code>{app.boot.data_dir}</code>
              <p>
                {t(
                  "网页书签元数据与远程封面、Markdown 中的远程图片可能访问对应网站。更新仅在主动检查时连接官方 GitHub Releases，安装包在签名验证后使用。GitHub 和外部帮助使用系统浏览器打开。无后台遥测、自动上传或云端文件索引。",
                )}
              </p>
              <p>
                {t(
                  "插件默认没有网络与系统命令。权限仅针对本次选择或当前目录；运行后引擎释放。删除插件不会删除原文件与已导出的报告。",
                )}
              </p>
              <Button
                onClick={() => {
                  setNotes(null);
                  app.navigate({ type: "settings" });
                }}
              >
                {t("查看本地数据设置")}
              </Button>
            </div>
          )}
        </Modal>
      )}
    </div>
  );
}
