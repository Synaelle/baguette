<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- A saved site: where, how, and who logs in. -->
<script lang="ts">
  import { app } from '../lib/app.svelte'
  import { ask, cancelers, dialogs, tell } from '../lib/dialogs.svelte'
  import { siteEditor } from '../lib/siteEditor.svelte'
  import { invoke } from '../lib/tauri'
  import type { Protocol, Site, SiteIn } from '../lib/types'

  const editing = siteEditor.site
  let name = $state(editing?.name ?? '')
  let protocol = $state<Protocol>(editing?.protocol ?? 'ftp')
  let host = $state(editing?.host ?? '')
  let port = $state(editing ? String(editing.port) : '')
  let user = $state(editing?.user ?? '')
  let pass = $state('')
  let key = $state(editing?.key ?? '')
  let passphrase = $state('')
  let startDir = $state(editing?.start_dir ?? '')
  let passDirty = false
  let phraseDirty = false

  const defaultPort = $derived({ ftp: 21, sftp: 22, ftps: 21, 'ftps-implicit': 990 }[protocol])
  const close = () => (dialogs.open = null)
  cancelers.site = close

  async function browseKey() {
    const f = await invoke<string | null>('browse_file', { title: 'Private key file' })
    if (f) key = f
  }

  async function save(connect: boolean) {
    const s: SiteIn = {
      id: editing?.id ?? 0,
      name,
      protocol,
      host,
      port: parseInt(port, 10) || 0,
      user,
      pass: passDirty || !editing ? pass : null,
      key: protocol === 'sftp' ? key : '',
      passphrase: phraseDirty || !editing ? passphrase : null,
      start_dir: startDir,
    }
    // Cement listens on 5000: a plain FTP site without a port is most likely the 3DS
    if (!s.port && s.protocol === 'ftp' && !s.user.trim()) s.port = 5000
    let list: Site[]
    try {
      list = await invoke<Site[]>('site_save', { s })
    } catch (e) {
      void tell(e)
      return
    }
    const before = new Set(app.sites.map((x) => x.id))
    app.sites = list
    close()
    const id = editing ? editing.id : list.find((x) => !before.has(x.id))?.id
    if (connect && id) {
      const open = app.tabOfSite(id)
      if (open) app.closeTab(open.id)
      void app.openSite(id)
    }
  }

  async function remove() {
    if (!editing || !(await ask(`Delete the site "${editing.name}"?`))) return
    try {
      app.sites = await invoke<Site[]>('site_delete', { id: editing.id })
      close()
    } catch (e) {
      void tell(e)
    }
  }

  const enter = (ev: KeyboardEvent) => ev.key === 'Enter' && void save(true)
</script>

<div id="siteDlg" class="modal">
  <div
    class="dialog"
    style:width="460px"
    role="dialog"
    aria-modal="true"
    aria-labelledby="siteDlgTitle"
  >
    <h2 id="siteDlgTitle">{editing ? 'Edit site' : 'New site'}</h2>
    <div class="form">
      <label for="sName">Name</label>
      <!-- svelte-ignore a11y_autofocus -->
      <input id="sName" placeholder="My 3DS" bind:value={name} onkeydown={enter} autofocus />
      <label for="sProto">Protocol</label>
      <select id="sProto" bind:value={protocol}>
        <option value="ftp">FTP (Cement, plain FTP servers)</option>
        <option value="sftp">SFTP (SSH)</option>
        <option value="ftps">FTPS (explicit TLS)</option>
        <option value="ftps-implicit">FTPS (implicit TLS, port 990)</option>
      </select>
      <label for="sHost">Address</label>
      <div class="pair">
        <input
          id="sHost"
          spellcheck="false"
          placeholder="192.168.1.10"
          bind:value={host}
          onkeydown={enter}
        />
        <input
          id="sPort"
          type="number"
          min="1"
          max="65535"
          aria-label="Port"
          placeholder={String(defaultPort)}
          bind:value={port}
          onkeydown={enter}
        />
      </div>
      <label for="sUser">User</label>
      <input
        id="sUser"
        spellcheck="false"
        placeholder={protocol === 'ftp' ? 'empty: anonymous (Cement)' : ''}
        bind:value={user}
        onkeydown={enter}
      />
      <label for="sPass">Password</label>
      <input
        id="sPass"
        type="password"
        placeholder={editing?.has_pass
          ? 'saved (type to change)'
          : editing?.needs_password
            ? 'type it again on this PC'
            : ''}
        bind:value={pass}
        oninput={() => (passDirty = true)}
        onkeydown={enter}
      />
      {#if protocol === 'sftp'}
        <label for="sKey">Key file</label>
        <div class="pair">
          <input
            id="sKey"
            spellcheck="false"
            placeholder="optional: id_ed25519"
            bind:value={key}
            onkeydown={enter}
          />
          <button class="pill small" onclick={browseKey}>Browse...</button>
        </div>
        <label for="sPassphrase">Passphrase</label>
        <input
          id="sPassphrase"
          type="password"
          placeholder={editing?.has_passphrase ? 'saved (type to change)' : ''}
          bind:value={passphrase}
          oninput={() => (phraseDirty = true)}
          onkeydown={enter}
        />
      {/if}
      <label for="sStart">Start folder</label>
      <input
        id="sStart"
        spellcheck="false"
        placeholder="empty: the server's home"
        bind:value={startDir}
        onkeydown={enter}
      />
    </div>
    <p id="passNote" class="note">
      {app.linux
        ? "Passwords are saved encrypted, with a key kept in your desktop's keyring, or next to your settings (~/.config/baguette) when there is none."
        : 'Passwords are saved encrypted for your Windows account.'}
    </p>
    <div class="buttons">
      {#if editing}<button id="siteDelete" class="pill ghost danger" onclick={remove}>Delete</button
        >{/if}
      <div class="grow"></div>
      <button id="siteCancel" class="pill" onclick={close}>Cancel</button>
      <button id="siteSave" class="pill accent" onclick={() => save(false)}>Save</button>
      <button id="siteConnect" class="pill accent" onclick={() => save(true)}
        >Save and connect</button
      >
    </div>
  </div>
</div>
