// 通用 UI 原语:模态框、Toast 容器、确认对话框、分段控件
import React, { useEffect, useId, useRef } from "react";
import { useApp } from "../store/app";
import { Icon } from "./Icon";

/** 可聚焦元素选择器(焦点陷阱用) */
const FOCUSABLE =
  'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

export function Modal(props: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  footer?: React.ReactNode;
  width?: number;
}) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const titleId = useId();

  useEffect(() => {
    // 打开时记住来源焦点,关闭时归还 —— 键盘用户不应被扔回文档开头
    const restoreTo = document.activeElement as HTMLElement | null;
    const node = dialogRef.current;
    // 焦点移入对话框:优先第一个可聚焦元素,否则对话框本身
    const first = node?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? node)?.focus();

    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        props.onClose();
        return;
      }
      // Tab 焦点陷阱:焦点不得逃逸到背后的页面
      if (e.key !== "Tab" || !node) return;
      const items = Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
        (el) => el.offsetParent !== null,
      );
      if (items.length === 0) {
        e.preventDefault();
        return;
      }
      const firstItem = items[0];
      const lastItem = items[items.length - 1];
      const active = document.activeElement;
      if (!e.shiftKey && active === lastItem) {
        e.preventDefault();
        firstItem.focus();
      } else if (e.shiftKey && (active === firstItem || active === node)) {
        e.preventDefault();
        lastItem.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      restoreTo?.focus?.();
    };
    // props.onClose 变化不应重装(会导致焦点反复重置)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      className="modal-mask"
      onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}
    >
      <div
        className="modal"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        tabIndex={-1}
        style={props.width ? { width: `min(${props.width}px, calc(100vw - 80px))` } : undefined}
      >
        <div className="row mb16">
          <h3 id={titleId} style={{ margin: 0, fontSize: 16 }}>{props.title}</h3>
          <div className="grow" />
          <button className="btn ghost sm" onClick={props.onClose} aria-label="关闭对话框">
            <Icon name="close" size={16} />
          </button>
        </div>
        <div>{props.children}</div>
        {props.footer && <div className="row mt20" style={{ justifyContent: "flex-end" }}>{props.footer}</div>}
      </div>
    </div>
  );
}

export function Toasts() {
  const { toasts, dismissToast } = useApp();
  const icons = { success: "check", error: "alert", warn: "alert", info: "info" } as const;
  const colors = { success: "var(--green)", error: "var(--red)", warn: "var(--amber)", info: "var(--cyan)" } as const;
  return (
    <div className="toasts">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind}`} onClick={() => dismissToast(t.id)}>
          <span style={{ color: colors[t.kind], display: "grid", placeItems: "center" }}>
            <Icon name={icons[t.kind]} size={17} />
          </span>
          <span className="grow">{t.text}</span>
        </div>
      ))}
    </div>
  );
}

/** 确认对话框(命令式) */
export function ConfirmModal(props: {
  title: string;
  text: string;
  danger?: boolean;
  onOk: () => void;
  onClose: () => void;
}) {
  return (
    <Modal
      title={props.title}
      onClose={props.onClose}
      footer={
        <>
          <button className="btn" onClick={props.onClose}>取消</button>
          <button
            className={`btn ${props.danger ? "danger" : "primary"}`}
            onClick={() => {
              props.onOk();
              props.onClose();
            }}
          >
            确认
          </button>
        </>
      }
    >
      <p className="dim" style={{ margin: 0, lineHeight: 1.8 }}>{props.text}</p>
    </Modal>
  );
}

export function Segmented<T extends string>(props: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
}) {
  return (
    <div className="segmented">
      {props.options.map((o) => (
        <button
          key={o.value}
          className={props.value === o.value ? "active" : ""}
          onClick={() => props.onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
