export const shortcutDefaults: Record<string, string> = {
  search: "Control+K",
  path: "Control+L",
  new: "Control+N",
  copy: "Control+C",
  cut: "Control+X",
  paste: "Control+V",
  undo: "Control+Z",
  rename: "F2",
  trash: "Delete",
  peek: "Space",
  dual: "Control+Backslash",
};
export const shortcutLabels: Record<string, string> = {
  search: "快速切换",
  path: "输入路径",
  new: "新建文件夹",
  copy: "复制",
  cut: "剪切",
  paste: "粘贴",
  undo: "撤销",
  rename: "重新命名",
  trash: "移到回收站",
  peek: "快速预览",
  dual: "切换双栏",
};
export function keyString(e: KeyboardEvent): string {
  if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return "";
  const key =
    e.code === "Space"
      ? "Space"
      : e.code === "Backslash"
        ? "Backslash"
        : e.key.length === 1
          ? e.key.toUpperCase()
          : e.key;
  return [
    ...(e.ctrlKey ? ["Control"] : []),
    ...(e.altKey ? ["Alt"] : []),
    ...(e.shiftKey ? ["Shift"] : []),
    ...(e.metaKey ? ["Super"] : []),
    key,
  ].join("+");
}
