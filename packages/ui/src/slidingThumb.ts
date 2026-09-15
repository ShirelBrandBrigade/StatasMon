/**
 * A sliding highlight for segmented controls, tab rows and nav lists: the
 * container draws one thumb (`::before`) whose position and size follow the
 * active child through CSS variables, so a selection change glides instead
 * of cutting. Call again whenever the active child or the layout changes.
 */
export function positionThumb(container: HTMLElement | null, active: HTMLElement | null): void {
  if (!container) return;
  if (!active) {
    container.dataset.thumb = "off";
    return;
  }
  const c = container.getBoundingClientRect();
  const a = active.getBoundingClientRect();
  const style = container.style;
  style.setProperty("--thumb-x", `${Math.round(a.left - c.left - container.clientLeft)}px`);
  style.setProperty("--thumb-y", `${Math.round(a.top - c.top - container.clientTop)}px`);
  style.setProperty("--thumb-w", `${Math.round(a.width)}px`);
  style.setProperty("--thumb-h", `${Math.round(a.height)}px`);
  container.dataset.thumb = "on";
  // The first placement must not slide in from the corner.
  if (container.dataset.thumbReady !== "true") {
    requestAnimationFrame(() => {
      container.dataset.thumbReady = "true";
    });
  }
}
