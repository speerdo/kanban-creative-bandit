// The HTML `autofocus` attribute only works on page load: browsers ignore it once anything else
// has focus, which is always the case for a box that appears after a click. This focuses the
// element when it mounts instead.
export function autofocus(node: HTMLElement) {
  queueMicrotask(() => {
    node.focus();
    if (node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement) {
      // Put the caret at the end, ready to keep typing.
      const end = node.value.length;
      node.setSelectionRange(end, end);
    }
  });
}
