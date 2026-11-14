import { useViewState } from "../hooks/useViewState";
import { viewKey } from "../lib/view-memory";
import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import {
  Cloud,
  KeyRound,
  Pencil,
  Plus,
  RefreshCw,
  Server,
  Trash2,
} from "lucide-react";
import { errorText, request } from "../api";
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
import { useApp } from "../context";
import { useAsync } from "../hooks/useAsync";
import { t } from "../i18n";
import { RemoteBrowser } from "./RemoteBrowser";
import { useViewScroll } from "../hooks/useViewScroll";

type SshAuth = "agent" | "key" | "password";
type Provider =
  | { kind: "minio"; bucket: string; region: string; user: string }
  | {
      kind: "s3";
      bucket: string;
      region: string;
      access_key: string;
      virtual_host: boolean;
    }
  | { kind: "oss"; bucket: string; region: string; access_key: string }
  | { kind: "sftp"; user: string; auth: SshAuth; key_file: string }
  | { kind: "ftp"; user: string };
type Kind = Provider["kind"];
type Config = {
  label: string;
  endpoint: string;
  root: string;
  read_only: boolean;
  provider: Provider;
};
export type Profile = {
  id: string;
  revision: number;
  config: Config;
  remember: boolean;
  needs_secret: boolean;
};
export type RemoteEntry = {
  name: string;
  path: string;
  is_dir: boolean;
  size: number | null;
  modified: number | null;
};
export type Listing = {
  entries: RemoteEntry[];
  truncated: boolean;
  skipped: number;
  next_cursor: string | null;
};

const providerName: Record<Kind, string> = {
  minio: "MinIO",
  s3: "S3",
  oss: "阿里云 OSS",
  sftp: "SFTP",
  ftp: "FTP / FTPS",
};
const example: Record<Kind, string> = {
  minio: "http://minio.example.com:9000",
  s3: "https://s3.us-east-1.amazonaws.com",
  oss: "https://oss-cn-hangzhou.aliyuncs.com",
  sftp: "sftp://server.example.com:22",
  ftp: "ftps://server.example.com:21",
};

function details(config: Config) {
  const { provider, endpoint, root } = config;
  return [endpoint, "bucket" in provider ? provider.bucket : "", root]
    .filter(Boolean)
    .join(" / ");
}

function ConnectionEditor({
  profile,
  close,
  saved,
}: {
  profile: Profile | null;
  close: () => void;
  saved: (profile: Profile) => void;
}) {
  const old = profile?.config;
  const p = old?.provider;
  const [kind, setKind] = useState<Kind>(p?.kind ?? "minio");
  const [label, setLabel] = useState(old?.label ?? "");
  const [endpoint, setEndpoint] = useState(old?.endpoint ?? "");
  const [root, setRoot] = useState(old?.root ?? "");
  const [bucket, setBucket] = useState(p && "bucket" in p ? p.bucket : "");
  const [region, setRegion] = useState(p && "region" in p ? p.region : "");
  const [accessKey, setAccessKey] = useState(
    p?.kind === "minio" ? p.user : p && "access_key" in p ? p.access_key : "",
  );
  const [virtualHost, setVirtualHost] = useState(
    p?.kind === "s3" && p.virtual_host,
  );
  const [user, setUser] = useState(p && "user" in p ? p.user : "");
  const [auth, setAuth] = useState<SshAuth>(
    p?.kind === "sftp" ? p.auth : "agent",
  );
  const [keyFile, setKeyFile] = useState(p?.kind === "sftp" ? p.key_file : "");
  const [secret, setSecret] = useState("");
  const [token, setToken] = useState("");
  const [remember, setRemember] = useState(profile?.remember ?? false);
  const [readOnly, setReadOnly] = useState(old?.read_only ?? false);
  const [status, setStatus] = useState<"test" | "save" | null>(null);
  const [message, setMessage] = useState<{
    error: boolean;
    text: string;
  } | null>(null);
  const errorRef = useRef<HTMLParagraphElement>(null);
  const form = useId();
  const object = kind === "minio" || kind === "s3" || kind === "oss";
  const hasSecret = kind !== "sftp" || auth !== "agent";
  const needsSecret =
    object ||
    (kind === "ftp" && user.trim().toLowerCase() !== "anonymous") ||
    (kind === "sftp" && auth === "password");
  const provider: Provider =
    kind === "minio"
      ? {
          kind,
          bucket: bucket.trim(),
          region: region.trim(),
          user: accessKey.trim(),
        }
      : kind === "s3"
        ? {
            kind,
            bucket: bucket.trim(),
            region: region.trim(),
            access_key: accessKey.trim(),
            virtual_host: virtualHost,
          }
        : kind === "oss"
          ? {
              kind,
              bucket: bucket.trim(),
              region: region.trim().replace(/^oss-/, ""),
              access_key: accessKey.trim(),
            }
          : kind === "sftp"
            ? {
                kind,
                user: user.trim(),
                auth,
                key_file: auth === "key" ? keyFile.trim() : "",
              }
            : { kind, user: user.trim() };
  const keep = Boolean(
    profile &&
    !profile.needs_secret &&
    endpoint.trim().replace(/\/$/, "") === old?.endpoint &&
    Object.entries(provider).every(([key, value]) =>
      Object.entries(p ?? {}).some(
        ([oldKey, oldValue]) => key === oldKey && value === oldValue,
      ),
    ),
  );

  useEffect(() => {
    if (message?.error) errorRef.current?.focus();
  }, [message]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (status) return;
    const submitter = (event.nativeEvent as SubmitEvent)
      .submitter as HTMLButtonElement | null;
    const action = submitter?.value === "test" ? "test" : "save";
    const args = {
      id: profile?.id,
      revision: profile?.revision,
      config: {
        label,
        endpoint,
        root,
        provider,
        read_only: readOnly,
      } satisfies Config,
      secret:
        hasSecret && (secret.length || (object && token.length))
          ? { value: secret, token: object ? token : "" }
          : undefined,
      remember: hasSecret && remember,
    };
    setStatus(action);
    setMessage(null);
    try {
      if (action === "test") {
        await request("remote_test", args);
        setMessage({
          error: false,
          text: t(
            object && !bucket.trim()
              ? "登录成功，可以查看 Bucket 列表。"
              : "连接成功，可以读取起始目录。",
          ),
        });
      } else {
        if (kind === "minio") await request("remote_test", args);
        saved(await request<Profile>("remote_save", args));
      }
    } catch (e) {
      setMessage({ error: true, text: t(errorText(e)) });
    } finally {
      setStatus(null);
    }
  }

  return (
    <Modal
      title={t(profile ? "编辑远程连接" : "添加远程连接")}
      onClose={() => !status && close()}
      footer={
        <>
          <Button onClick={close} disabled={Boolean(status)}>
            {t("取消")}
          </Button>
          <Button
            type="submit"
            form={form}
            value="test"
            disabled={Boolean(status)}
          >
            {t(status === "test" ? "正在测试…" : "测试连接")}
          </Button>
          <Button
            type="submit"
            form={form}
            value="save"
            tone="primary"
            disabled={Boolean(status)}
          >
            {t(
              status === "save"
                ? kind === "minio"
                  ? "正在登录…"
                  : "正在保存…"
                : kind === "minio"
                  ? "登录并保存"
                  : "保存连接",
            )}
          </Button>
        </>
      }
    >
      <form
        id={form}
        onSubmit={submit}
        onChange={() => setMessage(null)}
        className="remote-form"
      >
        {message && (
          <p
            tabIndex={-1}
            ref={errorRef}
            role={message.error ? "alert" : "status"}
            className={
              message.error ? "remote-form-error" : "remote-form-success"
            }
          >
            {message.text}
          </p>
        )}
        <fieldset disabled={Boolean(status)}>
          <Field label={t("连接名称")}>
            <input
              required
              maxLength={100}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={t("例如：项目归档")}
            />
          </Field>
          <Field label={t("协议")}>
            <Select
              value={kind}
              onChange={(e) => {
                const next = e.target.value as Kind;
                setKind(next);
                setEndpoint("");
                setRegion(next === "oss" ? "cn-hangzhou" : "");
                setSecret("");
                setToken("");
              }}
            >
              <option value="minio">{t("MinIO（账号密码）")}</option>
              <option value="s3">{t("S3 兼容（Access Key）")}</option>
              <option value="oss">{t("阿里云 OSS")}</option>
              <option value="sftp">SFTP</option>
              <option value="ftp">FTP / FTPS</option>
            </Select>
          </Field>
          <Field
            label={t("服务器地址")}
            hint={t(
              kind === "minio"
                ? "使用 MinIO API 地址，通常为 9000 端口；请勿填写控制台地址。"
                : "填写协议、主机及可选端口。",
            )}
          >
            <input
              type="url"
              required
              maxLength={2048}
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
              placeholder={example[kind]}
              spellCheck={false}
            />
          </Field>
          <Field
            label={t("起始目录（可选）")}
            hint={
              object
                ? t("相对于 Bucket 的对象前缀。")
                : t("留空使用服务器的登录目录。")
            }
          >
            <input
              value={root}
              onChange={(e) => setRoot(e.target.value)}
              placeholder={object ? "backups/2026" : "/uploads"}
              spellCheck={false}
            />
          </Field>
          {object ? (
            <>
              {kind === "minio" && (
                <p className="muted remote-form-span">
                  {t(
                    "使用 MinIO 本地账号和密码登录，也可填写 Access Key 和 Secret Key。",
                  )}
                </p>
              )}
              <Field
                label={kind === "oss" ? "Bucket" : t("Bucket（可选）")}
                hint={
                  kind === "oss"
                    ? undefined
                    : t(
                        "留空查看账号可列出的所有桶；也可指定已授权的桶直接进入。",
                      )
                }
              >
                <input
                  required={kind === "oss"}
                  value={bucket}
                  onChange={(e) => setBucket(e.target.value)}
                  spellCheck={false}
                />
              </Field>
              <Field
                label={kind === "oss" ? "Region" : t("Region（可选）")}
                hint={
                  kind === "oss"
                    ? undefined
                    : t(
                        "留空自动识别 Bucket Region，无需配置 Region 的 MinIO 可直接连接。",
                      )
                }
              >
                <input
                  required={kind === "oss"}
                  value={region}
                  onChange={(e) => setRegion(e.target.value)}
                  spellCheck={false}
                />
              </Field>
              <Field
                label={kind === "minio" ? t("账号 / 用户名") : "Access Key ID"}
              >
                <input
                  required
                  autoComplete={kind === "minio" ? "username" : "off"}
                  value={accessKey}
                  onChange={(e) => setAccessKey(e.target.value)}
                  spellCheck={false}
                />
              </Field>
              <Field label={t("会话令牌（可选）")}>
                <input
                  type="password"
                  autoComplete="new-password"
                  maxLength={16384}
                  value={token}
                  onChange={(e) => setToken(e.target.value)}
                />
              </Field>
              {kind === "s3" && (
                <label className="remote-check">
                  <input
                    type="checkbox"
                    checked={virtualHost}
                    onChange={(e) => setVirtualHost(e.target.checked)}
                  />
                  {t("Bucket 使用子域名寻址（MinIO 通常留空）")}
                </label>
              )}
            </>
          ) : (
            <>
              <Field label={t("用户名")}>
                <input
                  required
                  value={user}
                  onChange={(e) => setUser(e.target.value)}
                  autoComplete="username"
                  spellCheck={false}
                />
              </Field>
              {kind === "sftp" && (
                <>
                  <Field label={t("认证方式")}>
                    <Select
                      value={auth}
                      onChange={(e) => {
                        setAuth(e.target.value as SshAuth);
                        setSecret("");
                      }}
                    >
                      <option value="agent">SSH Agent</option>
                      <option value="key">{t("SSH 私钥文件")}</option>
                      <option value="password">{t("密码")}</option>
                    </Select>
                  </Field>
                  {auth === "key" && (
                    <Field label={t("SSH 私钥文件")}>
                      <input
                        required
                        value={keyFile}
                        onChange={(e) => setKeyFile(e.target.value)}
                        placeholder="~/.ssh/id_ed25519"
                        spellCheck={false}
                      />
                    </Field>
                  )}
                  <p className="muted remote-form-span">
                    {t("SFTP 仅连接已记录在 SSH known_hosts 的主机。")}
                  </p>
                </>
              )}
            </>
          )}
          {hasSecret && (
            <Field
              label={
                kind === "minio"
                  ? t("密码")
                  : object
                    ? "Secret Access Key"
                    : kind === "sftp" && auth === "key"
                      ? t("私钥口令（可选）")
                      : t("密码")
              }
              hint={keep ? t("留空继续使用已保存的凭据。") : undefined}
            >
              <input
                type="password"
                autoComplete={
                  kind === "minio" ? "current-password" : "new-password"
                }
                required={(needsSecret && !keep) || Boolean(object && token)}
                maxLength={4096}
                value={secret}
                onChange={(e) => setSecret(e.target.value)}
              />
            </Field>
          )}
          {hasSecret && (
            <label className="remote-check">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              <span>
                {t("在系统钥匙串中记住凭据")}
                <small>{t("不勾选时，凭据仅在本次运行有效。")}</small>
              </span>
            </label>
          )}
          <label className="remote-check">
            <input
              type="checkbox"
              checked={readOnly}
              onChange={(e) => setReadOnly(e.target.checked)}
            />
            {t("只读连接")}
          </label>
          {/^(ftp|http):\/\//i.test(endpoint) && (
            <p className="muted remote-form-span">
              {t(
                "此地址没有传输加密。公网连接请优先使用 HTTPS、FTPS 或 SFTP。",
              )}
            </p>
          )}
        </fieldset>
      </form>
    </Modal>
  );
}

export function RemoteStoragePage() {
  const app = useApp();
  const profiles = useAsync(() => request<Profile[]>("remote_profiles"), []);
  const [selected, setSelected] = useViewState("remote:selected", "");
  const scrollRef = useRef<HTMLElement>(null);
  const [editing, setEditing] = useState<Profile | null | undefined>();
  const [removing, setRemoving] = useState(false);
  const profile = profiles.data?.find((item) => item.id === selected);
  useViewScroll(
    scrollRef,
    viewKey("remote", selected),
    Boolean(profiles.data) && (!profile || profile.needs_secret),
  );
  useEffect(() => {
    if (profiles.data && selected && !profile) setSelected("");
  }, [profiles.data, selected, profile, setSelected]);
  async function remove(item: Profile) {
    setRemoving(true);
    try {
      if (
        !(await app.confirm(
          t("移除远程连接"),
          t(
            "移除「{0}」的本地配置和凭据？服务器上的文件不受影响。",
            item.config.label,
          ),
          true,
        ))
      )
        return;
      await request("remote_remove", { id: item.id, revision: item.revision });
      setSelected("");
      await profiles.refresh();
      app.notify(t("已移除远程连接"));
    } catch (e) {
      app.notify(t(errorText(e)), true);
    } finally {
      setRemoving(false);
    }
  }
  return (
    <section className="feature-page remote-page" ref={scrollRef}>
      <header className="page-heading remote-heading">
        <div className="heading-copy">
          <h2>{t("远程存储")}</h2>
          <p>{t("连接对象存储或文件服务器，浏览目录并传输文件。")}</p>
        </div>
        <Button tone="primary" onClick={() => setEditing(null)}>
          <Plus size={16} aria-hidden="true" />
          {t("添加连接")}
        </Button>
      </header>
      {profiles.error ? (
        <ErrorState
          error={t(profiles.error)}
          retry={() => void profiles.refresh()}
        />
      ) : !profiles.data ? (
        <Loading />
      ) : profiles.data.length === 0 ? (
        <Empty
          icon={<Cloud size={29} aria-hidden="true" />}
          title={t("还没有远程连接")}
        >
          {t("添加 MinIO、S3 兼容存储、阿里云 OSS、SFTP 或 FTP / FTPS。")}
        </Empty>
      ) : (
        <div className="remote-layout">
          <nav className="remote-connections" aria-label={t("远程连接")}>
            <div className="remote-connections-head">
              <h2>{t("远程连接")}</h2>
              <IconButton
                label={t("刷新连接")}
                disabled={profiles.loading}
                onClick={() => void profiles.refresh()}
              >
                <RefreshCw size={14} />
              </IconButton>
            </div>
            {profiles.data.map((item) => (
              <button
                type="button"
                key={item.id}
                className="remote-connection"
                aria-pressed={selected === item.id}
                onClick={() => setSelected(item.id)}
                title={details(item.config)}
              >
                <Server size={16} aria-hidden="true" />
                <span>
                  <strong>{item.config.label}</strong>
                  <small>{t(providerName[item.config.provider.kind])}</small>
                </span>
                {item.needs_secret && <KeyRound size={14} aria-hidden="true" />}
              </button>
            ))}
          </nav>
          {!profile ? (
            <Empty
              icon={<Server size={26} aria-hidden="true" />}
              title={t("选择一个连接")}
            >
              {t("选中连接后加载远程目录。")}
            </Empty>
          ) : (
            <div className="remote-browser">
              <header className="remote-browser-head">
                <div className="remote-profile-info">
                  <h2>
                    {profile.config.label}
                    {profile.config.read_only && <Badge>{t("只读")}</Badge>}
                  </h2>
                  <p className="muted" title={details(profile.config)}>
                    {details(profile.config)}
                  </p>
                </div>
                <IconButton
                  label={t("编辑远程连接")}
                  disabled={removing}
                  onClick={() => setEditing(profile)}
                >
                  <Pencil size={16} />
                </IconButton>
                <IconButton
                  label={t("移除远程连接")}
                  disabled={removing}
                  onClick={() => void remove(profile)}
                >
                  <Trash2 size={16} />
                </IconButton>
              </header>
              {profile.needs_secret ? (
                <Empty
                  icon={<KeyRound size={24} aria-hidden="true" />}
                  title={t("此连接需要凭据")}
                >
                  <Button onClick={() => setEditing(profile)}>
                    {t("输入凭据")}
                  </Button>
                </Empty>
              ) : (
                <RemoteBrowser
                  key={`${profile.id}:${profile.revision}`}
                  profile={profile}
                />
              )}
            </div>
          )}
        </div>
      )}
      {editing !== undefined && (
        <ConnectionEditor
          profile={editing}
          close={() => setEditing(undefined)}
          saved={(item) => {
            setEditing(undefined);
            profiles.setData((old) =>
              old?.some((p) => p.id === item.id)
                ? old.map((p) => (p.id === item.id ? item : p))
                : [...(old ?? []), item],
            );
            setSelected(item.id);
            app.notify(t("远程连接已保存"));
          }}
        />
      )}
    </section>
  );
}
