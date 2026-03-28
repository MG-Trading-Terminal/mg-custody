<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface AssetBalance {
		asset: string;
		balance: string;
		usd_value: string;
		pct: string;
	}

	interface ActivityEvent {
		id: string;
		type: string;
		asset: string;
		amount: string;
		status: string;
		created_at: string;
	}

	interface Portfolio {
		total_usd: string;
		balances: AssetBalance[];
		pending_approvals: number;
	}

	let portfolio = $state<Portfolio | null>(null);
	let activity  = $state<ActivityEvent[]>([]);
	let loading   = $state(true);
	let error     = $state('');

	onMount(async () => {
		try {
			const [port, act] = await Promise.all([
				api.getJson<Portfolio>('/v1/user/portfolio'),
				api.getJson<{ events: ActivityEvent[] }>('/v1/user/activity'),
			]);
			portfolio = port;
			activity  = act.events ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load portfolio';
		} finally {
			loading = false;
		}
	});

	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
	const STATUS_COLOR: Record<string, string> = {
		confirmed: 'green',
		broadcast: 'yellow',
		pending:   'yellow',
		failed:    'red',
	};
	function statusClass(s: string) { return STATUS_COLOR[s] ?? 'muted'; }
</script>

<h1>Portfolio</h1>

{#if loading}
	<p class="muted">Loading…</p>
{:else if error}
	<p class="err">{error}</p>
{:else if portfolio}
	<div class="top-row">
		<div class="card aum-card">
			<p class="aum-label">Total Balance</p>
			<p class="aum-value">${portfolio.total_usd}</p>
		</div>
		<div class="actions">
			<a class="btn-primary" href="/deposit">Deposit</a>
			<a class="btn-secondary" href="/withdraw">Withdraw</a>
			{#if portfolio.pending_approvals > 0}
				<a class="btn-warn" href="/approvals">
					Approvals
					<span class="badge-count">{portfolio.pending_approvals}</span>
				</a>
			{/if}
		</div>
	</div>

	<div class="card">
		<h3>Assets</h3>
		{#if portfolio.balances.length === 0}
			<p class="muted">No balances yet. Make a deposit to get started.</p>
		{:else}
			<div class="table-wrap"><table>
				<thead>
					<tr><th>Asset</th><th>Balance</th><th>USD Value</th><th>% of Portfolio</th></tr>
				</thead>
				<tbody>
					{#each portfolio.balances as row}
						<tr>
							<td class="asset-name">{row.asset}</td>
							<td class="mono">{row.balance}</td>
							<td class="mono">${row.usd_value}</td>
							<td>
								<div class="pct-row">
									<div class="pct-bar">
										<div class="pct-fill" style="width:{row.pct}%"></div>
									</div>
									<span class="muted">{row.pct}%</span>
								</div>
							</td>
						</tr>
					{/each}
				</tbody>
			</table></div>
		{/if}
	</div>

	<div class="card">
		<h3>Recent Activity</h3>
		{#if activity.length === 0}
			<p class="muted">No recent activity.</p>
		{:else}
			<ul class="activity-list">
				{#each activity as ev}
					<li class="activity-row">
						<span class="activity-type">{ev.type}</span>
						<span class="activity-asset muted">{ev.asset}</span>
						<span class="mono">{ev.amount}</span>
						<span class="badge {statusClass(ev.status)}">{ev.status}</span>
						<span class="muted activity-date">{fmt(ev.created_at)}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h3 { margin: 0 0 0.9rem; font-size: 0.9rem; color: var(--fg-muted, #8b949e); text-transform: uppercase; letter-spacing: 0.05em; }

	.top-row { display: flex; gap: 1rem; align-items: flex-start; margin-bottom: 1rem; flex-wrap: wrap; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		margin-bottom: 1rem;
	}

	.aum-card { flex: 1; min-width: 220px; }
	.aum-label { margin: 0 0 0.25rem; font-size: 0.8rem; color: var(--fg-muted, #8b949e); text-transform: uppercase; letter-spacing: 0.05em; }
	.aum-value { margin: 0; font-size: 2.4rem; font-weight: 700; font-family: monospace; color: var(--fg, #e1e4e8); }

	.actions { display: flex; gap: 0.75rem; align-items: flex-start; padding-top: 0.3rem; flex-wrap: wrap; }

	.btn-primary, .btn-secondary, .btn-warn {
		display: inline-flex; align-items: center; gap: 0.4rem;
		border-radius: 6px; padding: 0.55rem 1.2rem;
		font-size: 0.9rem; font-weight: 600; text-decoration: none; cursor: pointer;
	}
	.btn-primary  { background: var(--accent, #58a6ff); color: #0f1117; border: none; }
	.btn-secondary { background: none; color: var(--accent, #58a6ff); border: 1px solid var(--accent, #58a6ff); }
	.btn-warn     { background: none; color: var(--yellow, #d29922); border: 1px solid var(--yellow, #d29922); }

	.badge-count {
		background: var(--yellow, #d29922); color: #0f1117;
		border-radius: 99px; padding: 0.05rem 0.45rem;
		font-size: 0.75rem; font-weight: 700;
	}

	.table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }
	tr:last-child td { border-bottom: none; }

	.asset-name { font-weight: 600; }

	.pct-row { display: flex; align-items: center; gap: 0.5rem; }
	.pct-bar { flex: 1; max-width: 80px; height: 6px; background: var(--border, #30363d); border-radius: 3px; overflow: hidden; }
	.pct-fill { height: 100%; background: var(--accent, #58a6ff); border-radius: 3px; }

	.activity-list { list-style: none; padding: 0; margin: 0; }
	.activity-row {
		display: flex; align-items: center; gap: 0.75rem;
		padding: 0.55rem 0; border-bottom: 1px solid var(--border, #30363d);
		font-size: 0.88rem;
	}
	.activity-row:last-child { border-bottom: none; }
	.activity-type { font-weight: 600; text-transform: capitalize; min-width: 80px; }
	.activity-asset { min-width: 50px; }
	.activity-date { margin-left: auto; font-size: 0.8rem; }

	.badge { border-radius: 4px; padding: 0.15rem 0.5rem; font-size: 0.75rem; text-transform: capitalize; font-weight: 600; }
	.green  { background: rgba(63,185,80,.12);  color: var(--green, #3fb950); }
	.yellow { background: rgba(210,153,34,.12); color: var(--yellow, #d29922); }
	.red    { background: rgba(248,81,73,.12);  color: var(--red, #f85149); }
	.muted  { color: var(--fg-muted, #8b949e); }
	.mono   { font-family: monospace; }
	.err    { color: var(--red, #f85149); }

	@media (max-width: 768px) {
		.top-row { flex-direction: column; }
		.aum-card { min-width: unset; width: 100%; }
		.actions { flex-wrap: wrap; }
		.activity-row { flex-wrap: wrap; gap: 0.4rem; }
		.activity-date { margin-left: 0; width: 100%; }
	}
</style>
