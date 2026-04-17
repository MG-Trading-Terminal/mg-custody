<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { tokenIcon, chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';
	import { t, locale } from '$lib/i18n';

	interface Tx {
		id: string;
		type: string;
		asset: string;
		chain: string;
		amount: string;
		status: string;
		created_at: string;
		destination?: string;
		tx_hash?: string;
		usd_value?: string;
		fee?: string;
	}

	// Server can emit either `type` or `tx_type`, `destination` or `dest_address`.
	// Normalize both shapes to the single frontend model.
	function normalize(raw: any): Tx {
		return {
			id: raw.id,
			type: raw.type ?? raw.tx_type ?? 'unknown',
			asset: raw.asset ?? '',
			chain: raw.chain ?? '',
			amount: raw.amount ?? '0',
			status: raw.status ?? 'unknown',
			created_at: raw.created_at,
			destination: raw.destination ?? raw.dest_address ?? raw.source_address,
			tx_hash: raw.tx_hash ?? undefined,
			usd_value: raw.usd_value ?? undefined,
			fee: raw.fee ?? undefined,
		};
	}

	const CHAINS   = ['all','ethereum','bitcoin','tron','solana'];
	const STATUSES = ['all','confirmed','broadcast','pending','signed','validated','failed'];
	const TYPES    = ['all','deposit','withdrawal','internal','sweep'];

	let txs         = $state<Tx[]>([]);
	let loading     = $state(true);
	let loadingMore = $state(false);
	let error       = $state('');
	let cursor      = $state<string | null>(null);
	let hasMore     = $state(false);

	let filterChain  = $state('all');
	let filterStatus = $state('all');
	let filterType   = $state('all');
	let filterSearch = $state('');
	let filterFrom   = $state('');
	let filterTo     = $state('');

	let selectedTx = $state<Tx | null>(null);

	function buildQuery(nextCursor?: string): string {
		const p = new URLSearchParams({ limit: '25' });
		if (filterChain  !== 'all') p.set('chain',  filterChain);
		if (filterStatus !== 'all') p.set('status', filterStatus);
		if (filterType   !== 'all') p.set('type',   filterType);
		if (filterFrom)             p.set('from',   filterFrom);
		if (filterTo)               p.set('to',     filterTo);
		if (filterSearch)           p.set('q',      filterSearch);
		if (nextCursor)             p.set('cursor', nextCursor);
		return `/v1/transactions?${p}`;
	}

	async function load() {
		loading = true;
		error = '';
		txs = [];
		cursor = null;
		try {
			const data = await api.getJson<{ transactions: any[]; next_cursor: string | null }>(buildQuery());
			txs = (data.transactions ?? []).map(normalize);
			cursor = data.next_cursor ?? null;
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
			const data = await api.getJson<{ transactions: any[]; next_cursor: string | null }>(buildQuery(cursor));
			txs = [...txs, ...(data.transactions ?? []).map(normalize)];
			cursor = data.next_cursor ?? null;
			hasMore = !!cursor;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load more';
		} finally {
			loadingMore = false;
		}
	}

	function resetFilters() {
		filterChain = 'all';
		filterStatus = 'all';
		filterType = 'all';
		filterSearch = '';
		filterFrom = '';
		filterTo = '';
		load();
	}

	let activeFilters = $derived(
		(filterChain !== 'all' ? 1 : 0) +
		(filterStatus !== 'all' ? 1 : 0) +
		(filterType !== 'all' ? 1 : 0) +
		(filterSearch ? 1 : 0) +
		(filterFrom ? 1 : 0) +
		(filterTo ? 1 : 0)
	);

	onMount(load);

	function statusTone(s: string): 'success' | 'pending' | 'danger' | 'muted' {
		if (s === 'confirmed') return 'success';
		if (s === 'broadcast' || s === 'pending' || s === 'signed' || s === 'validated' || s === 'received') return 'pending';
		if (s === 'failed' || s === 'rejected') return 'danger';
		return 'muted';
	}

	function typeIconName(t: string): string {
		const tt = t.toLowerCase();
		if (tt.includes('deposit') || tt.includes('receive')) return 'arrow-down-left';
		if (tt.includes('withdraw') || tt.includes('send')) return 'arrow-up-right';
		if (tt.includes('sweep')) return 'refresh';
		if (tt.includes('internal')) return 'activity';
		return 'activity';
	}

	function typeTone(t: string): string {
		const tt = t.toLowerCase();
		if (tt.includes('deposit') || tt.includes('receive')) return 'in';
		if (tt.includes('withdraw') || tt.includes('send')) return 'out';
		return 'neutral';
	}

	function trunc(s: string, pre = 10, post = 8): string {
		if (!s) return '—';
		if (s.length <= pre + post + 3) return s;
		return `${s.slice(0, pre)}…${s.slice(-post)}`;
	}

	function fmtDate(iso: string): string {
		return new Date(iso).toLocaleString(undefined, {
			month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
		});
	}

	function fmtDateLong(iso: string): string {
		return new Date(iso).toLocaleString(undefined, {
			year: 'numeric', month: 'long', day: 'numeric',
			hour: '2-digit', minute: '2-digit', second: '2-digit',
		});
	}

	function copy(s: string) { navigator.clipboard.writeText(s); }
</script>

<!-- ═════════════════ Header ═════════════════ -->
<div class="page-head">
	<div>
		<h1>{$locale, t('tx.title')}</h1>
		<p class="page-sub">{$locale, t('tx.subtitle')}</p>
	</div>
	<div class="head-actions">
		<button class="btn btn-ghost" onclick={load} title="Refresh">
			<span>{@html icon('refresh', 14)}</span>
			<span>Refresh</span>
		</button>
		<button class="btn btn-ghost" title="Export CSV">
			<span>{@html icon('download', 14)}</span>
			<span>Export</span>
		</button>
	</div>
</div>

<!-- ═════════════════ Filter bar ═════════════════ -->
<section class="filter-bar">
	<div class="search-wrap">
		<span class="search-icon">{@html icon('search', 14)}</span>
		<input
			type="text"
			bind:value={filterSearch}
			placeholder={($locale, t('tx.search_hint'))}
			onkeydown={(e) => { if (e.key === 'Enter') load(); }}
		/>
	</div>

	<div class="filter-group">
		<select class="filter-select" bind:value={filterType} onchange={load}>
			{#each TYPES as t}
				<option value={t}>{t === 'all' ? 'All types' : t.charAt(0).toUpperCase() + t.slice(1)}</option>
			{/each}
		</select>

		<select class="filter-select" bind:value={filterChain} onchange={load}>
			{#each CHAINS as c}
				<option value={c}>{c === 'all' ? 'All chains' : c.charAt(0).toUpperCase() + c.slice(1)}</option>
			{/each}
		</select>

		<select class="filter-select" bind:value={filterStatus} onchange={load}>
			{#each STATUSES as s}
				<option value={s}>{s === 'all' ? 'All statuses' : s.charAt(0).toUpperCase() + s.slice(1)}</option>
			{/each}
		</select>

		<div class="date-range">
			<input type="date" bind:value={filterFrom} onchange={load} aria-label="From" />
			<span class="date-sep">→</span>
			<input type="date" bind:value={filterTo} onchange={load} aria-label="To" />
		</div>

		{#if activeFilters > 0}
			<button class="btn btn-ghost sm" onclick={resetFilters}>
				<span>{@html icon('x', 12)}</span>
				<span>Clear ({activeFilters})</span>
			</button>
		{/if}
	</div>
</section>

<!-- ═════════════════ Content ═════════════════ -->
{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn btn-ghost sm" onclick={load}>Retry</button>
	</div>
{/if}

<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{$locale, t('tx.ledger')}</h2>
			<p class="panel-sub">{loading ? 'Loading…' : `${txs.length} transaction${txs.length === 1 ? '' : 's'}`}</p>
		</div>
		<div class="head-stats">
			{#if !loading && txs.length > 0}
				<span class="badge-count">{txs.length}</span>
			{/if}
		</div>
	</header>

	{#if loading}
		<div class="panel-body">
			{#each Array(6) as _}
				<div class="skel-row"></div>
			{/each}
		</div>
	{:else if txs.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('transactions', 22)}</span>
			<div>
				<p class="empty-title">{$locale, t('tx.no_match')}</p>
				<p class="muted">{$locale, t('tx.no_match_sub')}</p>
			</div>
			{#if activeFilters > 0}
				<button class="btn btn-secondary" onclick={resetFilters}>Clear filters</button>
			{/if}
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>{$locale, t('tx.type')}</th>
					<th>{$locale, t('tx.asset')}</th>
					<th>{$locale, t('tx.network')}</th>
					<th class="num">{$locale, t('tx.amount')}</th>
					<th class="num">{$locale, t('tx.value')}</th>
					<th>{$locale, t('tx.reference')}</th>
					<th>{$locale, t('tx.status')}</th>
					<th class="num">{$locale, t('tx.time')}</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each txs as tx}
					<tr class:selected={selectedTx?.id === tx.id} onclick={() => selectedTx = tx}>
						<td>
							<div class="type-cell">
								<span class="type-ico tone-{typeTone(tx.type)}">{@html icon(typeIconName(tx.type), 12)}</span>
								<span class="capitalize strong">{tx.type}</span>
							</div>
						</td>
						<td>
							<div class="token-cell">
								{@html tokenIcon(tx.asset, 18)}
								<span class="strong">{tx.asset}</span>
							</div>
						</td>
						<td>
							<div class="chain-cell">
								{@html chainIcon(tx.chain, 14)}
								<span class="capitalize muted">{tx.chain}</span>
							</div>
						</td>
						<td class="num mono strong">{tx.amount}</td>
						<td class="num mono muted">{tx.usd_value ? `$${tx.usd_value}` : '—'}</td>
						<td class="mono muted">{tx.tx_hash ? trunc(tx.tx_hash) : trunc(tx.id)}</td>
						<td><span class="status-pill {statusTone(tx.status)}">{tx.status}</span></td>
						<td class="num mono muted">{fmtDate(tx.created_at)}</td>
						<td>
							<button class="row-action" aria-label="View">{@html icon('chevron-right', 14)}</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>

		{#if hasMore}
			<div class="load-more">
				<button class="btn btn-ghost" onclick={loadMore} disabled={loadingMore}>
					{#if loadingMore}
						<span class="spinner sm"></span>
						<span>Loading…</span>
					{:else}
						<span>{$locale, t('tx.load_more')}</span>
						<span>{@html icon('chevron-down', 14)}</span>
					{/if}
				</button>
			</div>
		{/if}
	{/if}
</section>

<!-- ═════════════════ Detail drawer ═════════════════ -->
{#if selectedTx}
	<div class="drawer-backdrop" role="presentation" onclick={() => selectedTx = null} onkeydown={(e) => e.key === 'Escape' && (selectedTx = null)}></div>
	<aside class="drawer">
		<header class="drawer-head">
			<div class="drawer-title">
				<span class="type-ico lg tone-{typeTone(selectedTx.type)}">{@html icon(typeIconName(selectedTx.type), 16)}</span>
				<div>
					<h3 class="capitalize">{selectedTx.type}</h3>
					<span class="muted mono">{trunc(selectedTx.id, 12, 8)}</span>
				</div>
			</div>
			<button class="icon-btn" onclick={() => selectedTx = null} aria-label="Close">
				{@html icon('x', 16)}
			</button>
		</header>

		<div class="drawer-amount">
			<span class="amount-asset">
				{@html tokenIcon(selectedTx.asset, 24)}
				<span class="amount-val mono">{selectedTx.amount}</span>
				<span class="amount-sym">{selectedTx.asset}</span>
			</span>
			{#if selectedTx.usd_value}
				<span class="amount-usd mono muted">≈ ${selectedTx.usd_value}</span>
			{/if}
		</div>

		<div class="drawer-body">
			<dl class="detail-list">
				<div>
					<dt>{$locale, t('tx.status')}</dt>
					<dd><span class="status-pill {statusTone(selectedTx.status)}">{selectedTx.status}</span></dd>
				</div>
				<div>
					<dt>{$locale, t('tx.network')}</dt>
					<dd class="capitalize">
						<span class="token-cell">{@html chainIcon(selectedTx.chain, 16)} {selectedTx.chain}</span>
					</dd>
				</div>
				<div>
					<dt>{$locale, t('tx.timestamp')}</dt>
					<dd>{fmtDateLong(selectedTx.created_at)}</dd>
				</div>
				{#if selectedTx.destination}
					<div>
						<dt>{$locale, t('tx.destination')}</dt>
						<dd>
							<div class="copyable">
								<code class="mono">{selectedTx.destination}</code>
								<button class="copy-mini" onclick={() => copy(selectedTx!.destination!)} aria-label="Copy">
									{@html icon('copy', 12)}
								</button>
							</div>
						</dd>
					</div>
				{/if}
				{#if selectedTx.tx_hash}
					<div>
						<dt>{$locale, t('tx.hash')}</dt>
						<dd>
							<div class="copyable">
								<code class="mono">{selectedTx.tx_hash}</code>
								<button class="copy-mini" onclick={() => copy(selectedTx!.tx_hash!)} aria-label="Copy">
									{@html icon('copy', 12)}
								</button>
							</div>
						</dd>
					</div>
				{/if}
				<div>
					<dt>{$locale, t('tx.custody_ref')}</dt>
					<dd>
						<div class="copyable">
							<code class="mono">{selectedTx.id}</code>
							<button class="copy-mini" onclick={() => copy(selectedTx!.id)} aria-label="Copy">
								{@html icon('copy', 12)}
							</button>
						</div>
					</dd>
				</div>
			</dl>

			{#if selectedTx.tx_hash}
				<a
					class="btn btn-secondary full"
					href={`https://etherscan.io/tx/${selectedTx.tx_hash}`}
					target="_blank"
					rel="noopener"
				>
					<span>{@html icon('external', 14)}</span>
					<span>{$locale, t('tx.explorer')}</span>
				</a>
			{/if}
		</div>
	</aside>
{/if}

<style>
	/* ═════════ Head ═════════ */
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-5);
		flex-wrap: wrap;
	}
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	.head-actions { display: flex; gap: var(--sp-2); }

	/* ═════════ Filter bar ═════════ */
	.filter-bar {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-4);
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		margin-bottom: var(--sp-4);
		flex-wrap: wrap;
	}

	.search-wrap {
		position: relative;
		flex: 1;
		min-width: 220px;
		display: flex;
		align-items: center;
	}

	.search-icon {
		position: absolute;
		left: 10px;
		color: var(--fg-dim);
		display: inline-flex;
		pointer-events: none;
	}

	.search-wrap input {
		width: 100%;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.85rem;
		padding: 7px 12px 7px 32px;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.search-wrap input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }
	.search-wrap input::placeholder { color: var(--fg-dim); }

	.filter-group {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.filter-select {
		appearance: none;
		-webkit-appearance: none;
		background: var(--surface-2) url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%238A919E' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polyline points='6 9 12 15 18 9'/></svg>") no-repeat right 10px center;
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.82rem;
		padding: 6px 28px 6px 10px;
		cursor: pointer;
		outline: none;
		transition: border-color 0.15s;
	}
	.filter-select:hover { border-color: var(--border-strong); }
	.filter-select:focus { border-color: var(--accent); }

	.date-range {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
	}

	.date-range input {
		background: transparent;
		border: none;
		color: var(--fg);
		font-size: 0.8rem;
		padding: 5px 6px;
		outline: none;
		font-family: inherit;
		color-scheme: dark;
	}
	:global(:root.light) .date-range input { color-scheme: light; }

	.date-sep { color: var(--fg-dim); font-size: 0.8rem; }

	/* ═════════ Buttons ═════════ */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 12px;
		font-size: 0.84rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s;
		text-decoration: none;
	}
	.btn.sm { padding: 4px 8px; font-size: 0.76rem; }
	.btn.full { width: 100%; justify-content: center; }
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }
	.btn-secondary { background: var(--surface-2); color: var(--fg); }

	.icon-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px; height: 28px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		cursor: pointer;
	}
	.icon-btn:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	/* ═════════ Alert ═════════ */
	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.alert .btn { margin-left: auto; }

	/* ═════════ Panel ═════════ */
	.panel {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-1);
		margin-bottom: var(--sp-4);
		overflow: hidden;
	}

	.panel-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}

	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }

	.badge-count {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 3px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

	.panel-body { padding: var(--sp-4) var(--sp-5); }

	/* ═════════ Data table ═════════ */
	.data-table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
	.data-table thead th {
		text-align: left;
		padding: var(--sp-3) var(--sp-5);
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		border-bottom: 1px solid var(--border);
		background: var(--surface-2);
		position: sticky;
		top: 0;
	}
	.data-table tbody td {
		padding: var(--sp-3) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr { cursor: pointer; transition: background 0.1s; }
	.data-table tbody tr:hover { background: var(--hover-bg); }
	.data-table tbody tr.selected { background: var(--accent-soft); }
	.data-table th.num, .data-table td.num { text-align: right; }

	.type-cell { display: flex; align-items: center; gap: 10px; }
	.type-ico {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px; height: 22px;
		border-radius: var(--r-sm);
		flex-shrink: 0;
	}
	.type-ico.lg { width: 32px; height: 32px; }
	.type-ico.tone-in { background: var(--green-soft); color: var(--green); }
	.type-ico.tone-out { background: var(--red-soft); color: var(--red); }
	.type-ico.tone-neutral { background: var(--surface-2); color: var(--fg-muted); }

	.token-cell, .chain-cell { display: inline-flex; align-items: center; gap: 8px; }
	.token-cell :global(svg), .chain-cell :global(svg) { border-radius: 50%; }

	.capitalize { text-transform: capitalize; }
	.strong { color: var(--fg-strong); font-weight: 500; }
	.muted { color: var(--fg-muted); }

	.status-pill {
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: capitalize;
		padding: 2px 8px;
		border-radius: var(--r-full);
	}
	.status-pill.success { background: var(--green-soft); color: var(--green); }
	.status-pill.pending { background: var(--yellow-soft); color: var(--yellow); }
	.status-pill.danger  { background: var(--red-soft); color: var(--red); }
	.status-pill.muted   { background: var(--surface-2); color: var(--fg-muted); }

	.row-action {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 24px; height: 24px;
		background: transparent;
		border: none;
		color: var(--fg-dim);
		cursor: pointer;
		border-radius: var(--r-sm);
	}
	tr:hover .row-action { color: var(--fg); background: var(--surface-2); }

	/* ═════════ Load more ═════════ */
	.load-more {
		display: flex;
		justify-content: center;
		padding: var(--sp-4);
		border-top: 1px solid var(--border);
	}

	.spinner {
		width: 13px; height: 13px;
		border: 2px solid var(--fg-muted);
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.spinner.sm { width: 11px; height: 11px; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ═════════ Empty ═════════ */
	.panel-empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-6);
	}
	.empty-mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 44px; height: 44px;
		background: var(--accent-soft);
		color: var(--accent);
		border-radius: var(--r-md);
		flex-shrink: 0;
	}
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.skel-row {
		height: 44px;
		margin-bottom: 6px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	/* ═════════ Drawer ═════════ */
	.drawer-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		backdrop-filter: blur(4px);
		z-index: 80;
		animation: fadeIn 0.15s;
	}
	@keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

	.drawer {
		position: fixed;
		top: 0; right: 0;
		width: 460px;
		max-width: 100vw;
		height: 100vh;
		background: var(--surface);
		border-left: 1px solid var(--border);
		box-shadow: -10px 0 40px rgba(0, 0, 0, 0.4);
		z-index: 90;
		display: flex;
		flex-direction: column;
		animation: slideIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
	}
	@keyframes slideIn { from { transform: translateX(100%); } to { transform: translateX(0); } }

	.drawer-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.drawer-title { display: flex; align-items: center; gap: 12px; }
	.drawer-title h3 { margin: 0; font-size: 1rem; font-weight: 600; color: var(--fg-strong); }
	.drawer-title .muted { font-size: 0.76rem; }

	.drawer-amount {
		padding: var(--sp-5);
		text-align: center;
		background: var(--surface-2);
		border-bottom: 1px solid var(--border);
	}
	.amount-asset {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		font-size: 1.6rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
	}
	.amount-asset :global(svg) { border-radius: 50%; }
	.amount-val { font-size: 1.6rem; }
	.amount-sym { color: var(--fg-muted); font-size: 1.1rem; font-weight: 500; }
	.amount-usd { display: block; margin-top: 6px; font-size: 0.88rem; }

	.drawer-body {
		flex: 1;
		overflow-y: auto;
		padding: var(--sp-5);
	}

	.detail-list { margin: 0 0 var(--sp-4); padding: 0; }
	.detail-list > div {
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: var(--sp-4);
		padding: var(--sp-3) 0;
		border-bottom: 1px solid var(--border);
	}
	.detail-list > div:last-child { border-bottom: none; }

	.detail-list dt {
		font-size: 0.76rem;
		color: var(--fg-muted);
		font-weight: 500;
		flex-shrink: 0;
		min-width: 120px;
	}
	.detail-list dd {
		margin: 0;
		font-size: 0.84rem;
		text-align: right;
		color: var(--fg);
		word-break: break-all;
	}

	.copyable {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 4px 8px;
	}
	.copyable code { font-size: 0.76rem; color: var(--fg); }
	.copy-mini {
		background: transparent;
		border: none;
		color: var(--fg-dim);
		cursor: pointer;
		display: inline-flex;
		padding: 2px;
		border-radius: 3px;
	}
	.copy-mini:hover { color: var(--accent); background: var(--hover-bg); }

	/* ═════════ Responsive ═════════ */
	@media (max-width: 1000px) {
		.filter-bar { flex-direction: column; align-items: stretch; }
		.filter-group { width: 100%; }
		.date-range { flex: 1; }
	}
	@media (max-width: 700px) {
		.drawer { width: 100vw; }
		.data-table thead th, .data-table tbody td { padding: 10px 12px; }
		.data-table { font-size: 0.78rem; }
	}
</style>
