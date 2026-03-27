<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Vault {
		id: string;
		name: string;
		account_count: number;
		auto_fuel: boolean;
		created_at: string;
	}

	let vaults = $state<Vault[]>([]);
	let loading = $state(true);
	let error = $state('');

	onMount(async () => {
		try {
			const data = await api.getJson<{ vaults: Vault[] }>('/v1/vaults');
			vaults = data.vaults ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load vaults';
		} finally {
			loading = false;
		}
	});

	function fmt(iso: string) {
		return new Date(iso).toLocaleDateString();
	}
</script>

<h1>Wallets</h1>

<div class="card">
	<div class="toolbar">
		<span>{vaults.length} vault{vaults.length !== 1 ? 's' : ''}</span>
		<button class="btn">+ Create Vault</button>
	</div>

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if vaults.length === 0}
		<p class="muted">No vaults found.</p>
	{:else}
		<table>
			<thead>
				<tr><th>Name</th><th>Accounts</th><th>Auto-fuel</th><th>Created</th></tr>
			</thead>
			<tbody>
				{#each vaults as v}
					<tr>
						<td>{v.name}</td>
						<td>{v.account_count}</td>
						<td class={v.auto_fuel ? 'yes' : 'no'}>{v.auto_fuel ? 'On' : 'Off'}</td>
						<td class="muted">{fmt(v.created_at)}</td>
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
	}

	.btn {
		background: var(--accent, #58a6ff);
		color: #0f1117;
		border: none;
		border-radius: 6px;
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.muted { color: var(--fg-muted, #8b949e); }
	.yes { color: var(--green, #3fb950); }
	.no  { color: var(--fg-muted, #8b949e); }
	.err { color: var(--red, #f85149); }
</style>
