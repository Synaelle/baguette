<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Settings. The look and the colour show at once and are kept on Save. -->
<script lang="ts">
  import { untrack } from 'svelte'
  import { app } from '../lib/app.svelte'
  import { ACCENTS, applyLook, pickedAccent, shownAccent } from '../lib/colour'
  import { ask, cancelers, dialogs, tell } from '../lib/dialogs.svelte'
  import { invoke } from '../lib/tauri'
  import type { Look } from '../lib/types'

  type TabName = 'general' | '3ds' | 'lab' | 'about'
  const TABS: [TabName, string][] = [
    ['general', 'Preferences'],
    ['3ds', '3DS'],
    ['lab', 'Experimental'],
    ['about', 'About'],
  ]
  const LOOKS: [Look, string][] = [
    ['evil-twin', 'Evil Twin'],
    ['ficelle', 'Ficelle'],
    ['ficelle-light', 'Ficelle light'],
  ]

  let shown = $state<TabName>('general')
  let look = $state(app.look)
  let accent = $state(app.accent)
  const custom = app.localStart !== 'home' && app.localStart !== 'last'
  let localStart = $state(custom ? 'dir' : app.localStart)
  let localDir = $state(custom ? app.localStart : app.local.path)
  let sendToPc = $state(app.sendToPc)
  let bmpToPng = $state(app.bmpToPng)
  let checkUpdates = $state(app.checkUpdates)
  let protect3ds = $state(app.protect3ds)
  let bundles = $state(app.bundles)

  const preset = $derived(ACCENTS.find(([a]) => a === accent))

  // a colour picked on a 3DS while this is open becomes the one picked here
  $effect(() => {
    const fromCement = app.accent
    untrack(() => {
      accent = fromCement
      applyLook(look, fromCement)
    })
  })
  const cement = $derived(app.cement)

  function pickLook(l: Look) {
    look = l
    applyLook(look, accent)
  }
  function pickColour(hex: string) {
    accent = pickedAccent(hex)
    applyLook(look, accent)
  }

  function cancel() {
    applyLook(app.look, app.accent)
    dialogs.open = null
  }
  cancelers.prefs = cancel

  async function browse() {
    const d = await invoke<string | null>('browse_folder', { title: 'Folder the PC side opens in' })
    if (d) localDir = d
  }

  async function protectChanged() {
    if (protect3ds) return
    protect3ds = !(await ask(
      'Without it, Baguette lets you delete, rename or replace the 3DS files that keep it working: ' +
        '/Nintendo 3DS (installed games and saves), /boot.firm (what the console boots), ' +
        '/luma, /gm9 (NAND backups) and /boot.3dsx.' +
        '\n\nA mistake there can lose your games and saves, ' +
        'or stop the 3DS from booting into its custom firmware. Turn the protection off?',
      'Turn it off',
      'Keep it on',
      'Protect important 3DS files',
    ))
  }

  async function save() {
    const start = localStart === 'dir' ? localDir.trim() : localStart
    try {
      await invoke('set_local_start', { start })
      app.localStart = start
    } catch (e) {
      void tell(e)
      return
    }
    const setFlag = (cmd: string, on: boolean) => invoke(cmd, { on }).catch((e) => void tell(e))
    app.protect3ds = protect3ds
    void setFlag('set_protect_3ds', protect3ds)
    if (sendToPc !== app.sendToPc) {
      app.sendToPc = sendToPc
      void setFlag('set_send_to_pc', sendToPc)
    }
    if (bmpToPng !== app.bmpToPng) {
      app.bmpToPng = bmpToPng
      void setFlag('set_bmp_to_png', bmpToPng)
    }
    if (bundles !== app.bundles) {
      app.bundles = bundles
      void setFlag('set_bundles', bundles)
    }
    if (checkUpdates !== app.checkUpdates) {
      app.checkUpdates = checkUpdates
      void setFlag('set_check_updates', checkUpdates)
      if (checkUpdates) void app.lookForUpdate()
      else app.update = null
    }
    if (look !== app.look) {
      invoke('set_look', { look }).catch((e) => void tell(e))
      app.look = look
    }
    if (accent !== app.accent) {
      try {
        await invoke('set_accent', { accent })
        app.accent = accent
      } catch (e) {
        applyLook(app.look, app.accent)
        void tell(e)
      }
    }
    dialogs.open = null
  }
</script>

<div id="prefs" class="modal">
  <div class="dialog prefs" role="dialog" aria-modal="true" aria-labelledby="prefsTitle">
    <div class="prefs-head">
      <h2 id="prefsTitle">Settings</h2>
      <div class="seg" role="tablist" aria-label="Settings">
        {#each TABS as [id, label] (id)}
          <button role="tab" aria-selected={shown === id} onclick={() => (shown = id)}
            >{label}</button
          >
        {/each}
      </div>
    </div>

    {#if shown === 'general'}
      <section class="prefs-tab" role="tabpanel">
        <div class="setting">
          <div class="setting-text">
            <b id="lookLabel">Look</b><span
              >Evil Twin is round and soft. Ficelle is sharp and quiet, like the notes app, in a
              dark or a light room.</span
            >
          </div>
          <div class="modes" role="radiogroup" aria-labelledby="lookLabel">
            {#each LOOKS as [id, label] (id)}
              <button role="radio" aria-checked={look === id} onclick={() => pickLook(id)}
                >{label}</button
              >
            {/each}
          </div>
        </div>
        <div class="setting">
          <div class="setting-text">
            <b id="colourLabel">Colour</b><span
              >The whole window follows it, and so do Cement&#39;s screens. A colour picked on the
              3DS comes here too.</span
            >
          </div>
          <div class="colour-row">
            <div class="swatches" role="radiogroup" aria-labelledby="colourLabel">
              {#each ACCENTS as [hex, label] (hex)}
                <button
                  class="swatch"
                  style:background="#{shownAccent(hex, look)}"
                  title={label}
                  role="radio"
                  aria-label={label}
                  aria-checked={hex === accent}
                  onclick={() => pickColour(hex)}
                ></button>
              {/each}
            </div>
            <input
              type="color"
              title="Your own colour"
              aria-label="Your own colour"
              value="#{accent}"
              onchange={(ev) => pickColour((ev.target as HTMLInputElement).value)}
            />
            <span class="note">{preset ? preset[1] : `Your own, #${accent}`}</span>
          </div>
        </div>
        <div class="setting">
          <div class="setting-text">
            <label for="localStart"><b>PC side opens in</b></label><span
              >Where the left side starts when Baguette opens.</span
            >
          </div>
          <select id="localStart" bind:value={localStart}>
            <option value="home">Your user folder</option>
            <option value="last">The folder you left it in</option>
            <option value="dir">A folder you choose</option>
          </select>
          {#if localStart === 'dir'}
            <div class="pair">
              <input spellcheck="false" aria-label="Folder" bind:value={localDir} />
              <button class="pill small" onclick={browse}>Browse...</button>
            </div>
          {/if}
        </div>
        <div class="setting row-setting">
          <div class="setting-text">
            <label for="sendToPc"><b>Send to PC</b></label><span
              >A button on the Cement card that copies the 3DS folders you choose to a PC folder in
              one go (Ctrl+S).</span
            >
          </div>
          <input id="sendToPc" class="switch" type="checkbox" bind:checked={sendToPc} />
        </div>
        <div class="setting row-setting">
          <div class="setting-text">
            <label for="bmpToPng"><b>Save BMP pictures as PNG</b></label><span
              >Downloads turn .bmp files (like the 3DS screenshots) into .png.</span
            >
          </div>
          <input id="bmpToPng" class="switch" type="checkbox" bind:checked={bmpToPng} />
        </div>
        <div class="setting row-setting">
          <div class="setting-text">
            <label for="checkUpdates"><b>Look for updates</b></label><span
              >Asks GitHub once when Baguette starts. Updates only download when you click them.</span
            >
          </div>
          <input id="checkUpdates" class="switch" type="checkbox" bind:checked={checkUpdates} />
        </div>
      </section>
    {:else if shown === '3ds'}
      <section class="prefs-tab" role="tabpanel">
        <div class="setting row-setting">
          <div class="setting-text">
            <label for="protect3ds"><b>Protect important 3DS files</b></label><span
              >On Cement, /Nintendo 3DS, /boot.firm, /luma, /gm9 and /boot.3dsx are read-only: you
              can look and download, not change them. Luma&#39;s screenshots and crash dumps stay
              free.</span
            >
          </div>
          <input
            id="protect3ds"
            class="switch"
            type="checkbox"
            bind:checked={protect3ds}
            onchange={protectChanged}
          />
        </div>
      </section>
    {:else if shown === 'lab'}
      <section class="prefs-tab" role="tabpanel">
        <p class="lab-note">Still being tried out: these may change or go away.</p>
        <div class="setting row-setting">
          <div class="setting-text">
            <label for="bundles"><b>Send many files as one bundle</b></label><span
              >Baguette and Cement only (Cement 2.0-RC3 and later). Uploads of 8 files or more go as
              one stream that Cement unpacks as it arrives.</span
            >
          </div>
          <input id="bundles" class="switch" type="checkbox" bind:checked={bundles} />
        </div>
      </section>
    {:else}
      <section class="prefs-tab" role="tabpanel">
        <div class="setting">
          <div class="about-row"><span>Baguette</span><b>{app.version}</b></div>
          <div class="about-row"><span>Cement protocol</span><b>{app.protocol}</b></div>
          {#if cement}
            <div class="about-row">
              <span>Cement on this tab</span><b
                >{cement.version ? `Cement ${cement.version}` : 'Cement'} - protocol {cement.protocol}</b
              >
            </div>
            {#if cement.protocol < app.protocol}
              <p class="note">Update Cement to use everything this Baguette can do</p>
            {:else if cement.protocol > app.protocol}
              <p class="note">This Cement is newer: update Baguette for its new features</p>
            {:else}
              <p class="note">They speak the same protocol: everything works.</p>
            {/if}
          {/if}
        </div>
        <p class="note">
          Baguette and Cement each have their own version. The protocol level is what tells which
          ones work together: a Cement with a lower level still works, without the newer features.
        </p>
      </section>
    {/if}

    <div class="buttons">
      <div class="grow"></div>
      <button id="prefsCancel" class="pill" onclick={cancel}>Cancel</button>
      <button id="prefsOk" class="pill accent" onclick={save}>Save</button>
    </div>
  </div>
</div>
