<script lang="ts">
  import Header from "./lib/Header.svelte";
  import { initRouter, router } from "./lib/router.svelte";
  import { routes } from "./lib/routes";
  import Create from "./pages/Create.svelte";
  import Credits from "./pages/Credits.svelte";
  import Dictionary from "./pages/Dictionary.svelte";
  import History from "./pages/History.svelte";
  import Batch from "./pages/Batch.svelte";
  import References from "./pages/References.svelte";

  $effect(() => initRouter());

  let title = $derived(routes[router.index].title);
</script>

<svelte:head>
  <title>{title} - irodori-server</title>
</svelte:head>

<Header />
<h1 class="sub-header">{title}</h1>

<main>
  {#if router.index === 1}
    <Dictionary />
  {:else if router.index === 2}
    <History />
  {:else if router.index === 3}
    <Credits />
  {:else if router.index === 4}
    <References />
  {:else if router.index === 5}
    <Batch />
  {:else}
    <Create />
  {/if}
</main>

<style lang="sass">
  .sub-header
    position: sticky
    top: var(--header-h)
    z-index: 9
    height: var(--subheader-h)
    margin: 0
    padding: 0 var(--sp-3)
    overflow: hidden
    background: var(--c-wash-raised)
    border-bottom: 1px solid var(--c-border)
    font-size: var(--fs-md)
    font-weight: 500
    line-height: var(--subheader-h)
    text-overflow: ellipsis
    white-space: nowrap
</style>
