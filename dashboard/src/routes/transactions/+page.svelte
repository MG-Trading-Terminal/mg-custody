<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Tx {
		id: string;
		type: string;
		asset: string;
		chain: string;
		amount: string;
		status: string;
		created_at: string;
	}

	const CHAINS  = ['all', 'ethereum', 'bitcoin', 'tron', 'solana'];
	const STATUSES = ['all', 'confirmed', 'broadcast', 'pending', 'failed'];

	let txs        = $state<Tx[]>([]);
	let loading    = $state(true);
	let loadingMore = $state(false);
	let error      = $state('');
	let cursor     = $state<string | null>(null);
	let hasMore    = $state(false);

	let filterChain  = $state('all');
	let filterStatus = $state('all');
	let filterFrom   = $state('');
	let filterTo     = $state('');

	function buildQuery(nextCursor?: string): string {
		const p = new URLSearchParams({ limit: '25' });
		if (filterChain  !== 'all') p.set('chain',  filterChain);
		if (filterStatus !== 'all') p.set('status', filterStatus);
		if (filterFrom)             p.set('from',   filterFrom);
		if (filterTo)               p.set('to',     filterTo);
		if (nextCursor)             p.set('cursor', nextCursor);
		return `/v1/transactions?${p}`;
	}

	async function load() {
		loading = true;
		error   = '';
		txs     = [];
		cursor  = null;
		try {
			const data = await api.getJson<{ transactions: Tx[]; next_cursor: string | null }>(buildQuery());
			txs     = data.transactions ?? [];
			cursor  = data.next_cursor ?? null;
			hasMore = !!cursor;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load transactions';
		} finally {
			loading = false;
		}
	}

	async function loadMore() {
		if (!cursor || loadingMore) return;
		loadingMore = true;
		try {
			const data = await api.getJson<{ transactions: Tx[]; next_cursor: string | null }>(buildQuery(cursor));
			txs    = [...txs, ...(data.transactions ?? [])];
			cursor = data.next_cursor ?? null;
			hasMore = !!cursor;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load more';
		} finally {
			loadingMore = false;
		}
	}

	onMount(load);

	const STATUS_COLOR: Record<string, string> = {
		confirmed: 'green',
		broadcast: 'yellow',
		pending:   'yellow',
		failed:    'red',
	};
	function statusClass(s: string) { return STATUS_COLOR[s] ?? 'muted'; }
	function trunc(s: string, n = 12) { return s.length > n ? s.slice(0, n) + '…' : s; }
	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Transactions</h1>

<div class="filters card">
	<label>Chain
		<select bind:value={filterChain}>
			{#each CHAINS as c}<option value={c}>{c}</option>{/each}
		</select>
	</label>
	<label>Status
		<select bind:value={filterStatus}>
			{#each STATUSES as s}<option value={s}>{s}</option>{/each}
		</select>
	</label>
	<label>From <input type="date" bind:value={filterFrom} /></label>
	<label>To   <input type="date" bind:value={filterTo}   /></label>
	<button class="btn" onclick={load}>Apply</button>
</div>

<div class="card">
	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if txs.length === 0}
		<p class="muted">No transactions match the selected filters.</p>
	{:else}
		<table>
			<thead>
				<tr><th>ID</th><th>Type</th><th>Chain</th><th>Asset</th><th>Amount</th><th>Status</th><th>Date</th></tr>
			</thead>
			<tbody>
				{#each txs as tx}
					<tr>
						<td class="mono muted">{trunc(tx.id)}</td>
						<td>{tx.type}</td>
						<td class="muted">{tx.chain}</td>
						<td>{tx.asset}</td>
						<td class="mono">{tx.amount}</td>
						<td><span class="badge {statusClass(tx.status)}">{tx.status}</span></td>
						<td class="muted">{fmt(tx.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		{#if hasMore}
			<div class="load-more">
				<button class="btn-ghost" onclick={loadMore} disabled={loadingMore}>
					{loadingMore ? 'Loading…' : 'Load more'}
				</button>
			</div>
		{/if}
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

	.filters {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: flex-end;
		margin-bottom: 1rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.78rem;
		color: var(--fg-muted, #8b949e);
	}

	select, input[type="date"] {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg, #e1e4e8);
		padding: 0.35rem 0.6rem;
		font-size: 0.85rem;
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
		align-self: flex-end;
	}

	.btn-ghost {
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg-muted, #8b949e);
		padding: 0.4rem 1.2rem;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.badge { border-radius: 4px; padding: 0.15rem 0.5rem; font-size: 0.78rem; text-transform: capitalize; font-weight: 600; }
	.green  { background: rgba(63,185,80,.12);  color: var(--green, #3fb950); }
	.yellow { background: rgba(210,153,34,.12); color: var(--yellow, #d29922); }
	.red    { background: rgba(248,81,73,.12);  color: var(--red, #f85149); }
	.muted  { color: var(--fg-muted, #8b949e); }
	.mono   { font-family: monospace; }
	.err    { color: var(--red, #f85149); }

	.load-more { display: flex; justify-content: center; padding-top: 1rem; }
</style>
