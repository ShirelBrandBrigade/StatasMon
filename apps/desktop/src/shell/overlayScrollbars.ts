/**
 * Overlay scrollbars drawn by the app (ADR-018): the native bars are hidden
 * on every scroll container and a thin rounded thumb on the app palette is
 * drawn instead, visible while scrolling or hovering and fading out after.
 * One installer watches the document, so every present and future scroll
 * area (the page, the log lists, tables, panels) gets the same bar on
 * macOS and Windows alike. No dependency on how the webview styles
 * `::-webkit-scrollbar`.
 */

/** Containers that get an overlay bar (plus anything with `.sm-scroll`). */
const SELECTOR = [
  ".main",
  ".dev-list",
  ".log-picker-list",
  ".sess-list",
  ".sess-detail",
  ".crash",
  ".crash-detail",
  ".pt-scroll",
  ".ad",
  ".sm-scroll",
  "[data-virtuoso-scroller]",
].join(",");

const MIN_THUMB = 28;
const HIDE_AFTER_MS = 900;
const EDGE = 3;
const THICKNESS = 6;

interface Bar {
  el: HTMLElement;
  y: HTMLElement;
  x: HTMLElement;
  hideTimer: number | undefined;
  update: () => void;
  detach: () => void;
}

const bars = new WeakMap<HTMLElement, Bar>();

function fillsParent(el: HTMLElement): boolean {
  const parent = el.parentElement;
  if (!parent) return false;
  const position = getComputedStyle(parent).position;
  if (position !== "relative" && position !== "absolute" && position !== "fixed") return false;
  const a = el.getBoundingClientRect();
  const b = parent.getBoundingClientRect();
  return (
    Math.abs(a.top - b.top) <= 2 &&
    Math.abs(a.bottom - b.bottom) <= 2 &&
    Math.abs(a.right - b.right) <= 2
  );
}

function attach(scroller: HTMLElement): void {
  if (bars.has(scroller)) return;
  scroller.classList.add("sm-osb-host");

  const el = document.createElement("div");
  el.className = "sm-osb";
  const y = document.createElement("div");
  y.className = "sm-osb-thumb sm-osb-y";
  const x = document.createElement("div");
  x.className = "sm-osb-thumb sm-osb-x";
  el.append(y, x);

  // Host mode: the bar sits in a positioned parent the scroller fills, so it
  // never scrolls away. Sticky mode: a zero-height sticky first child.
  const hostMode = fillsParent(scroller);
  if (hostMode) {
    el.dataset.mode = "host";
    scroller.parentElement?.appendChild(el);
  } else {
    el.dataset.mode = "sticky";
    const gap = Number.parseFloat(getComputedStyle(scroller).rowGap);
    if (Number.isFinite(gap) && gap > 0) el.style.marginBottom = `${-gap}px`;
    scroller.insertBefore(el, scroller.firstChild);
  }

  const bar: Bar = {
    el,
    y,
    x,
    hideTimer: undefined,
    update: () => undefined,
    detach: () => undefined,
  };

  const show = () => {
    el.dataset.visible = "true";
    if (bar.hideTimer !== undefined) window.clearTimeout(bar.hideTimer);
    bar.hideTimer = window.setTimeout(() => {
      if (el.dataset.dragging !== "true") el.dataset.visible = "false";
    }, HIDE_AFTER_MS);
  };

  const update = () => {
    const { scrollHeight, clientHeight, scrollTop, scrollWidth, clientWidth, scrollLeft } =
      scroller;
    const vertical = scrollHeight - clientHeight > 1;
    const horizontal = scrollWidth - clientWidth > 1;
    y.hidden = !vertical;
    x.hidden = !horizontal;
    if (vertical) {
      const track = clientHeight - EDGE * 2 - (horizontal ? THICKNESS + EDGE : 0);
      const h = Math.max(MIN_THUMB, Math.round((clientHeight / scrollHeight) * track));
      const top = Math.round((scrollTop / (scrollHeight - clientHeight)) * (track - h));
      y.style.height = `${h}px`;
      y.style.transform = `translateY(${EDGE + top}px)`;
    }
    if (horizontal) {
      const track = clientWidth - EDGE * 2 - (vertical ? THICKNESS + EDGE : 0);
      const w = Math.max(MIN_THUMB, Math.round((clientWidth / scrollWidth) * track));
      const left = Math.round((scrollLeft / (scrollWidth - clientWidth)) * (track - w));
      x.style.width = `${w}px`;
      x.style.transform = `translateX(${EDGE + left}px)`;
    }
    if (hostMode) {
      el.style.height = `${clientHeight}px`;
      el.style.width = `${clientWidth}px`;
    } else {
      el.style.setProperty("--osb-h", `${clientHeight}px`);
      el.style.setProperty("--osb-w", `${clientWidth}px`);
    }
  };
  bar.update = update;

  const onScroll = () => {
    update();
    show();
  };
  const onEnter = () => {
    update();
    show();
  };

  // Dragging the thumb scrolls the container.
  const drag = (thumb: HTMLElement, axis: "y" | "x") => (e: PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    el.dataset.dragging = "true";
    const startPos = axis === "y" ? e.clientY : e.clientX;
    const startScroll = axis === "y" ? scroller.scrollTop : scroller.scrollLeft;
    const client = axis === "y" ? scroller.clientHeight : scroller.clientWidth;
    const total = axis === "y" ? scroller.scrollHeight : scroller.scrollWidth;
    const thumbSize = axis === "y" ? thumb.offsetHeight : thumb.offsetWidth;
    const track = client - EDGE * 2 - thumbSize;
    const onMove = (m: PointerEvent) => {
      const delta = (axis === "y" ? m.clientY : m.clientX) - startPos;
      const next = startScroll + (delta / Math.max(1, track)) * (total - client);
      if (axis === "y") scroller.scrollTop = next;
      else scroller.scrollLeft = next;
    };
    const onUp = () => {
      el.dataset.dragging = "false";
      show();
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };
  const dragY = drag(y, "y");
  const dragX = drag(x, "x");
  y.addEventListener("pointerdown", dragY);
  x.addEventListener("pointerdown", dragX);

  scroller.addEventListener("scroll", onScroll, { passive: true });
  scroller.addEventListener("pointerenter", onEnter);
  let ro: ResizeObserver | undefined;
  if (typeof ResizeObserver !== "undefined") {
    ro = new ResizeObserver(update);
    ro.observe(scroller);
    for (const child of scroller.children) if (child !== el) ro.observe(child);
  }
  update();

  bar.detach = () => {
    scroller.removeEventListener("scroll", onScroll);
    scroller.removeEventListener("pointerenter", onEnter);
    y.removeEventListener("pointerdown", dragY);
    x.removeEventListener("pointerdown", dragX);
    ro?.disconnect();
    if (bar.hideTimer !== undefined) window.clearTimeout(bar.hideTimer);
    el.remove();
    scroller.classList.remove("sm-osb-host");
    bars.delete(scroller);
  };
  bars.set(scroller, bar);
}

function scan(root: ParentNode): void {
  if (root instanceof HTMLElement && root.matches(SELECTOR)) attach(root);
  for (const el of root.querySelectorAll<HTMLElement>(SELECTOR)) attach(el);
}

/**
 * Attaches overlay bars to every matching scroll container now and as the
 * DOM changes; returns the uninstaller. Safe to call in jsdom (no-op there).
 */
export function installOverlayScrollbars(root: ParentNode = document.body): () => void {
  if (typeof MutationObserver === "undefined" || typeof window === "undefined") return () => {};
  scan(root);
  const tracked = new Set<HTMLElement>();
  const observer = new MutationObserver((records) => {
    for (const r of records) {
      for (const n of r.addedNodes) if (n instanceof HTMLElement) scan(n);
      for (const n of r.removedNodes) {
        if (!(n instanceof HTMLElement)) continue;
        const gone = [n, ...n.querySelectorAll<HTMLElement>(SELECTOR)];
        for (const g of gone) if (!g.isConnected) bars.get(g)?.detach();
      }
    }
  });
  observer.observe(root, { childList: true, subtree: true });
  // Content can grow without a DOM mutation on the scroller itself (canvas
  // resize, text change); a light tick keeps thumbs sized.
  const tick = window.setInterval(() => {
    for (const el of root.querySelectorAll<HTMLElement>(SELECTOR)) {
      const bar = bars.get(el);
      if (bar) {
        bar.update();
        tracked.add(el);
      }
    }
  }, 1000);
  return () => {
    observer.disconnect();
    window.clearInterval(tick);
    for (const el of tracked) bars.get(el)?.detach();
  };
}
