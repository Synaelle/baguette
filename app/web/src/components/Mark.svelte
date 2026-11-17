<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
<!-- Baguette's loaf (meta/icon.svg) in the window's accent. -->
<script lang="ts">
  let { size = 22 }: { size?: number } = $props()
</script>

<svg class="logo" width={size} height={size} viewBox="0 0 512 512" aria-hidden="true">
  <g
    fill="none"
    stroke="var(--accent)"
    stroke-width="52"
    stroke-linecap="round"
    stroke-linejoin="round"
    transform="rotate(-35 256 256)"
  >
    <path d="M50 256C118 146 394 146 462 256C394 366 118 366 50 256Z" />
    <path d="M178 242L194 268M248 242L264 268M318 242L334 268" />
  </g>
</svg>
