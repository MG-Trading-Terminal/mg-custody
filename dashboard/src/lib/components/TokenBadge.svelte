<script lang="ts">
	import { tokenIcon, chainIcon } from '$lib/icons/tokens';

	interface Props {
		symbol?: string;
		chain?: string;
		size?: number;
		showLabel?: boolean;
	}

	let { symbol = '', chain = '', size = 20, showLabel = true }: Props = $props();

	function getIcon(): string {
		if (symbol) return tokenIcon(symbol, size);
		if (chain) return chainIcon(chain, size);
		return '';
	}

	function getLabel(): string {
		return symbol || chain || '';
	}
</script>

<span class="token-badge">
	{@html getIcon()}
	{#if showLabel}
		<span class="label">{getLabel()}</span>
	{/if}
</span>

<style>
	.token-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		white-space: nowrap;
	}

	.token-badge :global(svg) {
		flex-shrink: 0;
		border-radius: 50%;
	}

	.label {
		font-weight: 500;
		font-size: 0.88rem;
	}
</style>
