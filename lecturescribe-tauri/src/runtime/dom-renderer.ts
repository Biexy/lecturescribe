import { createContext, type ReactNode } from "react";
import createReconciler from "react-reconciler";
import {
  ConcurrentRoot,
  ContinuousEventPriority,
  DefaultEventPriority,
  DiscreteEventPriority,
  NoEventPriority,
} from "react-reconciler/constants.js";
import * as Scheduler from "scheduler";

type HostElement = HTMLElement | SVGElement;
type HostNode = HostElement | Text;
type HostContext = "html" | "svg";
type Props = Record<string, unknown>;

const listeners = new WeakMap<EventTarget, Map<string, EventListener>>();
const unitlessStyles = new Set([
  "animationIterationCount",
  "columnCount",
  "flex",
  "flexGrow",
  "flexShrink",
  "fontWeight",
  "gridColumn",
  "gridRow",
  "lineHeight",
  "opacity",
  "order",
  "orphans",
  "scale",
  "tabSize",
  "widows",
  "zIndex",
  "zoom",
]);

let currentUpdatePriority = NoEventPriority;

function eventName(property: string): { name: string; capture: boolean } {
  const capture = property.endsWith("Capture");
  const bare = property.slice(2, capture ? -7 : undefined);
  const name = bare === "DoubleClick" ? "dblclick" : bare.toLowerCase();
  return { name, capture };
}

function priorityForEvent(name: string): number {
  if (/^(click|dblclick|contextmenu|input|change|submit|keydown|keyup|pointerdown|pointerup)$/.test(name)) {
    return DiscreteEventPriority;
  }
  if (/^(mousemove|pointermove|scroll|wheel|drag|dragover)$/.test(name)) {
    return ContinuousEventPriority;
  }
  return DefaultEventPriority;
}

function setEvent(element: HostElement, property: string, value: unknown): void {
  const registered = listeners.get(element) ?? new Map<string, EventListener>();
  listeners.set(element, registered);
  const existing = registered.get(property);
  const { name, capture } = eventName(property);
  if (existing) {
    element.removeEventListener(name, existing, capture);
    registered.delete(property);
  }
  if (typeof value !== "function") {
    return;
  }
  const handler = value as (event: Event) => void;
  const listener: EventListener = (event) => {
    const previous = currentUpdatePriority;
    currentUpdatePriority = priorityForEvent(name);
    try {
      handler(event);
    } finally {
      currentUpdatePriority = previous;
    }
  };
  registered.set(property, listener);
  element.addEventListener(name, listener, capture);
}

function styleValue(property: string, value: unknown): string {
  if (typeof value === "number" && value !== 0 && !unitlessStyles.has(property)) {
    return `${value}px`;
  }
  return String(value ?? "");
}

function setStyles(
  element: HostElement,
  previous: Record<string, unknown> | undefined,
  next: Record<string, unknown> | undefined,
): void {
  const style = (element as HTMLElement).style;
  if (!style) {
    return;
  }
  for (const property of Object.keys(previous ?? {})) {
    if (!(property in (next ?? {}))) {
      if (property.startsWith("--")) {
        style.removeProperty(property);
      } else {
        (style as unknown as Record<string, string>)[property] = "";
      }
    }
  }
  for (const [property, value] of Object.entries(next ?? {})) {
    if (property.startsWith("--")) {
      style.setProperty(property, styleValue(property, value));
    } else {
      (style as unknown as Record<string, string>)[property] = styleValue(property, value);
    }
  }
}

function svgAttributeName(property: string): string {
  const aliases: Record<string, string> = {
    className: "class",
    strokeLinecap: "stroke-linecap",
    strokeLinejoin: "stroke-linejoin",
    strokeWidth: "stroke-width",
    tabIndex: "tabindex",
  };
  return aliases[property] ?? property.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
}

function setProperty(
  element: HostElement,
  property: string,
  previous: unknown,
  value: unknown,
): void {
  if (property === "children" || property === "key" || property === "ref") {
    return;
  }
  if (/^on[A-Z]/.test(property)) {
    setEvent(element, property, value);
    return;
  }
  if (property === "style") {
    setStyles(
      element,
      previous as Record<string, unknown> | undefined,
      value as Record<string, unknown> | undefined,
    );
    return;
  }
  if (property === "dangerouslySetInnerHTML") {
    const html = (value as { __html?: string } | undefined)?.__html;
    element.innerHTML = html ?? "";
    return;
  }

  const attribute = element instanceof SVGElement
    ? svgAttributeName(property)
    : property === "className"
      ? "class"
      : property === "htmlFor"
        ? "for"
        : property;
  if (value === null || value === undefined || value === false) {
    element.removeAttribute(attribute);
    if (property in element && !property.startsWith("aria-")) {
      try {
        (element as unknown as Record<string, unknown>)[property] =
          typeof previous === "boolean" ? false : "";
      } catch {
        // Read-only DOM properties are represented by attributes only.
      }
    }
    return;
  }
  if (value === true) {
    element.setAttribute(attribute, "");
    if (property in element) {
      try {
        (element as unknown as Record<string, unknown>)[property] = true;
      } catch {
        // Read-only DOM properties are represented by attributes only.
      }
    }
    return;
  }
  if (
    !(element instanceof SVGElement) &&
    property in element &&
    !property.startsWith("aria-") &&
    !property.startsWith("data-") &&
    property !== "list" &&
    property !== "form" &&
    property !== "role"
  ) {
    try {
      (element as unknown as Record<string, unknown>)[property] = value;
      return;
    } catch {
      // Fall through to an attribute for browser-managed properties.
    }
  }
  element.setAttribute(attribute, String(value));
}

function applyProps(element: HostElement, previous: Props, next: Props): void {
  const properties = new Set([...Object.keys(previous), ...Object.keys(next)]);
  for (const property of properties) {
    if (previous[property] !== next[property]) {
      setProperty(element, property, previous[property], next[property]);
    }
  }
}

const hostConfig: Record<string, unknown> = {
  getRootHostContext: (): HostContext => "html",
  getChildHostContext: (context: HostContext, type: string): HostContext => {
    if (type === "svg") return "svg";
    if (type === "foreignObject") return "html";
    return context;
  },
  prepareForCommit: () => null,
  resetAfterCommit: () => undefined,
  preparePortalMount: () => undefined,
  clearContainer: (container: Element) => {
    container.replaceChildren();
    return false;
  },
  shouldSetTextContent: () => false,
  createInstance: (type: string, props: Props, _root: Element, context: HostContext) => {
    const element = context === "svg" || type === "svg"
      ? document.createElementNS("http://www.w3.org/2000/svg", type)
      : document.createElement(type);
    applyProps(element, {}, props);
    return element;
  },
  createTextInstance: (text: string) => document.createTextNode(text),
  appendInitialChild: (parent: Node, child: Node) => parent.appendChild(child),
  appendChild: (parent: Node, child: Node) => parent.appendChild(child),
  appendChildToContainer: (parent: Node, child: Node) => parent.appendChild(child),
  insertBefore: (parent: Node, child: Node, before: Node) => parent.insertBefore(child, before),
  insertInContainerBefore: (parent: Node, child: Node, before: Node) =>
    parent.insertBefore(child, before),
  removeChild: (parent: Node, child: Node) => parent.removeChild(child),
  removeChildFromContainer: (parent: Node, child: Node) => parent.removeChild(child),
  finalizeInitialChildren: () => false,
  commitMount: () => undefined,
  commitUpdate: (element: HostElement, _type: string, oldProps: Props, newProps: Props) =>
    applyProps(element, oldProps, newProps),
  commitTextUpdate: (text: Text, _oldText: string, nextText: string) => {
    text.nodeValue = nextText;
  },
  resetTextContent: (element: HostElement) => {
    element.textContent = "";
  },
  hideInstance: (element: HostElement) => element.setAttribute("hidden", ""),
  unhideInstance: (element: HostElement) => element.removeAttribute("hidden"),
  hideTextInstance: (text: Text) => {
    text.nodeValue = "";
  },
  unhideTextInstance: (text: Text, value: string) => {
    text.nodeValue = value;
  },
  getPublicInstance: (instance: HostNode) => instance,
  isPrimaryRenderer: true,
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  supportsMicrotasks: true,
  scheduleMicrotask: queueMicrotask,
  scheduleCallback: Scheduler.unstable_scheduleCallback,
  cancelCallback: Scheduler.unstable_cancelCallback,
  shouldYield: Scheduler.unstable_shouldYield,
  now: Scheduler.unstable_now,
  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: -1,
  beforeActiveInstanceBlur: () => undefined,
  afterActiveInstanceBlur: () => undefined,
  detachDeletedInstance: () => undefined,
  getInstanceFromNode: (node: HostNode) => node,
  prepareScopeUpdate: () => undefined,
  getInstanceFromScope: () => null,
  setCurrentUpdatePriority: (priority: number) => {
    currentUpdatePriority = priority;
  },
  getCurrentUpdatePriority: () => currentUpdatePriority,
  resolveUpdatePriority: () =>
    currentUpdatePriority === NoEventPriority ? DefaultEventPriority : currentUpdatePriority,
  maySuspendCommit: () => false,
  NotPendingTransition: undefined,
  HostTransitionContext: createContext(null),
  resetFormInstance: (form: HTMLFormElement) => form.reset(),
  requestPostPaintCallback: (callback: (time: number) => void) =>
    requestAnimationFrame(() => callback(Date.now())),
  shouldAttemptEagerTransition: () => false,
  trackSchedulerEvent: () => undefined,
  resolveEventType: () => null,
  resolveEventTimeStamp: () => -1.1,
  preloadInstance: () => true,
  startSuspendingCommit: () => undefined,
  suspendInstance: () => undefined,
  waitForCommitToBeReady: () => null,
  rendererPackageName: "lecturescribe-dom",
  rendererVersion: "0.2.0",
};

const reconciler = createReconciler(hostConfig);

function reportRenderError(error: unknown): void {
  console.error("LectureScribe UI render error", error);
}

export function createRoot(container: HTMLElement) {
  const root = reconciler.createContainer(
    container,
    ConcurrentRoot,
    null,
    false,
    null,
    "lecturescribe",
    reportRenderError,
    reportRenderError,
    reportRenderError,
    () => undefined,
  );
  return {
    render(node: ReactNode) {
      reconciler.updateContainer(node, root, null, () => undefined);
    },
    unmount() {
      reconciler.updateContainer(null, root, null, () => undefined);
    },
  };
}
