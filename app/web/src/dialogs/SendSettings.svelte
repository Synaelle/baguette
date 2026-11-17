<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Send to PC: the 3DS folders and files it copies, and the PC folder they go to. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { ask, cancelers, dialogs, tell } from '../lib/dialogs.svelte'
  import { addSendItem, sendDraft } from '../lib/sendDraft.svelte'
  import { invoke } from '../lib/tauri'
  import type { SendSettings } from '../lib/types'

  let picked = $state<string[]>([])
  let newPath = $state('')
  const onCement = $derived(!!app.cement)

  const close = () => (dialogs.open = null)
  cancelers.send = close

  async function addPath() {
    const p = await invoke<string>('normalize', { path: newPath })
    if (newPath.trim() && p.length > 1) {
      addSendItem(p)
      newPath = ''
    }
  }

  async function browse() {
    const d = await invoke<string | null>('browse_folder', { title: 'Send to PC folder' })
    if (d) sendDraft.dir = d
  }

  async function replaceChanged() {
    if (!sendDraft.replace) return
    sendDraft.replace = await ask(
      `Each Send to PC will first empty the PC folder (its current files and folders go to ${app.binName}), ` +
        'then copy the 3DS items into it.\n\nOnly use a folder dedicated to this. Turn it on?',
      'Yes',
      'No',
      'Replace PC folder contents',
    )
  }

  async function save() {
    const s: SendSettings = {
      send_dir: sendDraft.dir.trim(),
      send_items: [...sendDraft.items],
      send_replace: sendDraft.replace,
    }
    try {
      await invoke('save_settings', { s })
    } catch (e) {
      void tell(e)
      return
    }
    app.send = s
    close()
  }
</script>

<div id="settings" class="modal">
  <div
    class="dialog"
    style:width="480px"
    role="dialog"
    aria-modal="true"
    aria-labelledby="settingsTitle"
  >
    <h2 id="settingsTitle">Send to PC</h2>
    <p class="note">Copies these 3DS folders and files into one PC folder, in one click.</p>
    <select id="sendItems" multiple size="6" aria-label="3DS folders and files" bind:value={picked}>
      {#each sendDraft.items as p (p)}<option value={p}>{p}</option>{/each}
    </select>
    <div class="row">
      <button
        id="btnAddPick"
        class="pill small"
        disabled={!onCement}
        onclick={() => (dialogs.open = 'picker')}>Add from 3DS...</button
      >
      <button
        id="btnRemoveItem"
        class="pill small"
        onclick={() => (sendDraft.items = sendDraft.items.filter((p) => !picked.includes(p)))}
        >Remove</button
      >
    </div>
    <div class="row">
      <input
        id="newPath"
        spellcheck="false"
        aria-label="3DS path"
        placeholder="/path/to/folder or file"
        bind:value={newPath}
        onkeydown={(ev) => ev.key === 'Enter' && addPath()}
      />
      <button id="btnAddPath" class="pill small" onclick={addPath}>Add</button>
    </div>
    <p id="sendNote" class="note">{onCement ? '' : 'Connect to Cement to pick from the 3DS.'}</p>
    <div class="row">
      <label for="sendDir">PC folder</label><input
        id="sendDir"
        spellcheck="false"
        bind:value={sendDraft.dir}
      />
      <button id="btnBrowse" class="pill small" onclick={browse}>Browse...</button>
    </div>
    <label class="check"
      ><input
        id="replace"
        type="checkbox"
        bind:checked={sendDraft.replace}
        onchange={replaceChanged}
      />
      Replace the PC folder contents (old files go to
      <span class="bin-name">{app.binName}</span>)</label
    >
    <div class="buttons">
      <div class="grow"></div>
      <button id="settingsCancel" class="pill" onclick={close}>Cancel</button>
      <button id="settingsOk" class="pill accent" onclick={save}>Save</button>
    </div>
  </div>
</div>
