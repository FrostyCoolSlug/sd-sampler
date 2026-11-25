/** A device list may be flat or grouped (a group is an item with `children`). Flatten to leaf options. */
export const flatten = (items) => items.flatMap((item) => item.children || [item])

/** Last path segment of a file path. */
export const basename = (path) => String(path).split('/').pop()

/** Human duration: 500 -> "500 ms", 1200 -> "1.2 s", 2000 -> "2 s". */
export const formatMs = (ms) => {
  if (ms < 1000) return `${ms} ms`
  const seconds = ms / 1000
  return `${seconds.toFixed(ms % 1000 ? 1 : 0)} s`
}

/** Cheap deep-equality for the plain settings objects that arrive in messages. */
export const isSame = (a, b) => JSON.stringify(a) === JSON.stringify(b)
