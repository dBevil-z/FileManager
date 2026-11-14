import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Download, ExternalLink, RefreshCw, ShieldCheck } from "lucide-react";
import { desktop, errorText, native } from "../api";
import { Button, Badge } from "../components/ui";
import { useApp } from "../context";
import { t } from "../i18n";
import { formatBytes } from "../lib/files";
import { project } from "../lib/project";

type Mode = "user" | "appimage" | "deb" | "rpm" | "manual";
type Release = { version: string; notes: string; date?: string };
type Progress = {
  phase: "downloading" | "verifying" | "installing" | "installed";
  downloaded?: number;
  total?: number;
};
type Status = {
  mode: Mode;
  installed?: boolean;
  busy?: boolean;
  progress?: Progress;
  update?: Release | null;
  error?: string;
};

export function UpdatePanel() {
  const app = useApp();
  const [status, setStatus] = useState<Status>({ mode: "manual" });
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState(false);
  const [error, setError] = useState("");
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    if (!native) return;
    let unlisten: (() => void) | undefined;
    void desktop<Status>("update_status")
      .then((s) => {
        if (mounted.current) {
          setStatus(s);
          setError(s.error || "");
        }
      })
      .catch((e) => {
        if (mounted.current) setError(errorText(e));
      });
    void listen<Progress>("filemanager-update-progress", ({ payload }) => {
      if (mounted.current) setStatus((s) => ({ ...s, progress: payload }));
    })
      .then((stop) => {
        if (mounted.current) unlisten = stop;
        else stop();
      })
      .catch(() => {});
    return () => {
      mounted.current = false;
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    if (!status.busy) return;
    const timer = setInterval(() => {
      void desktop<Status>("update_status")
        .then((s) => {
          if (mounted.current) {
            setStatus(s);
            setError(s.error || "");
          }
        })
        .catch(() => {});
    }, 1000);
    return () => clearInterval(timer);
  }, [status.busy]);
  const check = async () => {
    if (checking || status.busy) return;
    setChecking(true);
    setError("");
    try {
      const result = await desktop<Status>("update_check");
      if (mounted.current) {
        setStatus(result);
        setChecked(true);
      }
    } catch (e) {
      if (mounted.current) setError(errorText(e));
    } finally {
      if (mounted.current) setChecking(false);
    }
  };
  const install = async () => {
    if (status.busy || !status.update) return;
    if (
      app.jobs.some(
        (job) => job.status === "running" || job.status === "queued",
      )
    ) {
      setError(t("请先等待后台任务完成，再安装更新。"));
      return;
    }
    if (
      !(await app.confirm(
        t("安装更新"),
        t(
          "下载并验证官方签名后安装 {0}，完成后由你选择重启。",
          status.update.version,
        ),
      ))
    )
      return;
    setError("");
    setStatus((s) => ({
      ...s,
      busy: true,
      progress: { phase: "downloading" },
    }));
    try {
      await desktop("update_install");
      if (mounted.current)
        setStatus((s) => ({
          ...s,
          busy: false,
          installed: true,
          progress: { phase: "installed" },
        }));
    } catch (e) {
      if (mounted.current) {
        setError(errorText(e));
        setStatus((s) => ({ ...s, busy: false, progress: undefined }));
      }
    }
  };
  const progress = status.progress;
  const percent =
    progress?.total && progress.downloaded !== undefined
      ? Math.min(100, Math.floor((progress.downloaded / progress.total) * 100))
      : undefined;
  const busy = status.busy || checking;
  return (
    <section className="update-panel" aria-label={t("软件更新")}>
      <div className="update-panel-heading">
        <div className="update-emblem">
          <ShieldCheck size={24} />
        </div>
        <div className="flex-1">
          <h3>{t("软件更新")}</h3>
          <p>{t("当前版本 {0} · 官方稳定通道", app.boot.version)}</p>
        </div>
        <Badge>
          {status.mode === "user"
            ? t("用户安装")
            : status.mode === "manual"
              ? t("手动安装")
              : status.mode === "appimage"
                ? "AppImage"
                : status.mode.toUpperCase()}
        </Badge>
      </div>
      <p className="update-description">
        {t("仅在点击检查时连接官方 GitHub Releases，安装前验证签名。")}
      </p>
      {(status.mode === "deb" || status.mode === "rpm") && (
        <p className="update-description">
          {t("系统软件包安装需要系统授权，不会在应用内收集密码。")}
        </p>
      )}
      {status.mode === "manual" && (
        <p className="update-description">
          {t("源码构建或未识别的安装位置可检查版本，请从发行版页面手动更新。")}
        </p>
      )}
      <div className="update-result" role="status" aria-live="polite">
        {checking ? (
          <p>{t("正在检查官方发行版…")}</p>
        ) : status.installed ? (
          <p className="update-success">{t("更新已安装，重启后生效。")}</p>
        ) : status.update ? (
          <>
            <strong>{t("发现新版本 {0}", status.update.version)}</strong>
            {status.update.date && (
              <small>{status.update.date.slice(0, 10)}</small>
            )}
            {status.update.notes && (
              <pre className="update-notes">{status.update.notes}</pre>
            )}
          </>
        ) : checked && !error ? (
          <p className="update-success">{t("已是最新版本")}</p>
        ) : null}
        {status.busy && (
          <div className="update-progress">
            <p>
              {progress?.phase === "installing"
                ? t("正在安装更新…")
                : progress?.phase === "verifying"
                  ? t("正在验证官方签名…")
                  : t("正在下载更新…")}{" "}
              {percent !== undefined && `${percent}%`}{" "}
              {progress?.downloaded !== undefined &&
                ` · ${formatBytes(progress.downloaded)}`}
            </p>
            <progress max={100} value={percent} aria-label={t("更新进度")} />
          </div>
        )}
      </div>
      {error && (
        <p className="update-error" role="alert">
          {error}
        </p>
      )}
      <div className="about-links">
        {status.installed ? (
          <Button
            tone="primary"
            onClick={() =>
              void app.restartForUpdate().catch((e) => setError(errorText(e)))
            }
          >
            {t("立即重启")}
          </Button>
        ) : (
          <>
            <Button disabled={!native || busy} onClick={() => void check()}>
              <RefreshCw size={13} className={checking ? "spin" : undefined} />
              {t(checked || error ? "重新检查" : "检查更新")}
            </Button>
            {status.update && status.mode !== "manual" && (
              <Button
                tone="primary"
                disabled={busy}
                onClick={() => void install()}
              >
                <Download size={13} />
                {t("下载并安装")}
              </Button>
            )}
          </>
        )}
        <Button
          onClick={() =>
            void desktop("open_url", { url: project.latest }).catch((e) =>
              app.notify(errorText(e), true),
            )
          }
        >
          <ExternalLink size={13} />
          GitHub Releases
        </Button>
      </div>
    </section>
  );
}
