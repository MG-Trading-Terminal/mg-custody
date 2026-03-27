<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Tx {
		id: string;
		type: string;
		asset: string;
		amount: string;
		status: string;
		created_at: string;
	}

	let txs = $state<Tx[]>([]);
	let loading = $state(true);
	let error = $state('');

	onMount(async () => {
		try {
			const data = await api.getJson<{ transactions: Tx[]; next_cursor: string | null }>('/v1/transactions');
			txs = data.transactions ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load transactions';
		} finally {
			loading = false;
		}
	});

	const STATUS_CLASS: Record<string, string> = {
		confirmed: 'green',
		broadcast: 'yellow',
		pending:   'yellow',
		failed:    'red',
	};

	function statusClass(s: string) { return STATUS_CLASS[s] ?? 'muted'; }
	function trunc(s: string, n = 10) { return s.length > n ? s.slice(0, n) + '…' : s; }
	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Transactions</h1>

<div class="card">
	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if txs.length === 0}
		<p class="muted">No transactions found.</p>
	{:else}
		<table>
			<thead>
				<tr><th>ID</th><th>Type</th><th>Asset</th><th>Amount</th><th>Status</th><th>Date</th></tr>
			</thead>
			<tbody>
				{#each txs as tx}
					<tr>
						<td class="mono muted">{trunc(tx.id, 12)}</td>
						<td>{tx.type}</td>
						<td>{tx.asset}</td>
						<td class="mono">{tx.amount}</td>
						<td class={statusClass(tx.status)}>{tx.status}</td>
						<td class="muted">{fmt(tx.created_at)}</td>
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

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.mono  { font-family: monospace; }
	.muted { color: var(--fg-muted, #8b949e); }
	.green  { color: var(--green, #3fb950); text-transform: capitalize; }
	.yellow { color: var(--yellow, #d29922); text-transform: capitalize; }
	.red    { color: var(--red, #f85149);   text-transform: capitalize; }
	.err   { color: var(--red, #f85149); }
</style>
