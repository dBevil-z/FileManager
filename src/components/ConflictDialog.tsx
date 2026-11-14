import { t } from "../i18n";
import { useState } from "react";
import type { OperationResult } from "../types";
import { formatBytes } from "../lib/files";
import { FileIcon } from "./FileIcon";
import { Button, Modal, Select } from "./ui";

type Policy = "keep_both" | "replace" | "skip";
export function ConflictDialog({
  conflicts,
  done,
}: {
  conflicts: NonNullable<OperationResult["conflicts"]>;
  done: (resolutions: Record<string, Policy> | null) => void;
}) {
  const [choices, setChoices] = useState<Record<string, Policy>>(
    Object.fromEntries(conflicts.map((c) => [c.source.path, "keep_both"])),
  );
  return (
    <Modal
      title={t("目标中已有同名项目")}
      wide
      onClose={() => done(null)}
      footer={
        <>
          <Button onClick={() => done(null)}>{t("停止")}</Button>
          <Button tone="primary" onClick={() => done(choices)}>
            {t("继续")}
          </Button>
        </>
      }
    >
      <p className="muted">
        {t("替换的原文件将移到回收站，可随本次操作撤销。")}
      </p>
      <div className="form-row">
        <span>{t("全部采用")}</span>
        <Select
          aria-label={t("全部采用")}
          value=""
          onChange={(e) => {
            if (e.target.value)
              setChoices(
                Object.fromEntries(
                  conflicts.map((c) => [
                    c.source.path,
                    e.target.value === "replace" && c.can_replace === false
                      ? "keep_both"
                      : (e.target.value as Policy),
                  ]),
                ),
              );
          }}
        >
          <option value="">{t("逐项选择")}</option>
          <option value="keep_both">{t("保留两者")}</option>
          <option value="replace">{t("替换")}</option>
          <option value="skip">{t("跳过")}</option>
        </Select>
      </div>
      <div className="conflict-list">
        {conflicts.map((c) => (
          <div key={c.source.path}>
            <FileIcon entry={c.source} />
            <span className="flex-1">
              {c.source.name}
              <small className="muted">
                {formatBytes(c.source.size)} → {formatBytes(c.destination.size)}
              </small>
            </span>
            <Select
              aria-label={t("{0} 同名处理", c.source.name)}
              value={choices[c.source.path]}
              onChange={(e) =>
                setChoices({
                  ...choices,
                  [c.source.path]: e.target.value as Policy,
                })
              }
            >
              <option value="keep_both">{t("保留两者")}</option>
              <option value="replace" disabled={c.can_replace === false}>
                {t("替换")}
              </option>
              <option value="skip">{t("跳过")}</option>
            </Select>
          </div>
        ))}
      </div>
    </Modal>
  );
}
