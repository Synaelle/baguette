<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Picks 3DS folders and files for Send to PC. Nothing picked: the folder shown. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { cancelers, dialogs } from '../lib/dialogs.svelte'
  import { picker, pickNavigate } from '../lib/picker.svelte'
  import { addSendItem } from '../lib/sendDraft.svelte'
  import Icon from '../components/Icon.svelte'

  pickNavigate('/')
  const back = () => (dialogs.open = 'send')
  cancelers.picker = back

  const up = () => picker.cwd !== '/' && pickNavigate(app.parent('remote', picker.cwd))

  function down(ev: MouseEvent, i: number) {
    if (ev.ctrlKey)
      picker.sel = picker.sel.includes(i) ? picker.sel.filter((x) => x !== i) : [...picker.sel, i]
    else picker.sel = [i]
  }

  function add() {
    const out = picker.sel.map((i) => app.join('remote', picker.cwd, picker.entries[i]?.name ?? ''))
    if (!out.length) out.push(picker.cwd)
    out.forEach(addSendItem)
    back()
  }
</script>

<div id="picker" class="modal">
  <div
    class="dialog"
    style:width="440px"
    role="dialog"
    aria-modal="true"
    aria-labelledby="pickerTitle"
  >
    <h2 id="pickerTitle">Add from 3DS</h2>
    <div class="row">
      <button id="pickUp" class="pill small" onclick={up}>Up</button><input
        id="pickPath"
        readonly
        aria-label="Folder"
        value={picker.cwd}
      />
    </div>
    <div id="pickWrap" role="listbox" aria-multiselectable="true" aria-label="3DS folder">
      <div id="pickRows">
        {#if picker.cwd !== '/'}
          <div class="row-e" role="option" aria-selected="false" tabindex="-1" ondblclick={up}>
            <span class="name"><Icon name="folder" class="ico dir" /><span>..</span></span>
          </div>
        {/if}
        {#each picker.entries as e, i (e.name)}
          <div
            class="row-e"
            class:sel={picker.sel.includes(i)}
            role="option"
            aria-selected={picker.sel.includes(i)}
            tabindex="-1"
            onmousedown={(ev) => down(ev, i)}
            ondblclick={() => e.dir && pickNavigate(app.join('remote', picker.cwd, e.name))}
          >
            <span class="name"
              ><Icon name={e.dir ? 'folder' : 'file'} class="ico {e.dir ? 'dir' : 'file'}" /><span
                >{e.name}</span
              ></span
            >
          </div>
        {/each}
      </div>
    </div>
    <div class="buttons">
      <div class="grow"></div>
      <button id="pickCancel" class="pill" onclick={back}>Cancel</button>
      <button id="pickOk" class="pill accent" onclick={add}>Add</button>
    </div>
  </div>
</div>
