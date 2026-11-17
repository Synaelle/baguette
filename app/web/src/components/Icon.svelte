<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- One of the window's drawn icons (lib/icons.ts), as it is written there. -->
<script lang="ts">
  import { ICONS, type IconName } from '../lib/icons'

  let { name }: { name: IconName } = $props()
</script>

<!-- the markup comes from lib/icons.ts, never from user data -->
<!-- eslint-disable-next-line svelte/no-at-html-tags -->
{@html ICONS[name]}
