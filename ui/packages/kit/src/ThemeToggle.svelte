<script lang="ts">
  import { Moon, Sun, Monitor } from "@lucide/svelte";
  import SegmentedControl from "./SegmentedControl.svelte";
  import { theme, setTheme, type Theme } from "./theme.svelte";

  let { size = "sm", onchange }: { size?: "sm" | "md"; onchange?: (t: Theme) => void } = $props();
  let v = $state<Theme>(theme.value);
</script>

<SegmentedControl
  label="Theme"
  {size}
  bind:value={v}
  onchange={(t) => {
    setTheme(t);
    onchange?.(t);
  }}
  options={[
    { value: "dark", label: "Dark", icon: Moon },
    { value: "light", label: "Light", icon: Sun },
    { value: "system", label: "System", icon: Monitor },
  ]}
/>
