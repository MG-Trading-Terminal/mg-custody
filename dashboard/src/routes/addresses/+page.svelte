<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Address {
		address: string;
		chain: string;
		user_id: string;
		created_at: string;
	}

	let addresses = $state<Address[]>([]);
	let loading = $state(true);
	let error = $state('');
	let chain = $state('ethereum');

	const CHAINS = ['ethereum', 'bitcoin', 'solana', 'tron'];

	async function load() {
		loading = true;
		error = '';
		try {
			const data = await api.getJson<{ addresses: Address[] }>(`/v1/addresses/chain/${chain}`);
			addresses = data.addresses ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load addresses';
		} finally {
			loading = false;
		}
	}

	onMount(load);

	function trunc(s: string, n = 16) { return s.length > n ? s.slice(0, 8) + '…' + s.slice(-6) : s; }
	function fmt(iso: string) { return new Date(iso).toLocaleDateString(); }
</script>

<h1>Addresses</h1>

<div class="card">
	<div class="toolbar">
		<div class="tabs">
			{#each CHAINS as c}
				<button class="tab" class:active={chain === c} onclick={() => { chain = c; load(); }}>
					{c}
				</button>
			{/each}
		</div>
		<span class="muted">{addresses.length} address{addresses.length !== 1 ? 'es' : ''}</span>
	</div>

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if addresses.length === 0}
		<p class="muted">No addresses on {chain}.</p>
	{:else}
		<table>
			<thead>
				<tr><th>Address</th><th>Chain</th><th>User ID</th><th>Created</th></tr>
			</thead>
			<tbody>
				{#each addresses as a}
					<tr>
						<td class="mono" title={a.address}>{trunc(a.address)}</td>
						<td>{a.chain}</td>
						<td class="muted">{a.user_id}</td>
						<td class="muted">{fmt(a.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1rem;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.tabs { display: flex; gap: 0.4rem; }

	.tab {
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg-muted, #8b949e);
		padding: 0.3rem 0.75rem;
		font-size: 0.8rem;
		cursor: pointer;
		text-transform: capitalize;
	}

	.tab.active {
		border-color: var(--accent, #58a6ff);
		color: var(--accent, #58a6ff);
	}

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.mono  { font-family: monospace; font-size: 0.85rem; }
	.muted { color: var(--fg-muted, #8b949e); }
	.err   { color: var(--red, #f85149); }
</style>
