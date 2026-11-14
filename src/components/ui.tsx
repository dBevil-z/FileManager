import { t } from "../i18n";
import {
  useEffect,
  useRef,
  useId,
  Children,
  isValidElement,
  cloneElement,
  type ReactElement,
  type ReactNode,
} from "react";
import { X, LoaderCircle, AlertCircle, ChevronDown, Check } from "lucide-react";
export function IconButton({
  children,
  label,
  active,
  className = "",
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  active?: boolean;
}) {
  return (
    <button
      type="button"
      {...props}
      title={label}
      aria-label={label}
      className={`icon-button ${active ? "active" : ""} ${className}`}
    >
      {children}
    </button>
  );
}
export function Button({
  children,
  tone = "normal",
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: "normal" | "primary" | "danger";
}) {
  return (
    <button
      type="button"
      {...props}
      className={`button ${tone} ${props.className || ""}`}
    >
      {children}
    </button>
  );
}
export function Empty({
  icon,
  title,
  children,
}: {
  icon: ReactNode;
  title: string;
  children?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      <div>{children}</div>
    </div>
  );
}
export function Loading({ label = t("正在载入…") }: { label?: string }) {
  return (
    <div className="loading">
      <LoaderCircle size={18} className="spin" />
      {label}
    </div>
  );
}
export function ErrorState({
  error,
  retry,
}: {
  error: string;
  retry?: () => void;
}) {
  return (
    <div className="error-state">
      <AlertCircle size={22} />
      <strong>{t("无法完成操作")}</strong>
      <p>{error}</p>
      {retry && <Button onClick={retry}>{t("重新尝试")}</Button>}
    </div>
  );
}
export function Modal({
  title,
  children,
  onClose,
  wide = false,
  footer,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
  footer?: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const previous = document.activeElement as HTMLElement;
    const k = (e: KeyboardEvent) => {
      if (ref.current !== [...document.querySelectorAll(".modal")].at(-1))
        return;
      if (e.key === "Escape") {
        e.stopPropagation();
        closeRef.current();
      }
      if (e.key === "Tab") {
        const els = ref.current?.querySelectorAll<HTMLElement>(
          'button:not(:disabled),input,textarea,select,[tabindex="0"]',
        );
        if (!els?.length) return;
        const first = els[0],
          last = els[els.length - 1];
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    window.addEventListener("keydown", k, true);
    const localValue = window.setTimeout(
      () =>
        (
          ref.current?.querySelector<HTMLElement>("[autofocus]") ||
          ref.current?.querySelector<HTMLElement>("input,textarea,select") ||
          ref.current?.querySelector<HTMLElement>("button")
        )?.focus(),
      40,
    );
    return () => {
      window.clearTimeout(localValue);
      window.removeEventListener("keydown", k, true);
      previous?.focus();
    };
  }, []);
  return (
    <div
      className="modal-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={`modal ${wide ? "wide" : ""}`}
        ref={ref}
      >
        <header>
          <h2>{title}</h2>
          <IconButton label={t("关闭")} onClick={onClose}>
            <X size={17} />
          </IconButton>
        </header>
        <div className="modal-content">{children}</div>
        {footer && <footer>{footer}</footer>}
      </div>
    </div>
  );
}
export type MenuItem = {
  label: string;
  icon?: ReactNode;
  key?: string;
  danger?: boolean;
  disabled?: boolean;
  action?: () => void;
  separator?: boolean;
};
export function Menu({
  items,
  x,
  y,
  close,
}: {
  items: MenuItem[];
  x: number;
  y: number;
  close: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const click = () => close();
    window.addEventListener("pointerdown", click);
    const k = (e: KeyboardEvent) => {
      if (
        ref.current !== [...document.querySelectorAll(".context-menu")].at(-1)
      )
        return;
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", k);
    const box = ref.current?.getBoundingClientRect();
    if (box && ref.current) {
      ref.current.style.left = `${Math.max(8, Math.min(x, window.innerWidth - box.width - 8))}px`;
      ref.current.style.top = `${Math.max(8, Math.min(y, window.innerHeight - box.height - 8))}px`;
    }
    return () => {
      window.removeEventListener("pointerdown", click);
      window.removeEventListener("keydown", k);
    };
  }, [x, y, close]);
  return (
    <div
      role="menu"
      ref={ref}
      className="context-menu"
      style={{ left: x, top: y }}
      onPointerDown={(e) => e.stopPropagation()}
    >
      {items.map((item, i) =>
        item.separator ? (
          <hr key={i} />
        ) : (
          <button
            role="menuitem"
            key={i}
            disabled={item.disabled}
            className={item.danger ? "danger-text" : ""}
            onClick={() => {
              close();
              item.action?.();
            }}
          >
            {item.icon || <span className="menu-icon-space" />}
            <span>{item.label}</span>
            {item.key && <kbd>{item.key}</kbd>}
          </button>
        ),
      )}
    </div>
  );
}
export function Select({
  children,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className="select-wrap">
      <select {...props}>{children}</select>
      <ChevronDown size={13} />
    </span>
  );
}
export function Toggle({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`toggle ${checked ? "on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span>{checked && <Check size={10} />}</span>
    </button>
  );
}
export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: ReactNode;
  hint?: string;
}) {
  const id = useId();
  let linked = false;
  const attach = (nodes: ReactNode): ReactNode =>
    Children.map(nodes, (child) => {
      if (!isValidElement(child)) return child;
      const element = child as ReactElement<{
        id?: string;
        children?: ReactNode;
      }>;
      if (
        !linked &&
        (["input", "textarea", "select"].includes(String(element.type)) ||
          element.type === Select)
      ) {
        linked = true;
        return cloneElement(element, { id });
      }
      if (element.props.children)
        return cloneElement(element, {
          children: attach(element.props.children),
        });
      return element;
    });
  const content = attach(children);
  return (
    <div className="field">
      {linked ? (
        <label htmlFor={id}>{label}</label>
      ) : (
        <span className="field-label">{label}</span>
      )}
      {content}
      {hint && <small>{hint}</small>}
    </div>
  );
}
export function Badge({
  children,
  color,
}: {
  children: ReactNode;
  color?: string;
}) {
  return (
    <span
      className="badge"
      style={color ? { color, backgroundColor: `${color}18` } : undefined}
    >
      {children}
    </span>
  );
}
