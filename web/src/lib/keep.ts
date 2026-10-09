// Reading a Google Keep export from Takeout, in the browser: a .zip (or the .json files picked
// directly) → notes for POST /api/import/keep. The zip reader handles what Takeout writes
// (stored or deflated entries) using the browser's own DecompressionStream, so no library.

import type { KeepNote } from './api';

/** One Takeout Keep note, with just the fields we use. */
type TakeoutNote = {
  title?: string;
  textContent?: string;
  listContent?: { text?: string; isChecked?: boolean }[];
  isArchived?: boolean;
  isTrashed?: boolean;
};

export function toNote(raw: TakeoutNote): KeepNote {
  return {
    title: (raw.title ?? '').trim(),
    text: (raw.textContent ?? '').trim(),
    items: Array.isArray(raw.listContent)
      ? raw.listContent
          .map((i) => ({ text: (i.text ?? '').trim(), checked: !!i.isChecked }))
          .filter((i) => i.text)
      : null,
    archived: !!raw.isArchived,
    trashed: !!raw.isTrashed,
  };
}

/** Entries of a zip file: name → bytes (decompressed lazily). */
export async function unzip(data: Uint8Array, want: (name: string) => boolean): Promise<Map<string, Uint8Array>> {
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  // End of central directory: the last 0x06054b50 within the final 64 KiB (comment max).
  let eocd = -1;
  for (let i = data.length - 22; i >= Math.max(0, data.length - 22 - 65535); i--) {
    if (view.getUint32(i, true) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error("That file isn't a zip.");
  const count = view.getUint16(eocd + 10, true);
  let at = view.getUint32(eocd + 16, true);
  const out = new Map<string, Uint8Array>();
  const decoder = new TextDecoder();
  for (let n = 0; n < count; n++) {
    if (view.getUint32(at, true) !== 0x02014b50) throw new Error('The zip looks damaged.');
    const method = view.getUint16(at + 10, true);
    const size = view.getUint32(at + 20, true);
    const nameLen = view.getUint16(at + 28, true);
    const extraLen = view.getUint16(at + 30, true);
    const commentLen = view.getUint16(at + 32, true);
    const local = view.getUint32(at + 42, true);
    const name = decoder.decode(data.subarray(at + 46, at + 46 + nameLen));
    at += 46 + nameLen + extraLen + commentLen;
    if (!want(name)) continue;
    const start = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
    const raw = data.subarray(start, start + size);
    if (method === 0) out.set(name, raw);
    else if (method === 8) out.set(name, await inflate(raw));
    else throw new Error(`Unsupported compression in ${name}.`);
  }
  return out;
}

async function inflate(raw: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([raw as BlobPart]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** Notes from the files someone picked: a Takeout .zip and/or Keep .json files. */
export async function readKeep(files: File[]): Promise<KeepNote[]> {
  const notes: KeepNote[] = [];
  const decoder = new TextDecoder();
  const add = (text: string) => {
    try {
      const raw = JSON.parse(text);
      if (raw && typeof raw === 'object' && ('textContent' in raw || 'listContent' in raw || 'title' in raw)) {
        notes.push(toNote(raw));
      }
    } catch {
      // Not a note (Takeout also includes HTML and other files).
    }
  };
  for (const f of files) {
    if (f.name.toLowerCase().endsWith('.zip')) {
      const entries = await unzip(new Uint8Array(await f.arrayBuffer()), (n) => /(^|\/)Keep\/[^/]+\.json$/i.test(n));
      for (const bytes of entries.values()) add(decoder.decode(bytes));
    } else if (f.name.toLowerCase().endsWith('.json')) {
      add(await f.text());
    }
  }
  return notes;
}
