<script lang="ts">
  import type { ValidationItem } from '../models';

  let { items }: { items: ValidationItem[] } = $props();
</script>

{#if items.length > 0}
  <ul class="validation-list">
    {#each items as item (item.code + item.message)}
      <li class="validation-{item.severity}">
        <svg class="validation-icon" viewBox="0 0 16 16" aria-hidden="true">
          {#if item.severity === 'error'}
            <circle cx="8" cy="8" r="6" />
            <path d="m6 6 4 4m0-4-4 4" />
          {:else}
            <path d="M8 2.1 14 13H2L8 2.1Z" />
            <path d="M8 5.5v3.7m0 1.8v.1" />
          {/if}
        </svg>
        {item.message}
      </li>
    {/each}
  </ul>
{/if}
