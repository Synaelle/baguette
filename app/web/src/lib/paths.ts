// SPDX-License-Identifier: AGPL-3.0-or-later
// Paths on both sides. The server's are "/a/b". The PC's are "C:\a\b" on
// Windows and "/a/b" on Linux, with "" for This PC (the drives or mount points).

export type Side = 'local' | 'remote'

export function isRoot(side: Side, p: string): boolean {
  return side === 'local' ? p === '' : p === '/'
}

export function parent(side: Side, p: string, linux: boolean): string {
  if (side === 'local' && linux) {
    // "/a/b" -> "/a", "/a" -> "/", "/" -> "" (This PC)
    const t = p.replace(/\/+$/, '')
    if (t === '') return ''
    const i = t.lastIndexOf('/')
    return i <= 0 ? '/' : t.slice(0, i)
  }
  if (side === 'local') {
    const t = p.replace(/\\+$/, '')
    const i = t.lastIndexOf('\\')
    if (i < 0) return ''
    const head = t.slice(0, i)
    return head.endsWith(':') ? head + '\\' : head
  }
  const i = p.lastIndexOf('/')
  return i <= 0 ? '/' : p.slice(0, i)
}

export function join(side: Side, d: string, n: string, linux: boolean): string {
  if (side === 'local') {
    const sep = linux ? '/' : '\\'
    return d === '' ? n : d.endsWith(sep) ? d + n : d + sep + n
  }
  return d === '/' ? '/' + n : d + '/' + n
}

/** a is b or inside it (3DS paths, any case) */
export function inside(a: string, b: string): boolean {
  const x = a.replace(/\/+$/, '').toLowerCase()
  const y = b.replace(/\/+$/, '').toLowerCase()
  return x === y || x.startsWith(y + '/')
}
