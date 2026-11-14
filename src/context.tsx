import { createContext, useContext, type ReactNode } from "react";
import type { PluginItem } from "./features/Plugins";
import type {
  Bootstrap,
  Catalog,
  Entry,
  Job,
  Page,
  Settings,
  OperationResult,
} from "./types";
export type AppContextValue = {
  boot: Bootstrap;
  catalog: Catalog;
  settings: Settings;
  page: Page;
  plugins: PluginItem[];
  jobs: Job[];
  runPlugin: (id: string, selection?: string[], directory?: string) => void;
  openTab: (path: string) => void;
  navigate: (page: Page) => void;
  updateCatalog: (change: (value: Catalog) => Catalog) => Promise<void>;
  updateSettings: (change: Partial<Settings>) => Promise<void>;
  notify: (message: string, error?: boolean) => void;
  prompt: (
    title: string,
    initial?: string,
    hint?: string,
  ) => Promise<string | null>;
  confirm: (
    title: string,
    message: string,
    danger?: boolean,
  ) => Promise<boolean>;
  pickFiles: (options?: {
    directories?: boolean;
    filesOnly?: boolean;
    multiple?: boolean;
    initial?: string;
  }) => Promise<string[]>;
  showModal: (node: ReactNode | null) => void;
  refresh: number;
  refreshAll: () => void;
  open: (entry: Entry) => void;
  peek: (entry: Entry, entries?: Entry[]) => void;
  operate: (
    kind: string,
    paths: string[],
    destination?: string,
  ) => Promise<OperationResult | null>;
  saveText: (text: string) => Promise<void>;
};
export const AppContext = createContext<AppContextValue | null>(null);
export function useApp() {
  const value = useContext(AppContext);
  if (!value) throw new Error("App context is not available");
  return value;
}
