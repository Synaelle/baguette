<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- One Lucide icon. Decorative unless `label` is given, for an icon that is
     the only content of a control. -->
<script lang="ts">
  import { ICONS, type IconName } from '../lib/icons'

  let {
    name,
    class: cls = '',
    label,
  }: { name: IconName; class?: string; label?: string } = $props()
</script>

<svg
  class="icon {cls}"
  viewBox="0 0 24 24"
  role={label ? 'img' : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : 'true'}
>
  <!-- the markup comes from icon files bundled at build time, never from user data -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html ICONS[name]}
</svg>
