<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { t, locale } from '$lib/i18n';
	import { tokenIcon, chainIcon } from '$lib/icons/tokens';

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
		assets: AssetBalance[];
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
			// API returns "assets", dashboard uses "balances" — normalize
			port.balances = port.balances ?? port.assets ?? [];
			port.pending_approvals = port.pending_approvals ?? 0;
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

<!-- $locale read here makes the entire template reactive to locale changes -->
<h1>{$locale, t('portfolio.title')}</h1>

{#if loading}
	<p class="muted">{$locale, t('common.loading')}</p>
{:else if error}
	<p class="err">{error}</p>
{:else if portfolio}
	{#if portfolio.balances.length === 0 && activity.length === 0}
		<!-- Onboarding card for new users -->
		<div class="card onboarding">
			<div class="onboarding-header">
				<span class="onboarding-icon">
					<svg width="40" height="40" viewBox="0 0 40 40" fill="none"><circle cx="20" cy="20" r="20" fill="var(--accent, #58a6ff)" opacity="0.15"/><path d="M20 10v10m0 0v10m0-10h10m-10 0H10" stroke="var(--accent, #58a6ff)" stroke-width="2.5" stroke-linecap="round"/></svg>
				</span>
				<div>
					<h2 class="onboarding-title">Welcome to MG Custody</h2>
					<p class="onboarding-sub">Get started in 3 simple steps</p>
				</div>
			</div>
			<div class="onboarding-steps">
				<a href="/deposit" class="step">
					<span class="step-num">1</span>
					<div>
						<strong>Generate deposit address</strong>
						<p>Choose a chain and get your unique custody address with QR code</p>
					</div>
				</a>
				<a href="/deposit" class="step">
					<span class="step-num">2</span>
					<div>
						<strong>Send crypto</strong>
						<p>Transfer from your exchange or wallet. Scanner detects deposits automatically.</p>
					</div>
				</a>
				<a href="/transactions" class="step">
					<span class="step-num">3</span>
					<div>
						<strong>Track & manage</strong>
						<p>Monitor balances, approve withdrawals, set policies — all from this dashboard.</p>
					</div>
				</a>
			</div>
			<div class="onboarding-chains">
				<span class="chain-label">Supported chains:</span>
				{@html chainIcon('ethereum', 22)}
				{@html chainIcon('bitcoin', 22)}
				{@html chainIcon('tron', 22)}
				{@html chainIcon('solana', 22)}
				{@html chainIcon('arbitrum', 22)}
				{@html chainIcon('optimism', 22)}
				{@html chainIcon('polygon', 22)}
				{@html chainIcon('base', 22)}
				{@html chainIcon('avalanche', 22)}
				{@html chainIcon('bsc', 22)}
			</div>
		</div>
	{:else}
		<div class="top-row">
			<div class="card aum-card">
				<p class="aum-label">{$locale, t('portfolio.total')}</p>
				<p class="aum-value">${portfolio.total_usd}</p>
			</div>
			<div class="actions">
				<a class="btn-primary" href="/deposit">{$locale, t('portfolio.deposit')}</a>
				<a class="btn-secondary" href="/withdraw">{$locale, t('portfolio.withdraw')}</a>
				{#if portfolio.pending_approvals > 0}
					<a class="btn-warn" href="/approvals">
						{$locale, t('portfolio.approvals')}
						<span class="badge-count">{portfolio.pending_approvals}</span>
					</a>
				{/if}
			</div>
		</div>
	{/if}

	<div class="card">
		<h3>{$locale, t('portfolio.assets')}</h3>
		{#if portfolio.balances.length === 0}
			<p class="muted">{$locale, t('portfolio.no_balances')}</p>
		{:else}
			<div class="table-wrap"><table>
				<thead>
					<tr>
						<th>{$locale, t('portfolio.col_asset')}</th>
						<th>{$locale, t('portfolio.col_balance')}</th>
						<th>{$locale, t('portfolio.col_usd')}</th>
						<th>{$locale, t('portfolio.col_pct')}</th>
					</tr>
				</thead>
				<tbody>
					{#each portfolio.balances as row}
						<tr>
							<td class="asset-name"><span class="token-cell">{@html tokenIcon(row.asset, 20)} {row.asset}</span></td>
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
		<h3>{$locale, t('portfolio.activity')}</h3>
		{#if activity.length === 0}
			<p class="muted">{$locale, t('portfolio.no_activity')}</p>
		{:else}
			<ul class="activity-list">
				{#each activity as ev}
					<li class="activity-row">
						<span class="activity-type">{ev.type}</span>
						<span class="activity-asset muted">{@html tokenIcon(ev.asset, 14)} {ev.asset}</span>
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

	/* Onboarding card */
	.onboarding {
		border-color: var(--accent, #58a6ff);
		border-width: 2px;
	}

	.onboarding-header {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-bottom: 1.5rem;
	}

	.onboarding-icon { flex-shrink: 0; }

	.onboarding-title {
		margin: 0;
		font-size: 1.3rem;
		color: var(--fg, #e1e4e8);
	}

	.onboarding-sub {
		margin: 0.2rem 0 0;
		color: var(--fg-muted, #8b949e);
		font-size: 0.9rem;
	}

	.onboarding-steps {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-bottom: 1.5rem;
	}

	.step {
		display: flex;
		align-items: flex-start;
		gap: 0.9rem;
		padding: 0.8rem 1rem;
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		text-decoration: none;
		color: inherit;
		transition: border-color 0.15s, background 0.15s;
	}

	.step:hover {
		border-color: var(--accent, #58a6ff);
		background: var(--card-bg, #161b22);
	}

	.step-num {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		background: var(--accent, #58a6ff);
		color: #0f1117;
		font-weight: 700;
		font-size: 0.85rem;
		flex-shrink: 0;
	}

	.step strong {
		display: block;
		font-size: 0.95rem;
		margin-bottom: 0.15rem;
	}

	.step p {
		margin: 0;
		font-size: 0.82rem;
		color: var(--fg-muted, #8b949e);
	}

	.onboarding-chains {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.onboarding-chains :global(svg) { border-radius: 50%; }

	.chain-label {
		font-size: 0.8rem;
		color: var(--fg-muted, #8b949e);
		margin-right: 0.3rem;
	}

	@media (max-width: 768px) {
		.top-row { flex-direction: column; }
		.aum-card { min-width: unset; width: 100%; }
		.actions { flex-wrap: wrap; }
		.activity-row { flex-wrap: wrap; gap: 0.4rem; }
		.activity-date { margin-left: 0; width: 100%; }
	}
</style>
