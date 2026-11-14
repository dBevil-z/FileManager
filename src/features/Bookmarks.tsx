import { useViewState } from "../hooks/useViewState";
import { viewKey } from "../lib/view-memory";
import { t } from "../i18n";
import { useEffect, useRef, useState } from "react";
import {
  Bookmark as BookmarkIcon,
  ExternalLink,
  Globe2,
  Grid2X2,
  Layers,
  Link2,
  List,
  MoreHorizontal,
  Pencil,
  Plus,
  Search,
  Star,
  Tag,
  Trash2,
  X,
} from "lucide-react";
import { useApp } from "../context";
import { desktop, request } from "../api";
import type { Bookmark, Collection, Page } from "../types";
import {
  Badge,
  Button,
  Empty,
  Field,
  IconButton,
  Menu,
  Modal,
  Select,
} from "../components/ui";
export function Bookmarks({
  page,
}: {
  page: Extract<Page, { type: "bookmarks" }>;
}) {
  const app = useApp();
  const scope = viewKey(
    "bookmarks",
    page.id || "",
    page.favourites ? "star" : "",
    page.tag || "",
  );
  const [query, setQuery] = useViewState(scope + ":query", "");
  const [view, setView] = useViewState(
    scope + ":view",
    localStorage.getItem("filemanager-bookmark-view") || "grid",
  );
  const [sort, setSort] = useViewState(scope + ":sort", "newest");
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    bookmark: Bookmark;
  } | null>(null);
  const collection = app.catalog.collections.find((c) => c.id === page.id);
  const [tag, setTag] = useViewState(scope + ":tag", page.tag || "");
  const q = query.toLowerCase();
  const list = app.catalog.bookmarks
    .filter(
      (b) =>
        (!page.id || b.collections.includes(page.id)) &&
        (!page.favourites || b.favourite) &&
        (!tag || b.tags.includes(tag)) &&
        `${b.title} ${b.description} ${b.url} ${b.tags.join(" ")}`
          .toLowerCase()
          .includes(q),
    )
    .sort((a, b) =>
      sort === "title"
        ? a.title.localeCompare(b.title, "zh-CN")
        : sort === "site"
          ? (a.site || "").localeCompare(b.site || "")
          : b.created - a.created,
    );
  const tags = [...new Set(app.catalog.bookmarks.flatMap((b) => b.tags))];
  const edit = (bookmark?: Bookmark, url?: string) =>
    app.showModal(
      <BookmarkEditor
        bookmark={bookmark}
        initialUrl={url}
        collection={page.id}
        close={() => app.showModal(null)}
      />,
    );
  async function favourite(b: Bookmark) {
    await app.updateCatalog((v) => ({
      ...v,
      bookmarks: v.bookmarks.map((x) =>
        x.id === b.id ? { ...x, favourite: !x.favourite } : x,
      ),
    }));
  }
  return (
    <div
      className="feature-page"
      onPaste={(e) => {
        if ((e.target as HTMLElement).closest("input,textarea")) return;
        const text = e.clipboardData.getData("text/plain").trim();
        if (/^https?:\/\/\S+$/.test(text)) {
          e.preventDefault();
          edit(undefined, text);
        }
      }}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        const text = (
          e.dataTransfer.getData("text/uri-list") ||
          e.dataTransfer.getData("text/plain")
        )
          .split("\n")
          .find((v) => /^https?:\/\//.test(v));
        if (text) edit(undefined, text.trim());
      }}
    >
      <div className="page-heading">
        <div className="heading-copy">
          <h2>
            {collection?.name || (page.favourites ? t("收藏书签") : t("书签"))}
          </h2>
          <p>
            {collection
              ? t("这个集合里有 {0} 个链接。", list.length)
              : t("收集值得留下的页面，随时回到灵感发生的地方。")}
          </p>
        </div>
        {collection && (
          <IconButton
            label={t("编辑集合")}
            onClick={() =>
              app.showModal(
                <CollectionEditor
                  collection={collection}
                  close={() => app.showModal(null)}
                />,
              )
            }
          >
            <Pencil size={15} />
          </IconButton>
        )}
        <Button
          onClick={() =>
            app.showModal(
              <CollectionEditor close={() => app.showModal(null)} />,
            )
          }
        >
          <Layers size={14} />
          {t("新建集合")}
        </Button>
        <Button tone="primary" onClick={() => edit()}>
          <Plus size={14} />
          {t("添加书签")}
        </Button>
      </div>
      <div className="page-toolbar">
        <div className="search-input">
          <Search size={14} />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("搜索标题、链接与标签…")}
          />
        </div>
        {tags.length > 0 && (
          <Select
            value={tag}
            onChange={(e) => setTag(e.target.value)}
            aria-label={t("标签筛选")}
          >
            <option value="">{t("所有标签")}</option>
            {tags.map((localValue) => (
              <option key={localValue}>{localValue}</option>
            ))}
          </Select>
        )}
        <div className="flex-1" />
        <Select
          value={sort}
          onChange={(e) => setSort(e.target.value)}
          aria-label={t("书签排序")}
        >
          <option value="newest">{t("最近添加")}</option>
          <option value="title">{t("标题")}</option>
          <option value="site">{t("网站")}</option>
        </Select>
        <Select
          value={view}
          onChange={(e) => {
            setView(e.target.value);
            localStorage.setItem("filemanager-bookmark-view", e.target.value);
          }}
          aria-label={t("书签视图")}
        >
          <option value="grid">{t("网格")}</option>
          <option value="title">{t("标题")}</option>
          <option value="masonry">{t("瀑布流")}</option>
          <option value="list">{t("列表")}</option>
        </Select>
      </div>
      {!list.length ? (
        <Empty
          icon={<BookmarkIcon size={31} strokeWidth={1.5} />}
          title={query || tag ? t("没有找到书签") : t("留住一个好链接")}
        >
          <p>{t("粘贴或拖入网页链接，也可以手动添加。")}</p>
          <Button onClick={() => edit()}>
            <Plus size={13} />
            {t("添加第一个书签")}
          </Button>
        </Empty>
      ) : (
        <div
          className={`bookmark-grid ${view === "masonry" ? "bookmark-masonry" : view === "list" ? "bookmark-list" : view === "title" ? "bookmark-title" : ""}`}
        >
          {list.map((b) => (
            <article
              className="bookmark-card"
              key={b.id}
              draggable
              onDragStart={(e) => {
                e.dataTransfer.setData("text/uri-list", b.url);
                e.dataTransfer.setData("text/plain", b.url);
              }}
            >
              <div
                className="bookmark-image"
                onClick={() =>
                  void desktop("open_url", { url: b.url }).catch((e) =>
                    app.notify(String(e), true),
                  )
                }
              >
                {b.image ? (
                  <img
                    alt=""
                    src={b.image}
                    loading="lazy"
                    onError={(e) => {
                      e.currentTarget.style.display = "none";
                    }}
                  />
                ) : (
                  <span className="site-letter">
                    {(b.site || new URL(b.url).hostname)
                      .replace(/^www\./, "")
                      .charAt(0)
                      .toUpperCase()}
                  </span>
                )}
              </div>
              <div
                className="bookmark-body"
                onClick={() =>
                  void desktop("open_url", { url: b.url }).catch((e) =>
                    app.notify(String(e), true),
                  )
                }
              >
                <h3>{b.title || b.url}</h3>
                {b.description && <p>{b.description}</p>}
                <footer>
                  <Globe2 size={11} />
                  <span>{b.site || new URL(b.url).hostname}</span>
                  <div className="flex-1" />
                  {b.favourite && <Star size={11} fill="currentColor" />}
                </footer>
                {b.tags.length > 0 && view !== "list" && (
                  <div className="tag-row">
                    {b.tags.map((localValue) => (
                      <button
                        key={localValue}
                        onClick={(e) => {
                          e.stopPropagation();
                          setTag(localValue);
                        }}
                      >
                        {localValue}
                      </button>
                    ))}
                  </div>
                )}
              </div>
              <div className="bookmark-actions">
                <IconButton
                  label={b.favourite ? t("取消收藏") : t("收藏")}
                  onClick={() => void favourite(b)}
                >
                  <Star
                    size={13}
                    fill={b.favourite ? "currentColor" : "none"}
                  />
                </IconButton>
                <IconButton
                  label={t("书签选项")}
                  onClick={(e) =>
                    setMenu({ x: e.clientX, y: e.clientY, bookmark: b })
                  }
                >
                  <MoreHorizontal size={15} />
                </IconButton>
              </div>
            </article>
          ))}
        </div>
      )}
      {menu && (
        <Menu
          x={menu.x}
          y={menu.y}
          close={() => setMenu(null)}
          items={[
            {
              label: t("编辑书签…"),
              icon: <Pencil size={14} />,
              action: () => edit(menu.bookmark),
            },
            {
              label: t("复制链接"),
              icon: <Link2 size={14} />,
              action: () => void app.saveText(menu.bookmark.url),
            },
            {
              label: menu.bookmark.favourite ? t("取消收藏") : t("加入收藏"),
              icon: <Star size={14} />,
              action: () => void favourite(menu.bookmark),
            },
            ...(page.id
              ? [
                  {
                    label: t("从集合移除"),
                    action: () =>
                      void app.updateCatalog((v) => ({
                        ...v,
                        bookmarks: v.bookmarks.map((b) =>
                          b.id === menu.bookmark.id
                            ? {
                                ...b,
                                collections: b.collections.filter(
                                  (x) => x !== page.id,
                                ),
                              }
                            : b,
                        ),
                      })),
                  },
                ]
              : []),
            { separator: true, label: "" },
            {
              label: t("删除书签"),
              danger: true,
              icon: <Trash2 size={14} />,
              action: () =>
                void app.updateCatalog((v) => ({
                  ...v,
                  bookmarks: v.bookmarks.filter(
                    (b) => b.id !== menu.bookmark.id,
                  ),
                  workspaces: v.workspaces.map((w) => ({
                    ...w,
                    links: w.links.filter((id) => id !== menu.bookmark.id),
                  })),
                })),
            },
          ]}
        />
      )}
    </div>
  );
}
export function BookmarkEditor({
  bookmark,
  initialUrl = "",
  collection,
  close,
}: {
  bookmark?: Bookmark;
  initialUrl?: string;
  collection?: string;
  close: () => void;
}) {
  const app = useApp();
  const [url, setUrl] = useState(bookmark?.url || initialUrl);
  const [title, setTitle] = useState(bookmark?.title || "");
  const [description, setDescription] = useState(bookmark?.description || "");
  const [image, setImage] = useState(bookmark?.image || "");
  const [collections, setCollections] = useState<string[]>(
    bookmark?.collections || (collection ? [collection] : []),
  );
  const [tags, setTags] = useState(bookmark?.tags.join(", ") || "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const current = useRef(url);
  current.current = url;
  async function fetchMeta() {
    if (!/^https?:\/\//.test(url)) return;
    const target = url;
    setBusy(true);
    setError("");
    try {
      const meta = await request<Partial<Bookmark>>("bookmark_metadata", {
        url,
      });
      if (current.current !== target) return;
      if (!title) setTitle(meta.title || "");
      if (!description) setDescription(meta.description || "");
      if (!image) setImage(meta.image || "");
    } catch (e) {
      setError(t("{0}。可手动填写标题后保存。", e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    if (initialUrl && !bookmark) void fetchMeta();
  }, []);
  let valid = false;
  try {
    const parsed = new URL(url);
    valid =
      /^https?:$/.test(parsed.protocol) && !parsed.username && !parsed.password;
  } catch {}
  return (
    <Modal
      title={bookmark ? t("编辑书签") : t("添加书签")}
      onClose={close}
      footer={
        <>
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!valid || busy}
            onClick={async () => {
              const parsed = new URL(url);
              const b: Bookmark = {
                ...bookmark,
                id: bookmark?.id || crypto.randomUUID(),
                url: parsed.href,
                title: title.trim() || parsed.hostname,
                description,
                image: /^https?:\/\//.test(image) ? image : undefined,
                site: parsed.hostname.replace(/^www\./, ""),
                collections,
                tags: [
                  ...new Set(
                    tags
                      .split(/[,，]/)
                      .map((s) => s.trim())
                      .filter(Boolean),
                  ),
                ],
                favourite: bookmark?.favourite || false,
                created: bookmark?.created || Date.now(),
              };
              await app.updateCatalog((v) => ({
                ...v,
                bookmarks: bookmark
                  ? v.bookmarks.map((x) => (x.id === b.id ? b : x))
                  : [...v.bookmarks, b],
              }));
              close();
            }}
          >
            {t("保存书签")}
          </Button>
        </>
      }
    >
      <Field label={t("链接")}>
        <div className="form-row" style={{ marginBottom: 0 }}>
          <input
            autoFocus
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://…"
            onBlur={() => {
              if (!title) void fetchMeta();
            }}
          />
          <Button disabled={!valid || busy} onClick={() => void fetchMeta()}>
            {busy ? t("读取中…") : t("读取页面")}
          </Button>
        </div>
      </Field>
      <Field label={t("标题")}>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t("页面标题")}
        />
      </Field>
      <Field label={t("描述")}>
        <textarea
          rows={2}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </Field>
      <Field label={t("封面图片地址")}>
        <input
          value={image}
          onChange={(e) => setImage(e.target.value)}
          placeholder={t("可选，HTTP / HTTPS 图片地址")}
        />
      </Field>
      <Field label={t("标签")} hint={t("用逗号分隔")}>
        <input
          value={tags}
          onChange={(e) => setTags(e.target.value)}
          placeholder={t("设计, 灵感, 开发")}
        />
      </Field>
      {app.catalog.collections.length > 0 && (
        <Field label={t("集合")}>
          <div className="tag-row">
            {app.catalog.collections.map((c) => (
              <label className="check-label" key={c.id}>
                <input
                  type="checkbox"
                  checked={collections.includes(c.id)}
                  onChange={(e) =>
                    setCollections((v) =>
                      e.target.checked
                        ? [...v, c.id]
                        : v.filter((x) => x !== c.id),
                    )
                  }
                />
                {c.name}
              </label>
            ))}
          </div>
        </Field>
      )}
      {error && <p className="inline-warning">{error}</p>}
    </Modal>
  );
}
function CollectionEditor({
  collection,
  close,
}: {
  collection?: Collection;
  close: () => void;
}) {
  const app = useApp();
  const [name, setName] = useState(collection?.name || "");
  const [color, setColor] = useState(collection?.color || "#9b83ed");
  return (
    <Modal
      title={collection ? t("编辑集合") : t("新建集合")}
      onClose={close}
      footer={
        <>
          {collection && (
            <Button
              tone="danger"
              onClick={async () => {
                await app.updateCatalog((v) => ({
                  ...v,
                  collections: v.collections.filter(
                    (c) => c.id !== collection.id,
                  ),
                  bookmarks: v.bookmarks.map((b) => ({
                    ...b,
                    collections: b.collections.filter(
                      (x) => x !== collection.id,
                    ),
                  })),
                }));
                close();
              }}
            >
              {t("删除集合")}
            </Button>
          )}
          <div className="flex-1" />
          <Button onClick={close}>{t("取消")}</Button>
          <Button
            tone="primary"
            disabled={!name.trim()}
            onClick={async () => {
              const c: Collection = {
                id: collection?.id || crypto.randomUUID(),
                name: name.trim(),
                color,
                icon: "layers",
              };
              await app.updateCatalog((v) => ({
                ...v,
                collections: collection
                  ? v.collections.map((x) => (x.id === c.id ? c : x))
                  : [...v.collections, c],
              }));
              close();
            }}
          >
            {t("保存")}
          </Button>
        </>
      }
    >
      <Field label={t("名称")}>
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("集合名称")}
        />
      </Field>
      <Field label={t("颜色")}>
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
        />
      </Field>
    </Modal>
  );
}
