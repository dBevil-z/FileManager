import { t } from "../i18n";
import { MarkdownImage } from "../components/MarkdownImage";
import { useEffect, useMemo, useRef, useState, useDeferredValue } from "react";
import {
  ArrowLeft,
  BookOpen,
  Columns2,
  Eye,
  FileText,
  List,
  Maximize2,
  Pencil,
  RotateCcw,
  Save,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useApp } from "../context";
import { desktop, native, request } from "../api";
import { useAsync } from "../hooks/useAsync";
import type { Page } from "../types";
import { Button, ErrorState, IconButton, Loading } from "../components/ui";
export function MarkdownEditor({ path, back }: { path: string; back: Page }) {
  const app = useApp();
  const load = useAsync(
    () =>
      request<{ text: string; revision: string; line_ending: string }>(
        "read_text",
        { path },
      ),
    [path],
  );
  const [text, setText] = useState("");
  const [original, setOriginal] = useState("");
  const [revision, setRevision] = useState("");
  const [view, setView] = useState("split");
  const [toc, setToc] = useState(false);
  const [saving, setSaving] = useState(false);
  const [external, setExternal] = useState(false);
  const previewText = useDeferredValue(text.slice(0, 65536));
  const dirty = text !== original;
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;
  const textarea = useRef<HTMLTextAreaElement>(null);
  const article = useRef<HTMLElement>(null);
  useEffect(() => {
    if (load.data) {
      setText(load.data.text);
      setOriginal(load.data.text);
      setRevision(load.data.revision);
      setExternal(false);
    }
  }, [load.data]);
  async function save() {
    setSaving(true);
    try {
      const result = await request<{ revision: string }>("write_text", {
        path,
        text,
        revision,
      });
      setOriginal(text);
      setRevision(result.revision);
      setExternal(false);
      app.notify(t("文稿已保存"));
    } catch (e) {
      setExternal(true);
      app.notify(String(e), true);
    } finally {
      setSaving(false);
    }
  }
  useEffect(() => {
    const before = (e: Event) => {
      if (!dirtyRef.current) return;
      e.preventDefault();
      const { proceed } = (e as CustomEvent).detail;
      void app
        .confirm(t("文稿尚未保存"), t("放弃当前修改并离开？"))
        .then((ok) => {
          if (ok) proceed();
        });
    };
    const unload = (e: BeforeUnloadEvent) => {
      if (dirtyRef.current) e.preventDefault();
    };
    window.addEventListener("filemanager-before-navigate", before);
    window.addEventListener("beforeunload", unload);
    return () => {
      window.removeEventListener("filemanager-before-navigate", before);
      window.removeEventListener("beforeunload", unload);
    };
  }, []);
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [text, revision]);
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.hidden) return;
      void request<{ revision: string }>("read_text", { path })
        .then((v) => {
          if (revision && v.revision !== revision) setExternal(true);
        })
        .catch(() => {});
    }, 6000);
    return () => clearInterval(timer);
  }, [path, revision]);
  const headings = useMemo(
    () =>
      previewText
        .split("\n")
        .filter((l) => /^#{1,6}\s/.test(l))
        .slice(0, 300)
        .map((l) => ({
          level: l.match(/^#+/)![0].length,
          text: l.replace(/^#+\s*/, ""),
        })),
    [previewText],
  );
  const renderHeading =
    (level: number) =>
    ({ children }: { children?: React.ReactNode }) => {
      const Tag = `h${level}` as "h1";
      return (
        <Tag id={`md-${String(children).replace(/\s+/g, "-")}`}>{children}</Tag>
      );
    };
  return (
    <div className="markdown-editor">
      <header className="editor-toolbar">
        <Button onClick={() => app.navigate(back)}>
          <ArrowLeft size={13} />
          {t("返回")}
        </Button>
        <span>{dirty ? t("未保存的修改") : t("已保存")}</span>
        <div className="flex-1" />
        <IconButton
          label={t("文稿目录")}
          active={toc}
          onClick={() => setToc((v) => !v)}
        >
          <List size={16} />
        </IconButton>
        <div className="segmented">
          {(
            [
              ["edit", Pencil, t("编辑")],
              ["split", Columns2, t("分屏")],
              ["preview", Eye, t("预览")],
            ] as const
          ).map(([v, Icon, label]) => (
            <IconButton
              label={label}
              active={view === v}
              key={v}
              onClick={() => setView(v)}
            >
              <Icon size={15} />
            </IconButton>
          ))}
        </div>
        <IconButton
          label={t("还原修改")}
          disabled={!dirty}
          onClick={async () => {
            if (
              await app.confirm(
                t("还原未保存的修改？"),
                t("将恢复为上次读取或保存的内容。"),
              )
            )
              setText(original);
          }}
        >
          <RotateCcw size={15} />
        </IconButton>
        <IconButton
          label={t("全屏编辑")}
          onClick={() => {
            if (document.fullscreenElement) void document.exitFullscreen();
            else void document.documentElement.requestFullscreen();
          }}
        >
          <Maximize2 size={15} />
        </IconButton>
        <Button
          tone="primary"
          disabled={!dirty || saving}
          onClick={() => void save()}
        >
          <Save size={13} />
          {t("保存")}
        </Button>
      </header>
      {external && (
        <div className="inline-warning" style={{ margin: 12 }}>
          {t(
            "文件在其他应用中发生变化。为避免覆盖，请复制当前草稿后重新载入。",
          )}
          <Button
            onClick={() => void app.saveText(text)}
            style={{ marginLeft: 10 }}
          >
            {t("复制草稿")}
          </Button>
          <Button
            onClick={async () => {
              if (
                !dirty ||
                (await app.confirm(
                  t("重新载入文件？"),
                  t("当前草稿将被文件中的最新内容替换。"),
                ))
              )
                void load.refresh();
            }}
            style={{ marginLeft: 8 }}
          >
            {t("重新载入")}
          </Button>
        </div>
      )}
      {load.loading ? (
        <Loading />
      ) : load.error ? (
        <ErrorState error={load.error} />
      ) : (
        <div className="editor-body">
          {toc && (
            <aside className="editor-toc">
              <h4>{t("文稿目录")}</h4>
              {headings.map((h, i) => (
                <button
                  style={{ paddingLeft: (h.level - 1) * 9 }}
                  key={i}
                  onClick={() =>
                    article.current
                      ?.querySelector(
                        `#${CSS.escape(`md-${h.text.replace(/\s+/g, "-")}`)}`,
                      )
                      ?.scrollIntoView({ behavior: "smooth", block: "start" })
                  }
                >
                  {h.text}
                </button>
              ))}
            </aside>
          )}
          {view !== "preview" && (
            <textarea
              ref={textarea}
              aria-label={t("Markdown 编辑器")}
              spellCheck={false}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Tab") {
                  e.preventDefault();
                  const localValue = e.currentTarget,
                    start = localValue.selectionStart,
                    end = localValue.selectionEnd;
                  setText(text.slice(0, start) + "  " + text.slice(end));
                  setTimeout(() => {
                    localValue.selectionStart = localValue.selectionEnd =
                      start + 2;
                  });
                }
              }}
            />
          )}
          {view !== "edit" && (
            <article ref={article}>
              {text.length > 65536 && (
                <p className="inline-warning">
                  {t("文稿较大，仅渲染前 64 KiB；编辑与保存保留完整内容。")}
                </p>
              )}
              <div className="markdown-content">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm]}
                  components={{
                    h1: renderHeading(1),
                    h2: renderHeading(2),
                    h3: renderHeading(3),
                    h4: renderHeading(4),
                    a: ({ href, children }) => (
                      <a
                        href={href}
                        onClick={(e) => {
                          e.preventDefault();
                          if (href && /^https?:/.test(href))
                            void desktop("open_url", { url: href });
                        }}
                      >
                        {children}
                      </a>
                    ),
                    img: ({ src, alt }) => (
                      <MarkdownImage document={path} src={src} alt={alt} />
                    ),
                  }}
                >
                  {previewText}
                </ReactMarkdown>
              </div>
            </article>
          )}
        </div>
      )}
      <footer className="editor-status">
        <span>Markdown</span>
        <span>
          {text.length.toLocaleString()}
          {t(" 字符")}
        </span>
        <span>
          {text.split("\n").length}
          {t(" 行")}
        </span>
        <div className="flex-1" />
        <span>UTF-8</span>
        <span>{load.data?.line_ending?.toUpperCase() || "LF"}</span>
        <span>{t("Ctrl S 保存")}</span>
      </footer>
    </div>
  );
}
