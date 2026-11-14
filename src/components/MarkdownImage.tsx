import { t } from "../i18n";
import { mediaUrl, request } from "../api";
import { useAsync } from "../hooks/useAsync";
export function MarkdownImage({
  document,
  src,
  alt,
}: {
  document: string;
  src?: string;
  alt?: string;
}) {
  const remote = !!src && /^https?:\/\//.test(src);
  const load = useAsync(
    () =>
      !src || remote
        ? Promise.resolve(null)
        : request<{ media: string }>("markdown_image", {
            document,
            source: src,
          }),
    [document, src],
  );
  const url = remote
    ? src
    : load.data?.media
      ? mediaUrl(load.data.media)
      : null;
  return url ? (
    <img src={url} alt={alt || ""} loading="lazy" />
  ) : (
    <span className="muted" title={load.error || undefined}>
      {t("[图片：")}
      {alt || src}]
    </span>
  );
}
