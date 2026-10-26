/**
 * Local face gallery in OPFS (`faces/`). Consent + forget-all (docs/26 §8).
 */

const DIR = "faces";

async function rootDir(create = true) {
  const root = await navigator.storage.getDirectory();
  return root.getDirectoryHandle(DIR, { create });
}

/**
 * @returns {Promise<Array<{ id: string, name: string, consent: string, created: number }>>}
 */
export async function listFaces() {
  try {
    const dir = await rootDir(false);
    const out = [];
    for await (const [name, handle] of dir.entries()) {
      if (handle.kind !== "directory") continue;
      try {
        const fh = await handle.getFileHandle("meta.json");
        const file = await fh.getFile();
        const meta = JSON.parse(await file.text());
        out.push({ id: name, ...meta });
      } catch {
        /* skip broken */
      }
    }
    return out.sort((a, b) => (b.created || 0) - (a.created || 0));
  } catch {
    return [];
  }
}

/**
 * @param {{ id: string, name: string, consent: string, frontBlob: Blob }} entry
 */
export async function saveFace(entry) {
  const dir = await rootDir(true);
  const face = await dir.getDirectoryHandle(entry.id, { create: true });
  const meta = {
    name: entry.name,
    consent: entry.consent,
    created: Date.now(),
  };
  const mh = await face.getFileHandle("meta.json", { create: true });
  const mw = await mh.createWritable();
  await mw.write(JSON.stringify(meta));
  await mw.close();
  if (entry.frontBlob) {
    const fh = await face.getFileHandle("front.png", { create: true });
    const fw = await fh.createWritable();
    await fw.write(entry.frontBlob);
    await fw.close();
  }
}

/** @param {string} id */
export async function deleteFace(id) {
  const dir = await rootDir(true);
  await dir.removeEntry(id, { recursive: true });
}

/** Forget all faces + clear caches (caller drops live overrides). */
export async function forgetAllFaces() {
  try {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry(DIR, { recursive: true });
  } catch {
    /* already empty */
  }
}
