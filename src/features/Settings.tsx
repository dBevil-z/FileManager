import { t } from "../i18n";
import { useState } from "react";
import {
  Check,
  Command,
  Download,
  Folder,
  Info,
  Keyboard,
  Layers,
  Monitor,
  Moon,
  Palette,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Sun,
  Terminal,
  Trash2,
} from "lucide-react";
import { useApp } from "../context";
import { desktop, native, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Settings } from "../types";
import { GlassFolder, clearPreviewCache } from "../components/FileIcon";
import {
  Badge,
  Button,
  Field,
  IconButton,
  Modal,
  Select,
  Toggle,
} from "../components/ui";
import { skins } from "../lib/themes";
import { repository } from "./About";
import { ResourcePanel } from "./Resources";
import { shortcutDefaults, shortcutLabels, keyString } from "../lib/shortcuts";
export function SettingsPage() {
  const app = useApp();
  const s = app.settings;
  const apps = useAsync(
    () => desktop<{ id: string; path: string }[]>("apps"),
    [],
  );
  const [editing, setEditing] = useState<string | null>(null);
  const update = (v: Partial<Settings>) =>
    void app.updateSettings(v).catch(() => {});
  return (
    <div className="feature-page settings-page">
      <div className="page-heading">
        <div className="heading-copy">
          <h2>{t("把 FileManager，变成你的。")}</h2>
          <p>{t("外观、工作习惯和桌面集成，都在这里。")}</p>
        </div>
        <Badge>FileManager {app.boot.version}</Badge>
      </div>
      <Section title={t("外观")}>
        <div className="skin-grid">
          {skins.map((skin) => (
            <button
              key={skin.id}
              className={`skin-card ${s.skin === skin.id ? "selected" : ""}`}
              aria-pressed={s.skin === skin.id}
              onClick={() =>
                update({
                  skin: skin.id,
                  accent: skin.accent,
                  folderColor: skin.folder,
                })
              }
            >
              <span
                className="skin-sample"
                style={{ background: skin.dark[0] }}
              >
                <i style={{ background: skin.dark[1] }} />
                <span style={{ background: skin.dark[2] }} />
                <b style={{ background: skin.accent }} />
                <em style={{ background: skin.light[1] }} />
              </span>
              <span>
                <strong>{t(skin.name)}</strong>
                <small>{t(skin.subtitle)}</small>
              </span>
              {s.skin === skin.id && <Check size={13} />}
            </button>
          ))}
        </div>
        <Row
          title={t("减少动态效果")}
          description={t(
            "保留状态反馈，关闭装饰动画。系统的减少动态偏好始终优先。",
          )}
        >
          <Toggle
            label={t("减少动态效果")}
            checked={s.reduceMotion}
            onChange={(v) => update({ reduceMotion: v })}
          />
        </Row>
        <Row
          title={t("主题")}
          description={t("跟随系统，或选择你喜欢的明暗。")}
        >
          <div className="theme-options">
            {(
              [
                ["system", Monitor, t("系统")],
                ["light", Sun, t("浅色")],
                ["dark", Moon, t("深色")],
              ] as const
            ).map(([value, Icon, label]) => (
              <button
                className={s.theme === value ? "active" : ""}
                key={value}
                onClick={() => update({ theme: value })}
              >
                <Icon size={13} />
                {label}
              </button>
            ))}
          </div>
        </Row>
        <Row title={t("强调色")}>
          <div className="color-options">
            {[
              "#8b73ef",
              "#6c9ee8",
              "#61b29a",
              "#cc9760",
              "#d381a5",
              "#9196a5",
            ].map((color) => (
              <button
                key={color}
                aria-label={color}
                style={{ backgroundColor: color }}
                className={s.accent === color ? "selected" : ""}
                onClick={() => update({ accent: color })}
              />
            ))}
            <input
              aria-label={t("自定义强调色")}
              type="color"
              value={s.accent}
              onChange={(e) => update({ accent: e.target.value })}
            />
          </div>
        </Row>
        <Row
          title={t("文件夹样式")}
          description={t("玻璃、纯色、扁平与系统蓝色。")}
        >
          <GlassFolder color={s.folderColor} style={s.folderStyle} small />
          <Select
            value={s.folderStyle}
            onChange={(e) =>
              update({ folderStyle: e.target.value as Settings["folderStyle"] })
            }
          >
            <option value="glass">{t("玻璃")}</option>
            <option value="plain">{t("纯色")}</option>
            <option value="flat">{t("扁平")}</option>
            <option value="system">{t("系统")}</option>
          </Select>
        </Row>
        <Row title={t("默认文件夹颜色")}>
          <input
            type="color"
            aria-label={t("默认文件夹颜色")}
            value={s.folderColor}
            onChange={(e) => update({ folderColor: e.target.value })}
          />
        </Row>
        <Row title={t("界面语言")}>
          <Select
            value={s.language}
            onChange={(e) => update({ language: e.target.value })}
          >
            <option value="zh-CN">简体中文</option>
            <option value="en">English</option>
            <option value="ja">日本語 (English)</option>
            <option value="de">Deutsch (English)</option>
            <option value="fr">Français (English)</option>
            <option value="es">Español (English)</option>
          </Select>
        </Row>
      </Section>
      <Section title={t("打开方式")}>
        <Row
          title={t("默认终端")}
          description={t("打开当前目录，或在项目中启动 Agent。")}
        >
          <Select
            value={s.terminal}
            onChange={(e) => update({ terminal: e.target.value })}
          >
            {["konsole", "gnome-terminal", "kitty", "alacritty"].map((id) => (
              <option
                key={id}
                value={id}
                disabled={
                  apps.data ? !apps.data.some((a) => a.id === id) : false
                }
              >
                {id}
                {apps.data && !apps.data.some((a) => a.id === id)
                  ? t(" · 未安装")
                  : ""}
              </option>
            ))}
          </Select>
        </Row>
        <Row title={t("默认编辑器")}>
          <Select
            value={s.editor}
            onChange={(e) => update({ editor: e.target.value })}
          >
            {[
              "code",
              "cursor",
              "zed",
              "idea",
              "webstorm",
              "pycharm",
              "kate",
            ].map((id) => (
              <option
                key={id}
                value={id}
                disabled={
                  apps.data ? !apps.data.some((a) => a.id === id) : false
                }
              >
                {id}
                {apps.data && !apps.data.some((a) => a.id === id)
                  ? t(" · 未安装")
                  : ""}
              </option>
            ))}
          </Select>
        </Row>
        <Row
          title={t("路径栏别名")}
          description={t(
            "输入 term: /目录 或 code: /目录，直接在终端或编辑器打开。",
          )}
        >
          <kbd>Ctrl L</kbd>
        </Row>
      </Section>
      <Section title={t("桌面与 Drop Shelf")}>
        <Row
          title={t("显示系统托盘")}
          description={t("关窗后继续运行，提供常用目录和工作空间。")}
        >
          <Toggle
            label={t("系统托盘")}
            checked={s.showTray}
            onChange={(v) => update({ showTray: v })}
          />
        </Row>
        <Row title={t("登录时启动")}>
          <Toggle
            label={t("登录启动")}
            checked={s.startAtLogin}
            onChange={(v) => update({ startAtLogin: v })}
          />
        </Row>
        <Row
          title={t("Drop Shelf 全局快捷键")}
          description={t("默认避开 KDE 的 Alt Space。")}
        >
          <button className="shortcut-key" onClick={() => setEditing("shelf")}>
            {s.shelfShortcut}
          </button>
        </Row>
        <Row
          title={t("拖动文件时显示 Shelf")}
          description={t("X11 拖动检测；Wayland 采用快捷键与顶边触发。")}
        >
          <Toggle
            label={t("拖动显示 Shelf")}
            checked={s.shelfOnDrag}
            onChange={(v) => update({ shelfOnDrag: v })}
          />
        </Row>
        <Row title={t("顶边悬停触发")}>
          <Toggle
            label={t("顶边触发")}
            checked={s.shelfOnHover}
            onChange={(v) => update({ shelfOnHover: v })}
          />
        </Row>
        <Row
          title={t("固定 Shelf 目录")}
          description={t("最多九个目录，可使用 1–9 键。")}
        >
          <Button
            onClick={async () => {
              const paths = await app.pickFiles({
                directories: true,
                multiple: true,
              });
              if (paths.length)
                await app.updateCatalog((v) => ({
                  ...v,
                  shelf: paths.slice(0, 9),
                }));
            }}
          >
            {t("选择目录")}
          </Button>
        </Row>
      </Section>
      <Section title={t("快捷键")}>
        {Object.entries(shortcutLabels).map(([id, label]) => (
          <Row key={id} title={t(label)}>
            <button className="shortcut-key" onClick={() => setEditing(id)}>
              {s.shortcuts[id] || shortcutDefaults[id]}
            </button>
          </Row>
        ))}
        <Row title={t("恢复快捷键")}>
          <Button
            onClick={() =>
              update({ shortcuts: {}, shelfShortcut: "Control+Alt+Space" })
            }
          >
            {t("恢复默认")}
          </Button>
        </Row>
      </Section>
      <Section title={t("系统搜索")}>
        <Row title={t("索引工作空间")}>
          <Toggle
            label={t("索引工作空间")}
            checked={s.indexWorkspaces}
            onChange={(v) => update({ indexWorkspaces: v })}
          />
        </Row>
        <Row title={t("索引书签集合")}>
          <Toggle
            label={t("索引集合")}
            checked={s.indexCollections}
            onChange={(v) => update({ indexCollections: v })}
          />
        </Row>
        <Row
          title={t("索引书签")}
          description={t("供 FileManager CLI 和 KRunner 查询本地名称。")}
        >
          <Toggle
            label={t("索引书签")}
            checked={s.indexBookmarks}
            onChange={(v) => update({ indexBookmarks: v })}
          />
        </Row>
      </Section>
      <Section title={t("性能与资源")}>
        <ResourcePanel />
        <Row
          title={t("缩略图模式")}
          description={t(
            "轻量模式使用文件类型图标；选中文件仍可查看完整预览。",
          )}
        >
          <Select
            value={s.thumbnailMode}
            onChange={(e) => {
              clearPreviewCache();
              update({
                thumbnailMode: e.target.value as Settings["thumbnailMode"],
              });
            }}
          >
            <option value="auto">{t("按需生成缩略图")}</option>
            <option value="icons">{t("轻量模式：仅图标")}</option>
          </Select>
        </Row>
      </Section>
      <Section title={t("访问与缓存")}>
        <Row title={t("主目录访问")} description={app.boot.home}>
          <Badge color="#72b99c">{t("可访问")}</Badge>
        </Row>
        <Row title={t("本地数据")} description={app.boot.data_dir}>
          <Button
            onClick={() =>
              app.navigate({ type: "folder", path: app.boot.data_dir })
            }
          >
            {t("打开")}
          </Button>
        </Row>
        <Row
          title={t("缩略图缓存")}
          description={t("删除生成的预览，下次查看时会重新生成。")}
        >
          <Select
            value={s.cacheLimitMB}
            onChange={(e) => update({ cacheLimitMB: Number(e.target.value) })}
          >
            <option value={256}>256 MB</option>
            <option value={512}>512 MB</option>
            <option value={1024}>1 GB</option>
            <option value={2048}>2 GB</option>
          </Select>
          <Button
            onClick={async () => {
              await request("clear_cache");
              clearPreviewCache();
              app.refreshAll();
              app.notify(t("预览缓存已清空"));
              app.refreshAll();
            }}
          >
            <Trash2 size={12} />
            {t("清空")}
          </Button>
        </Row>
      </Section>
      <Section title={t("关于与更新")}>
        <Row
          title={`FileManager Linux ${app.boot.version}`}
          description={t("Tauri + Rust · 本机数据 · 系统 WebKit")}
        >
          <Badge>Linux</Badge>
        </Row>
        <Row title="GitHub" description={s.repositoryUrl}>
          <Button
            onClick={() => void desktop("open_url", { url: s.repositoryUrl })}
          >
            {t("访问")}
          </Button>
          <Button
            onClick={async () => {
              const value = await app.prompt(
                t("GitHub 地址"),
                s.repositoryUrl,
                t("先使用作者主页；创建仓库后可替换为仓库地址。"),
              );
              if (value === null) return;
              const valid = repository(value);
              if (!valid) {
                app.notify(
                  t("请输入 https://github.com/用户名 或仓库地址"),
                  true,
                );
                return;
              }
              await app.updateSettings({ repositoryUrl: valid.url });
            }}
          >
            {t("修改地址")}
          </Button>
        </Row>
        <Row title={t("软件介绍与使用指南")}>
          <Button onClick={() => app.navigate({ type: "about" })}>
            {t("关于 FileManager")}
          </Button>
        </Row>
        <Row
          title={t("检查更新")}
          description={t("当前为本地构建版，更新通过本项目重新构建安装。")}
        >
          <Button
            onClick={() => {
              const repo = repository(s.repositoryUrl);
              if (repo?.isRepo)
                void desktop("open_url", { url: repo.url + "/releases" }).catch(
                  (e) => app.notify(String(e), true),
                );
              else
                app.notify(
                  t(
                    "当前版本 {0}。创建仓库后可连接 GitHub Releases。",
                    app.boot.version,
                  ),
                );
            }}
          >
            <RefreshCw size={12} />
            {t("检查版本")}
          </Button>
        </Row>
        <Row title={t("参考产品与使用指南")}>
          <Button
            onClick={() =>
              void desktop("open_url", {
                url: "https://fileosmac.com/docs/",
              })
            }
          >
            {t("FileOS 参考指南")}
          </Button>
        </Row>
      </Section>
      {editing && (
        <ShortcutEditor id={editing} close={() => setEditing(null)} />
      )}
    </div>
  );
}
function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="settings-section">
      <h3>{title}</h3>
      <div className="settings-group">{children}</div>
    </section>
  );
}
function Row({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div className="setting-copy">
        <strong>{title}</strong>
        {description && <p>{description}</p>}
      </div>
      {children}
    </div>
  );
}
function ShortcutEditor({ id, close }: { id: string; close: () => void }) {
  const app = useApp();
  const [value, setValue] = useState(
    id === "shelf"
      ? app.settings.shelfShortcut
      : app.settings.shortcuts[id] || shortcutDefaults[id],
  );
  const all = {
    ...shortcutDefaults,
    ...app.settings.shortcuts,
    shelf: app.settings.shelfShortcut,
  };
  const conflict = Object.entries(all).find(
    ([key, v]) => key !== id && v.toLowerCase() === value.toLowerCase(),
  );
  return (
    <Modal
      title={t("设置快捷键")}
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!value || !!conflict}
            onClick={async () => {
              if (id === "shelf")
                await app.updateSettings({ shelfShortcut: value });
              else
                await app.updateSettings({
                  shortcuts: { ...app.settings.shortcuts, [id]: value },
                });
              close();
            }}
          >
            {t("保存")}
          </Button>
        </>
      }
    >
      <Field
        label={id === "shelf" ? "Drop Shelf" : t(shortcutLabels[id])}
        hint={t("点击输入框，然后按下组合键。")}
      >
        <input
          value={value}
          readOnly
          onKeyDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
            const key = keyString(e.nativeEvent);
            if (key) setValue(key);
          }}
        />
      </Field>
      {conflict && (
        <p className="inline-warning">
          {t("这个组合键已分配给")}{" "}
          {conflict[0] === "shelf"
            ? "Drop Shelf"
            : t(shortcutLabels[conflict[0]])}
          。
        </p>
      )}
    </Modal>
  );
}
