<script lang="ts">
  import type { ValidationItem } from "../models";

  let { items }: { items: ValidationItem[] } = $props();
</script>

{#if items.length > 0}
  <ul class="validation-list">
    {#each items as item (item.code + item.message)}
      <li class="validation-{item.severity}">
        {#if item.severity === "error"}✕{:else}⚠{/if}
        {item.message}
      </li>
    {/each}
  </ul>
{/if}
