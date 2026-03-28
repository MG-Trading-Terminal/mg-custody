<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	// -- Types --

	interface AnalyticsSummary {
		period: string;
		total_inflow_usd: string;
		total_outflow_usd: string;
		net_flow_usd: string;
		total_fees_usd: string;
		avg_fee_per_tx_usd: string;
		tx_count: number;
		deposits_count: number;
		withdrawals_count: number;
		busiest_chain: string;
		largest_tx_usd: string;
	}

	interface FeeBreakdown {
		chain: string;
		total_fees_native: string;
		total_fees_usd: string;
		avg_fee_native: string;
		tx_count: number;
	}

	interface TopTransaction {
		id: string;
		tx_type: string;
		asset: string;
		chain: string;
		amount_usd: string;
		fee_usd: string | null;
		created_at: string;
		tx_hash: string | null;
	}

	// -- State --

	let summary       = $state<AnalyticsSummary | null>(null);
	let fees          = $state<FeeBreakdown[]>([]);
	let topTxs        = $state<TopTransaction[]>([]);
	let loading       = $state(true);
	let error         = $state('');

	// Filter controls
	let periodType    = $state<'daily' | 'monthly'>('daily');
	let filterDate    = $state(new Date().toISOString().slice(0, 10));
	let filterYear    = $state(new Date().getFullYear());
	let filterMonth   = $state(new Date().getMonth() + 1);
	let feeFrom       = $state(firstDayOfMonth());
	let feeTo         = $state(new Date().toISOString().slice(0, 10));
	let topLimit      = $state(10);

	function firstDayOfMonth(): string {
		const d = new Date();
		return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-01`;
	}

	function buildSummaryQuery(): string {
		const p = new URLSearchParams({ period: periodType });
		if (periodType === 'daily') {
			p.set('date', filterDate);
		} else {
			p.set('year', String(filterYear));
			p.set('month', String(filterMonth));
		}
		return `/v1/analytics/summary?${p}`;
	}

	function buildFeesQuery(): string {
		return `/v1/analytics/fees?from=${feeFrom}&to=${feeTo}`;
	}

	function buildTopQuery(): string {
		return `/v1/analytics/top-transactions?limit=${topLimit}`;
	}

	async function loadAll() {
		loading = true; error = '';
		try {
			const [s, f, t] = await Promise.all([
				api.getJson<{ analytics: AnalyticsSummary }>(buildSummaryQuery()),
				api.getJson<{ fees: FeeBreakdown[] }>(buildFeesQuery()),
				api.getJson<{ transactions: TopTransaction[] }>(buildTopQuery()),
			]);
			summary = s.analytics ?? null;
			fees    = f.fees ?? [];
			topTxs  = t.transactions ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load analytics';
		} finally {
			loading = false;
		}
	}

	function fmtUsd(s: string | undefined | null): string {
		if (!s) return '$0.00';
		const n = parseFloat(s);
		return isNaN(n) ? s : `$${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
	}

	function fmtDate(s: string): string {
		return new Date(s).toLocaleString();
	}

	onMount(loadAll);
</script>

<div class="page">
	<h1>Transaction Analytics</h1>

	<!-- Filter bar -->
	<div class="filters">
		<label>
			Period:
			<select bind:value={periodType}>
				<option value="daily">Daily</option>
				<option value="monthly">Monthly</option>
			</select>
		</label>

		{#if periodType === 'daily'}
			<label>
				Date:
				<input type="date" bind:value={filterDate} />
			</label>
		{:else}
			<label>
				Year:
				<input type="number" bind:value={filterYear} min="2020" max="2099" style="width:80px" />
			</label>
			<label>
				Month:
				<input type="number" bind:value={filterMonth} min="1" max="12" style="width:60px" />
			</label>
		{/if}

		<label>
			Fees from:
			<input type="date" bind:value={feeFrom} />
		</label>
		<label>
			to:
			<input type="date" bind:value={feeTo} />
		</label>

		<label>
			Top N:
			<input type="number" bind:value={topLimit} min="1" max="100" style="width:60px" />
		</label>

		<button onclick={loadAll}>Refresh</button>
	</div>

	{#if loading}
		<p class="loading">Loading analytics…</p>
	{:else if error}
		<p class="error">{error}</p>
	{:else}

		<!-- Summary cards -->
		{#if summary}
			<section class="cards">
				<div class="card inflow">
					<div class="card-label">Total Inflow</div>
					<div class="card-value">{fmtUsd(summary.total_inflow_usd)}</div>
					<div class="card-sub">{summary.deposits_count} deposits</div>
				</div>
				<div class="card outflow">
					<div class="card-label">Total Outflow</div>
					<div class="card-value">{fmtUsd(summary.total_outflow_usd)}</div>
					<div class="card-sub">{summary.withdrawals_count} withdrawals</div>
				</div>
				<div class="card net">
					<div class="card-label">Net Flow</div>
					<div class="card-value">{fmtUsd(summary.net_flow_usd)}</div>
					<div class="card-sub">{summary.tx_count} transactions total</div>
				</div>
				<div class="card fees">
					<div class="card-label">Total Fees</div>
					<div class="card-value">{fmtUsd(summary.total_fees_usd)}</div>
					<div class="card-sub">avg {fmtUsd(summary.avg_fee_per_tx_usd)} / tx</div>
				</div>
				<div class="card chain">
					<div class="card-label">Busiest Chain</div>
					<div class="card-value chain-name">{summary.busiest_chain || '—'}</div>
					<div class="card-sub">largest tx {fmtUsd(summary.largest_tx_usd)}</div>
				</div>
			</section>
		{:else}
			<p class="empty">No summary data for this period.</p>
		{/if}

		<!-- Per-chain fee table -->
		<section class="section">
			<h2>Fee Breakdown by Chain</h2>
			{#if fees.length === 0}
				<p class="empty">No fee data for the selected range.</p>
			{:else}
				<table>
					<thead>
						<tr>
							<th>Chain</th>
							<th>Transactions</th>
							<th>Total Fees (native)</th>
							<th>Avg Fee (native)</th>
							<th>Total Fees (USD)</th>
						</tr>
					</thead>
					<tbody>
						{#each fees as f}
							<tr>
								<td><span class="chain-badge">{f.chain}</span></td>
								<td>{f.tx_count}</td>
								<td>{f.total_fees_native}</td>
								<td>{f.avg_fee_native}</td>
								<td>{fmtUsd(f.total_fees_usd)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</section>

		<!-- Top transactions table -->
		<section class="section">
			<h2>Top Transactions (by USD value)</h2>
			{#if topTxs.length === 0}
				<p class="empty">No transactions found.</p>
			{:else}
				<table>
					<thead>
						<tr>
							<th>Date</th>
							<th>Type</th>
							<th>Asset</th>
							<th>Chain</th>
							<th>Amount (USD)</th>
							<th>Fee (USD)</th>
							<th>Tx Hash</th>
						</tr>
					</thead>
					<tbody>
						{#each topTxs as tx}
							<tr>
								<td>{fmtDate(tx.created_at)}</td>
								<td><span class="badge {tx.tx_type}">{tx.tx_type}</span></td>
								<td>{tx.asset}</td>
								<td><span class="chain-badge">{tx.chain}</span></td>
								<td>{fmtUsd(tx.amount_usd)}</td>
								<td>{tx.fee_usd ? fmtUsd(tx.fee_usd) : '—'}</td>
								<td>
									{#if tx.tx_hash}
										<span class="hash" title={tx.tx_hash}>{tx.tx_hash.slice(0, 12)}…</span>
									{:else}
										—
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</section>

	{/if}
</div>

<style>
	.page { padding: 1.5rem; max-width: 1200px; margin: 0 auto; }
	h1 { margin: 0 0 1.25rem; font-size: 1.5rem; }
	h2 { margin: 0 0 0.75rem; font-size: 1.1rem; }

	.filters {
		display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: center;
		background: #f8f9fa; border: 1px solid #e2e8f0;
		border-radius: 6px; padding: 0.75rem 1rem; margin-bottom: 1.5rem;
	}
	.filters label { display: flex; gap: 0.4rem; align-items: center; font-size: 0.875rem; }
	.filters input, .filters select { padding: 0.3rem 0.5rem; border: 1px solid #cbd5e0; border-radius: 4px; font-size: 0.875rem; }
	.filters button { padding: 0.4rem 1rem; background: #3182ce; color: #fff; border: none; border-radius: 4px; cursor: pointer; font-size: 0.875rem; }
	.filters button:hover { background: #2b6cb0; }

	.cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 1rem; margin-bottom: 2rem;
	}
	.card {
		background: #fff; border: 1px solid #e2e8f0; border-radius: 8px;
		padding: 1rem; display: flex; flex-direction: column; gap: 0.25rem;
	}
	.card-label { font-size: 0.75rem; text-transform: uppercase; color: #718096; letter-spacing: 0.05em; }
	.card-value { font-size: 1.4rem; font-weight: 700; }
	.card-sub   { font-size: 0.75rem; color: #718096; }
	.card.inflow  .card-value { color: #2f855a; }
	.card.outflow .card-value { color: #c53030; }
	.card.net     .card-value { color: #2b6cb0; }
	.card.fees    .card-value { color: #744210; }
	.chain-name { text-transform: capitalize; }

	.section { margin-bottom: 2rem; }

	table { width: 100%; border-collapse: collapse; font-size: 0.875rem; }
	thead th { background: #f7fafc; text-align: left; padding: 0.6rem 0.75rem; border-bottom: 2px solid #e2e8f0; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; color: #4a5568; }
	tbody td { padding: 0.6rem 0.75rem; border-bottom: 1px solid #edf2f7; }
	tbody tr:hover { background: #f7fafc; }

	.badge { padding: 0.2rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600; text-transform: capitalize; }
	.badge.deposit    { background: #c6f6d5; color: #276749; }
	.badge.withdrawal { background: #fed7d7; color: #9b2c2c; }
	.chain-badge { padding: 0.15rem 0.4rem; background: #ebf8ff; color: #2b6cb0; border-radius: 4px; font-size: 0.75rem; text-transform: capitalize; }
	.hash { font-family: monospace; font-size: 0.8rem; color: #4a5568; }

	.loading { color: #718096; }
	.error   { color: #c53030; }
	.empty   { color: #718096; font-style: italic; }
</style>
