// 通用 UI 原语:模态框、Toast 容器、确认对话框、分段控件
import React, { useEffect } from "react";
import { useApp } from "../store/app";
import { Icon } from "./Icon";

export function Modal(props: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  footer?: React.ReactNode;
  width?: number;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") props.onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [props]);
  return (
    <div className="modal-mask" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="modal" style={props.width ? { width: `min(${props.width}px, calc(100vw - 80px))` } : undefined}>
        <div className="row mb16">
          <h3 style={{ margin: 0, fontSize: 16 }}>{props.title}</h3>
          <div className="grow" />
          <button className="btn ghost sm" onClick={props.onClose} aria-label="关闭">
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
