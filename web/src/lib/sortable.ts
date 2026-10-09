// Svelte action wrapping SortableJS for task lists (board columns and list sections).
//
// SortableJS moves DOM nodes itself, but Svelte owns this DOM. So on drop we put the node back
// exactly where it was and report the move; the store updates state and Svelte re-renders.
// Containers carry `data-status`, items carry `data-task`.

import Sortable from 'sortablejs';

export type SortableOptions = {
  /** Lists sharing a group can exchange items (one group per project). */
  group: string;
  /** Restrict dragging to a grip inside the item (list rows); omit to drag the whole item. */
  handle?: string;
  onmove: (taskId: number, toStatusId: number, index: number) => void;
};

export function sortable(node: HTMLElement, options: SortableOptions) {
  let opts = options;
  let anchor: Node | null = null;

  const instance = Sortable.create(node, {
    group: opts.group,
    handle: opts.handle,
    draggable: '[data-task]',
    // Buttons and inputs inside a card stay clickable and never start a drag.
    filter: 'button, input, select, a, .no-drag',
    preventOnFilter: false,
    animation: 150,
    // On touch screens a short press starts a drag, so plain swipes still scroll.
    delay: 180,
    delayOnTouchOnly: true,
    touchStartThreshold: 4,
    ghostClass: 'drag-ghost',
    chosenClass: 'drag-chosen',
    dragClass: 'drag-active',
    fallbackOnBody: true,
    onStart(evt) {
      anchor = evt.item.nextSibling;
    },
    onEnd(evt) {
      const { item, from, to } = evt;
      from.insertBefore(item, anchor); // undo Sortable's DOM move
      anchor = null;
      const index = evt.newDraggableIndex;
      if (index === undefined) return;
      opts.onmove(Number(item.dataset.task), Number(to.dataset.status), index);
    },
  });

  return {
    update(next: SortableOptions) {
      opts = next;
      instance.option('group', next.group);
    },
    destroy() {
      instance.destroy();
    },
  };
}
