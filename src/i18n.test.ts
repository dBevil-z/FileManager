import { describe, expect, it } from "vitest";
import { translate, setLanguage, t, languageResources } from "./i18n";

describe("localization", () => {
  it("translates interface text and preserves user supplied names exactly", () => {
    expect(translate("打开 {0}", "en", "设计 {1} $&.md")).toBe(
      "Open 设计 {1} $&.md",
    );
    expect(translate("打开 {0}", "zh-CN", "设计 {1} $&.md")).toBe(
      "打开 设计 {1} $&.md",
    );
  });
  it("switches live and retains whitespace around inline labels", () => {
    setLanguage("en");
    expect(t(" 已选择 ")).toBe(" Selected ");
    setLanguage("zh-CN");
    expect(t("已选择")).toBe("已选择");
  });
  it("has six explicit resources with an English fallback for unfinished translations", () => {
    expect(Object.keys(languageResources)).toHaveLength(6);
    expect(translate("设置", "de")).toBe("Settings");
  });
});
