import { invoke, isTauri } from "@tauri-apps/api/core";
export const native = isTauri();
export const mediaUrl = (id: string) =>
  native
    ? `filemanager-media://localhost/${id}`
    : `/api/media/${encodeURIComponent(id)}`;

export async function request<T = unknown>(
  op: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (native) return invoke<T>("request", { input: { op, args } });
  const response = await fetch("/api/request", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-FileManager-Client": "local-ui",
    },
    body: JSON.stringify({ op, args }),
  });
  const result = await response.json();
  if (!response.ok || result.error)
    throw new Error(result.error || `请求失败 (${response.status})`);
  return result.data as T;
}
export async function desktop<T = unknown>(
  action: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  if (native) return invoke<T>("desktop_action", { action, args });
  return request<T>("desktop_action", { action, ...args });
}
export async function chooseFolder(multiple = false): Promise<string[] | null> {
  if (native) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const result = await open({ directory: true, multiple });
    return result ? (Array.isArray(result) ? result : [result]) : null;
  }
  return null;
}
export const errorText = (error: unknown) =>
  error instanceof Error ? error.message : String(error);
