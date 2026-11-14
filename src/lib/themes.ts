export type Skin = {
  id: string;
  name: string;
  subtitle: string;
  accent: string;
  folder: string;
  dark: string[];
  light: string[];
};
export const skins: Skin[] = [
  {
    id: "iris",
    name: "鸢尾紫",
    subtitle: "柔和 · 经典",
    accent: "#8b73ef",
    folder: "#8870ed",
    dark: ["#1e1c24", "#27232f", "#2d2935", "#35303f", "#40394c"],
    light: ["#fefcff", "#f0ecf6", "#f7f3fb", "#ece6f3", "#e1d9ea"],
  },
  {
    id: "graphite",
    name: "石墨",
    subtitle: "克制 · 专注",
    accent: "#9099b8",
    folder: "#8b94b5",
    dark: ["#1e1e21", "#26262a", "#29292e", "#303036", "#393940"],
    light: ["#ffffff", "#ececee", "#f5f5f7", "#ebebef", "#e0e0e6"],
  },
  {
    id: "ocean",
    name: "深海蓝",
    subtitle: "清晰 · 冷静",
    accent: "#70a9ea",
    folder: "#64a3de",
    dark: ["#19232c", "#202d38", "#263440", "#2e3d4b", "#384a5a"],
    light: ["#f9fcff", "#e7f0f8", "#eff5fb", "#e2ecf6", "#d6e3f0"],
  },
  {
    id: "forest",
    name: "林间",
    subtitle: "自然 · 舒展",
    accent: "#65b79b",
    folder: "#63b69b",
    dark: ["#1b2522", "#22302a", "#29372f", "#314237", "#3a5042"],
    light: ["#fbfdf9", "#eaf1e8", "#f1f6ee", "#e6eee1", "#dce7d6"],
  },
  {
    id: "sand",
    name: "暖砂",
    subtitle: "温暖 · 纸感",
    accent: "#cdac79",
    folder: "#c6a26c",
    dark: ["#25211d", "#302a23", "#373028", "#42392f", "#504536"],
    light: ["#fefbf5", "#f3eadc", "#faf3e8", "#eee3d2", "#e5d7c2"],
  },
  {
    id: "rose",
    name: "暮樱",
    subtitle: "轻盈 · 细腻",
    accent: "#da91ba",
    folder: "#ca8cb4",
    dark: ["#271f27", "#322732", "#3b2d39", "#473545", "#554052"],
    light: ["#fffafd", "#f5e9f1", "#fcf0f7", "#f0e0ec", "#e8d4e2"],
  },
];
function channel(s: string) {
  const v = parseInt(s, 16) / 255;
  return v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
}
export function luminance(hex: string) {
  return (
    channel(hex.slice(1, 3)) * 0.2126 +
    channel(hex.slice(3, 5)) * 0.7152 +
    channel(hex.slice(5, 7)) * 0.0722
  );
}
export function contrast(a: string, b: string) {
  const x = luminance(a),
    y = luminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
export function accentInk(hex: string) {
  return contrast(hex, "#ffffff") >= contrast(hex, "#17171c")
    ? "#ffffff"
    : "#17171c";
}
export function applySkin(
  skinId: string,
  mode: "light" | "dark",
  accent: string,
) {
  const skin = skins.find((s) => s.id === skinId) || skins[1];
  ["--bg", "--sidebar", "--surface", "--surface-2", "--surface-3"].forEach(
    (v, i) => document.documentElement.style.setProperty(v, skin[mode][i]),
  );
  document.documentElement.style.setProperty(
    "--text-2",
    mode === "dark" ? "#c5c5ce" : "#555561",
  );
  document.documentElement.style.setProperty(
    "--muted",
    mode === "dark" ? "#bcbcc7" : "#5a5a65",
  );
  const safe = /^#[\da-f]{6}$/i.test(accent) ? accent : skin.accent;
  document.documentElement.style.setProperty("--accent", safe);
  document.documentElement.style.setProperty("--selected", safe + "20");
  document.documentElement.style.setProperty("--accent-ink", accentInk(safe));
}
