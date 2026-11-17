<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- A list of files: the PC side and the server side work the same way.
     Rows are an optional ".." then the entries; row index 0 is ".." when there is one. -->
<script lang="ts">
  import { tick } from 'svelte'
  import { app } from '../lib/app.svelte'
  import { anyModal } from '../lib/dialogs.svelte'
  import { drag, startDrag } from '../lib/drag.svelte'
  import { focusSide, registerList } from '../lib/focus'
  import { formatSize, formatStamp } from '../lib/format'
  import { showMenu } from '../lib/menu.svelte'
  import { isRoot, type Side } from '../lib/paths'
  import { showRefused } from '../lib/refused.svelte'
  import { invoke } from '../lib/tauri'
  import type { Entry } from '../lib/types'
  import Icon from './Icon.svelte'
  import StatusDot from './StatusDot.svelte'

  let { side }: { side: Side } = $props()

  const st = $derived(app.pane(side))
  const hasParent = $derived(!isRoot(side, st.path))
  const rowCount = $derived(st.entries.length + (hasParent ? 1 : 0))
  const entryAt = (i: number): Entry | null =>
    hasParent ? (i === 0 ? null : (st.entries[i - 1] ?? null)) : (st.entries[i] ?? null)
  const thisPc = $derived(side === 'local' && st.path === '')
  const title = $derived(side === 'local' ? 'This PC' : (app.tab?.name ?? 'Server'))
  const readOnly = $derived(side === 'local' ? app.localReadOnly(st.path) : app.readOnly(st.path))
  const dropping = $derived(drag.target?.side === side)

  let list = $state<HTMLElement | null>(null)
  let pathText = $state('')
  let editingPath = $state(false)
  $effect(() => {
    if (!editingPath) pathText = st.path || (side === 'local' ? 'This PC' : '')
  })
  $effect(() => registerList(side, list))

  const icon = (e: Entry) =>
    (thisPc ? 'drive' : e.dir ? 'folder' : 'file') as 'drive' | 'folder' | 'file'

  function typedPath() {
    let p = pathText.trim()
    editingPath = false
    if (side === 'local') {
      if (p.toLowerCase() === 'this pc') p = ''
      if (/^[a-z]:$/i.test(p)) p += '\\'
      void app.navigate('local', p)
    } else {
      void invoke<string>('normalize', { path: p }).then((n) => app.navigate('remote', n || '/'))
    }
    list?.focus()
  }

  // clicks and multi-selection
  function rowDown(ev: MouseEvent, i: number) {
    if ((ev.target as HTMLElement).tagName === 'INPUT') return
    const e = entryAt(i)
    st.focus = i
    if (ev.button === 2 && e && st.selected.has(e.name)) return // right-click keeps the selection
    if (ev.shiftKey && st.anchor >= 0) {
      const [a, b] = [Math.min(st.anchor, i), Math.max(st.anchor, i)]
      const names: string[] = []
      for (let k = a; k <= b; k++) {
        const x = entryAt(k)
        if (x) names.push(x.name)
      }
      app.select(side, names)
    } else if (ev.ctrlKey) {
      if (e) {
        const names = [...st.selected]
        app.select(
          side,
          st.selected.has(e.name) ? names.filter((n) => n !== e.name) : [...names, e.name],
        )
      }
      st.anchor = i
    } else if (!(e && st.selected.has(e.name) && ev.button === 0)) {
      app.select(side, e ? [e.name] : [])
      st.anchor = i
    }
    if (ev.button === 0 && e && st.selected.has(e.name) && !thisPc) startDrag(side, ev)
  }

  function rowClick(ev: MouseEvent, i: number) {
    if (ev.ctrlKey || ev.shiftKey || (ev.target as HTMLElement).tagName === 'INPUT') return
    const e = entryAt(i)
    if (e && st.selected.size > 1 && !drag.moved) app.select(side, [e.name]) // a plain click on a multi-selection
  }

  function key(ev: KeyboardEvent) {
    if (anyModal() || (ev.target as HTMLElement).tagName === 'INPUT') return
    const move = (i: number) => {
      st.focus = Math.max(0, Math.min(rowCount - 1, i))
      const e = entryAt(st.focus)
      app.select(side, e ? [e.name] : [])
      st.anchor = st.focus
      void tick().then(() =>
        list?.querySelector(`[data-i="${st.focus}"]`)?.scrollIntoView({ block: 'nearest' }),
      )
    }
    if (ev.key === 'ArrowDown') move(st.focus + 1)
    else if (ev.key === 'ArrowUp') move(st.focus - 1)
    else if (ev.key === 'Home') move(0)
    else if (ev.key === 'End') move(rowCount - 1)
    else if (ev.key === 'Enter') {
      if (st.focus < rowCount) void app.openEntry(side, entryAt(st.focus))
    } else if (ev.key === 'Backspace') app.up(side)
    else if (ev.key === 'Delete') void app.deleteSelected(side)
    else if (ev.key === 'F2') app.startRename(side)
    else if (ev.key === 'F5') app.refresh(side)
    // Tab goes from the PC side to the server side and Shift+Tab back; past them it leaves
    // the lists. WebKitGTK names Shift+Tab (ISO_Left_Tab) "Unidentified", its code is Tab.
    else if (ev.code === 'Tab' && !ev.shiftKey && side === 'local') focusSide('remote')
    else if (ev.code === 'Tab' && ev.shiftKey && side === 'remote') focusSide('local')
    else if (ev.key === 'a' && ev.ctrlKey)
      app.select(
        side,
        st.entries.map((e) => e.name),
      )
    else if (ev.key === 'ContextMenu' || (ev.key === 'F10' && ev.shiftKey)) {
      const r = (list?.querySelector(`[data-i="${st.focus}"]`) ?? list)?.getBoundingClientRect()
      if (r) showMenu(side, r.left + 40, r.bottom, true)
    } else return
    ev.preventDefault()
  }

  // renaming in place: Enter keeps it, Escape drops it, leaving the field keeps it
  function renameKey(ev: KeyboardEvent) {
    ev.stopPropagation()
    const input = ev.target as HTMLInputElement
    if (ev.key === 'Enter') input.blur()
    if (ev.key === 'Escape') {
      st.renaming = null
      list?.focus()
    }
  }
  function renameStart(node: HTMLInputElement, e: Entry) {
    node.value = e.name
    node.focus()
    const dot = e.dir ? -1 : e.name.lastIndexOf('.')
    node.setSelectionRange(0, dot > 0 ? dot : e.name.length)
  }
  function renameDone(ev: FocusEvent, from: string) {
    if (st.renaming !== from) return
    app.finishRename(side, from, (ev.target as HTMLInputElement).value)
    list?.focus()
  }
</script>

<!-- the mouse only marks the side in use here; the keyboard does it through the list's focus -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  id={side === 'local' ? 'paneLocal' : 'paneRemote'}
  class="pane"
  class:active={app.activeSide === side}
  class:dropping
  aria-label={side === 'local' ? 'This PC' : 'Server'}
  data-side={side}
  onmousedown={() => (app.activeSide = side)}
>
  <div class="pane-head">
    <button class="up" title="Up (Backspace)" aria-label="Up" onclick={() => app.up(side)}
      ><Icon name="up" /></button
    >
    <span
      class="pane-title"
      id={side === 'remote' ? 'remoteTitle' : undefined}
      title={side === 'remote' ? title : undefined}>{title}</span
    >
    {#if readOnly}
      <span
        class="ro-tag"
        title={side === 'local'
          ? 'The Windows folder: Baguette only reads it. Start Baguette with --allow-windows-folder to change that.'
          : 'A protected 3DS folder: it can be looked at and downloaded, not changed. The protection is in the settings.'}
        ><Icon name="lock" />read-only</span
      >
    {/if}
    <input
      class="path"
      spellcheck="false"
      aria-label="Folder"
      bind:value={pathText}
      onfocus={() => (editingPath = true)}
      onblur={() => (editingPath = false)}
      onkeydown={(ev) => {
        ev.stopPropagation()
        if (ev.key === 'Enter') typedPath()
        if (ev.key === 'Escape') {
          editingPath = false
          list?.focus()
        }
      }}
    />
  </div>
  <div class="cols">
    <span class="c-name">Name</span><span class="c-size">Size</span><span class="c-date"
      >Modified</span
    >
  </div>
  <div
    class="list"
    tabindex="0"
    role="listbox"
    aria-multiselectable="true"
    aria-label={side === 'local' ? 'This PC' : 'Server'}
    bind:this={list}
    onfocus={() => (app.activeSide = side)}
    onkeydown={key}
    onmousedown={(ev) => {
      const t = ev.target as HTMLElement
      if (t === list || t.classList.contains('rows')) app.select(side, [])
    }}
    oncontextmenu={(ev) => {
      ev.preventDefault()
      app.activeSide = side
      showMenu(side, ev.clientX, ev.clientY)
    }}
  >
    <div class="rows">
      {#if hasParent}
        <div
          class="row-e"
          class:focus={st.focus === 0}
          data-i="0"
          data-dir="false"
          role="option"
          aria-selected="false"
          tabindex="-1"
          onmousedown={(ev) => rowDown(ev, 0)}
          ondblclick={() => app.up(side)}
        >
          <span class="name"><Icon name="folder" /><span>..</span></span><span class="size"
          ></span><span class="date"></span>
        </div>
      {/if}
      {#each st.entries as e, k (e.name)}
        {@const i = k + (hasParent ? 1 : 0)}
        {@const sel = st.selected.has(e.name)}
        <!-- rows take the mouse, the list around them takes the keyboard (arrows, Enter) -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row-e"
          class:sel
          class:focus={i === st.focus}
          class:drop={dropping && drag.target?.row === e.name}
          data-i={i}
          data-name={e.name}
          data-dir={e.dir ? 'true' : 'false'}
          role="option"
          aria-selected={sel}
          tabindex="-1"
          onmousedown={(ev) => rowDown(ev, i)}
          onclick={(ev) => rowClick(ev, i)}
          ondblclick={(ev) =>
            (ev.target as HTMLElement).tagName !== 'INPUT' && void app.openEntry(side, e)}
        >
          <span class="name"
            ><Icon name={icon(e)} />
            {#if st.renaming === e.name}
              <input
                class="rename"
                spellcheck="false"
                aria-label="New name"
                use:renameStart={e}
                onkeydown={renameKey}
                onmousedown={(ev) => ev.stopPropagation()}
                onblur={(ev) => renameDone(ev, e.name)}
              />
            {:else}
              <span>{e.name}</span>
            {/if}
          </span><span class="size">{e.dir ? '' : formatSize(e.size)}</span><span class="date"
            >{formatStamp(e.modified)}</span
          >
        </div>
      {/each}
      {#if st.shown === st.path && !st.entries.length}
        <div class="empty-row">This folder is empty</div>
      {/if}
    </div>
  </div>
  {#if side === 'remote' && app.tab && app.tab.state !== 'connected'}
    {@const t = app.tab}
    <div class="offline">
      <span class="offline-text">
        {#if t.state === 'refused'}
          {t.refused?.message || 'The server refused the connection.'}<br /><br />
          <button
            class="pill small accent"
            onclick={() => (t.refused ? showRefused(t) : invoke('reconnect', { tab: t.id }))}
            >Try again</button
          >
        {:else}
          <span><StatusDot tab={t} />&nbsp; Connecting to {t.host}...</span>
          {#if t.protocol === 'ftp'}<br />For a 3DS: start Cement, then check the IP and port it
            shows.{/if}
        {/if}
      </span>
    </div>
  {/if}
</section>
