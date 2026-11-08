<!-- 12-key phone keypad. Calls `onkey` for every press. -->
<script lang="ts">
  let { onkey }: { onkey: (key: string) => void } = $props();
  const keys = [
    ["1", ""], ["2", "ABC"], ["3", "DEF"],
    ["4", "GHI"], ["5", "JKL"], ["6", "MNO"],
    ["7", "PQRS"], ["8", "TUV"], ["9", "WXYZ"],
    ["*", ""], ["0", "+"], ["#", ""],
  ];
</script>

<div class="pad">
  {#each keys as [k, letters] (k)}
    <button onclick={() => onkey(k)}>
      <span class="k">{k}</span><span class="l">{letters}</span>
    </button>
  {/each}
</div>

<style>
  .pad { display: grid; grid-template-columns: repeat(3, 72px); gap: 12px; justify-content: center; }
  button { height: 64px; border-radius: 50%; display: flex; flex-direction: column; align-items: center; justify-content: center; background: var(--panel-2); border: none; }
  .k { font-size: 1.5rem; font-weight: 500; }
  .l { font-size: 0.6rem; color: var(--muted); letter-spacing: 0.1em; min-height: 0.8em; }
</style>
