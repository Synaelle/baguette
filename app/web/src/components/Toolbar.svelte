<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Upload and Download between the sides; the rest acts on the side last used. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { invoke } from '../lib/tauri'
  import Icon from './Icon.svelte'

  const side = $derived(app.activeSide)
  const ro = $derived(app.readOnly(app.remote.path))
  const lro = $derived(app.localReadOnly(app.local.path))
  const ready = $derived(side === 'local' ? app.local.path !== '' && !lro : app.connected && !ro)
  const locked = $derived(app.touchesLocked(side))
  const picked = $derived(app.pane(side).selected.size)
</script>

<div id="toolbar">
  <button
    id="btnUpload"
    class="pill accent"
    title="Send the selected PC items to the server"
    disabled={!app.connected || ro || !app.local.selected.size || app.local.path === ''}
    onclick={() => app.upload()}><Icon name="upload" />Upload</button
  >
  <button
    id="btnDownload"
    class="pill"
    title="Copy the selected server items to the PC folder"
    disabled={!app.connected || lro || !app.remote.selected.size}
    onclick={() => void app.download()}><Icon name="download" />Download</button
  >
  <button id="btnMkdir" class="pill" disabled={!ready} onclick={() => app.newFolder(side)}
    >New folder</button
  >
  <button
    id="btnRename"
    class="pill"
    title="Rename (F2)"
    disabled={!ready || picked !== 1 || locked}
    onclick={() => app.startRename(side)}>Rename</button
  >
  <button
    id="btnDelete"
    class="pill"
    title="Delete (Del)"
    disabled={!ready || !picked || locked}
    onclick={() => void app.deleteSelected(side)}>Delete</button
  >
  <button
    id="btnRefresh"
    class="pill icon"
    title="Refresh (F5)"
    aria-label="Refresh"
    onclick={() => app.refresh(side)}><Icon name="refresh" /></button
  >
  <div class="grow"></div>
  <button
    id="btnReconnect"
    class="pill ghost"
    title="Drop the connection and connect again"
    onclick={() => app.active !== null && invoke('reconnect', { tab: app.active })}
    >Reconnect</button
  >
</div>
