<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- The yes/no box and messages, over any dialog. -->
<script lang="ts">
  import { answer, dialogs } from '../lib/dialogs.svelte'
  import { focusActive } from '../lib/focus'

  let yesButton = $state<HTMLButtonElement | null>(null)
  $effect(() => {
    if (dialogs.question) yesButton?.focus()
  })

  function done(ok: boolean) {
    const over = dialogs.open !== null
    answer(ok)
    if (!over) focusActive()
  }
</script>

{#if dialogs.question}
  {@const q = dialogs.question}
  <div id="ask" class="modal top">
    <div
      class="dialog"
      style:width="420px"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="askTitle"
      aria-describedby="askText"
    >
      <h2 id="askTitle">{q.title}</h2>
      <p id="askText">{q.text}</p>
      <div class="buttons">
        <div class="grow"></div>
        {#if q.no}<button id="askNo" class="pill" onclick={() => done(false)}>{q.no}</button>{/if}
        <button id="askYes" class="pill accent" bind:this={yesButton} onclick={() => done(true)}
          >{q.yes}</button
        >
      </div>
    </div>
  </div>
{/if}
