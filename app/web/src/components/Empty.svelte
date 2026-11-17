<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- No tab open: the way to a site. -->
<script lang="ts">
  import { editSite } from '../lib/siteEditor.svelte'
  import Icon from './Icon.svelte'
</script>

<div id="empty">
  <div class="empty-card">
    <Icon name="biteBig" />
    <h1>Pick a site to start</h1>
    <p>
      Open one from the list, add a new one, or type an address in Quick connect.<br />Connected to
      Cement, the 3DS tools show up on the right.
    </p>
    <button id="btnEmptyNew" class="pill accent" onclick={() => editSite(null)}>+ New site</button>
  </div>
</div>
