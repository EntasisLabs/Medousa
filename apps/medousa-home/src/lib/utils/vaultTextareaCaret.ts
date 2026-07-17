/** Approximate caret pixel position inside a textarea (for slash menu anchoring). */

export interface CaretPixelPosition {
  top: number;
  left: number;
  height: number;
}

/**
 * Returns caret coordinates relative to `relativeTo` (usually the editor shell),
 * suitable for `position: absolute` slash menus.
 */
export function getTextareaCaretPosition(
  textarea: HTMLTextAreaElement,
  position: number,
  relativeTo?: HTMLElement | null,
): CaretPixelPosition {
  const style = getComputedStyle(textarea);
  const mirror = document.createElement("div");
  mirror.style.cssText = [
    "position:absolute",
    "visibility:hidden",
    "white-space:pre-wrap",
    "word-wrap:break-word",
    "overflow:hidden",
    "top:0",
    "left:-9999px",
    `box-sizing:${style.boxSizing}`,
    `width:${textarea.clientWidth}px`,
    `padding:${style.paddingTop} ${style.paddingRight} ${style.paddingBottom} ${style.paddingLeft}`,
    `border:${style.border}`,
    `font:${style.font}`,
    `letter-spacing:${style.letterSpacing}`,
    `text-align:${style.textAlign}`,
    `text-transform:${style.textTransform}`,
    `line-height:${style.lineHeight}`,
    `tab-size:${style.tabSize}`,
  ].join(";");

  const value = textarea.value;
  mirror.textContent = value.slice(0, position);
  const marker = document.createElement("span");
  marker.textContent = value.slice(position, position + 1) || ".";
  mirror.appendChild(marker);
  document.body.appendChild(mirror);

  const lineHeight =
    Number.parseFloat(style.lineHeight) ||
    Number.parseFloat(style.fontSize) * 1.5 ||
    20;

  const caretInTextarea = {
    top: marker.offsetTop - textarea.scrollTop,
    left: marker.offsetLeft - textarea.scrollLeft,
  };
  document.body.removeChild(mirror);

  const textareaRect = textarea.getBoundingClientRect();
  const origin = relativeTo?.getBoundingClientRect() ?? textareaRect;

  return {
    top: Math.max(
      0,
      caretInTextarea.top + (textareaRect.top - origin.top) + lineHeight + 4,
    ),
    left: Math.max(8, caretInTextarea.left + (textareaRect.left - origin.left)),
    height: lineHeight,
  };
}
