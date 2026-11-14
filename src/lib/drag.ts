// Shared only inside this webview. External drags always default to copy.
export const dragState: { paths: string[]; kind: "copy" | "move" } = {
  paths: [],
  kind: "move",
};
