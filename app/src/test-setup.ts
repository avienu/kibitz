/**
 * Vitest setup (vite.config.ts → test.setupFiles).
 *
 * Node >= 25 ships an experimental Web Storage `localStorage` global whose
 * methods are unusable unless node runs with `--localstorage-file`, and it
 * shadows the jsdom test environment's storage. Replace any non-functional
 * `localStorage` with a spec-shaped in-memory implementation so tests
 * behave identically under CI's node 22 and newer local nodes.
 */
function memoryStorage(): Storage {
  const map = new Map<string, string>();
  const storage = {
    getItem: (k: string) => (map.has(k) ? (map.get(k) as string) : null),
    setItem: (k: string, v: string) => void map.set(k, String(v)),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: (i: number) => [...map.keys()][i] ?? null,
    get length() {
      return map.size;
    },
  };
  return storage as Storage;
}

const broken = (() => {
  try {
    return typeof globalThis.localStorage?.getItem !== "function";
  } catch {
    return true; // accessing the getter itself threw
  }
})();

if (broken) {
  try {
    Object.defineProperty(globalThis, "localStorage", {
      value: memoryStorage(),
      configurable: true,
      writable: true,
    });
  } catch {
    // Non-configurable property: last resort, plain assignment.
    (globalThis as { localStorage: Storage }).localStorage = memoryStorage();
  }
}
