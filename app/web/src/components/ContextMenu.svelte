<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The right-click menu. From the keyboard: arrows, Enter, Escape (back to the list). -->
<script lang="ts">
  import { tick } from 'svelte'
  import { focusActive } from '../lib/focus'
  import { hideMenu, menu, type MenuItem } from '../lib/menu.svelte'

  let box = $state<HTMLElement | null>(null)
  let left = $state(0)
  let top = $state(0)

  // placed inside the window, the first item focused when opened from the keyboard
  $effect(() => {
    if (!menu.side || !box) return
    const { x, y, fromKeyboard } = menu
    void tick().then(() => {
      if (!box) return
      left = Math.min(x, innerWidth - box.offsetWidth - 4)
      top = Math.min(y, innerHeight - box.offsetHeight - 4)
      if (fromKeyboard) box.querySelector<HTMLElement>('[role=menuitem]:not(.off)')?.focus()
    })
  })

  function close() {
    const hadFocus = !!box?.contains(document.activeElement)
    hideMenu()
    if (hadFocus) focusActive()
  }

  function run(it: MenuItem) {
    if (it.off) return
    close()
    it.run()
  }

  function key(ev: KeyboardEvent) {
    const items = [...(box?.querySelectorAll<HTMLElement>('[role=menuitem]:not(.off)') ?? [])]
    const i = items.indexOf(document.activeElement as HTMLElement)
    if (ev.key === 'ArrowDown') items[(i + 1) % items.length]?.focus()
    else if (ev.key === 'ArrowUp') items[(i - 1 + items.length) % items.length]?.focus()
    else if (ev.key === 'Home') items[0]?.focus()
    else if (ev.key === 'End') items[items.length - 1]?.focus()
    else if (ev.key === 'Enter' || ev.key === ' ')
      (document.activeElement as HTMLElement | null)?.click()
    else if (ev.key === 'Escape' || ev.key === 'Tab') close()
    else return
    ev.preventDefault()
    ev.stopPropagation()
  }
</script>

<svelte:document onmousedown={(ev) => menu.side && !box?.contains(ev.target as Node) && close()} />

{#if menu.side}
  <div
    id="menu"
    class="menu"
    role="menu"
    tabindex="-1"
    bind:this={box}
    style:left="{left}px"
    style:top="{top}px"
    onkeydown={key}
  >
    {#each menu.items as it, i (i)}
      {#if it}
        <div
          role="menuitem"
          tabindex="-1"
          class:off={it.off}
          aria-disabled={it.off ? 'true' : undefined}
          onclick={() => run(it)}
          onkeydown={() => {}}
        >
          <span>{it.label}</span><span class="key">{it.key}</span>
        </div>
      {:else}
        <hr />
      {/if}
    {/each}
  </div>
{/if}
