<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	let health = $state('loading...');
	let chainHealth = $state<Record<string, { status: string }>>({});

	onMount(async () => {
		try {
			health = await api.get('/health');
			const resp = await api.getJson('/health/chains');
			chainHealth = resp.chains || {};
		} catch {
			health = 'error';
		}
	});
</script>

<h1>Dashboard</h1>

<div class="cards">
	<div class="card">
		<h3>System Status</h3>
		<p class="status" class:ok={health === 'ok'}>{health}</p>
	</div>

	<div class="card">
		<h3>Chain Health</h3>
		{#each Object.entries(chainHealth) as [chain, info]}
			<div class="chain-row">
				<span class="chain-name">{chain}</span>
				<span class="chain-status">{info.status}</span>
			</div>
		{/each}
	</div>
</div>

<style>
	h1 {
		margin-top: 0;
		font-size: 1.5rem;
	}

	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
		gap: 1rem;
		margin-top: 1rem;
	}

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.card h3 {
		margin: 0 0 0.8rem;
		font-size: 0.95rem;
		color: var(--fg-muted, #8b949e);
	}

	.status {
		font-size: 1.5rem;
		font-weight: bold;
		color: var(--fg-muted, #8b949e);
	}

	.status.ok {
		color: var(--green, #3fb950);
	}

	.chain-row {
		display: flex;
		justify-content: space-between;
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--border, #30363d);
	}

	.chain-name {
		text-transform: capitalize;
	}

	.chain-status {
		color: var(--fg-muted, #8b949e);
	}
</style>
