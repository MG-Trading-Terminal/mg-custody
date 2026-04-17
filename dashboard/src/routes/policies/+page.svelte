<script lang="ts">
	import { api } from '$lib/api';
	import { onMount } from 'svelte';
	import { icon } from '$lib/icons/ui';
	import { t, locale } from '$lib/i18n';

	interface PolicyRule {
		id: string;
		name: string;
		priority: number;
		enabled: boolean;
		conditions: string;
		action: string;
	}

	interface CircuitBreakerStatus {
		is_tripped: boolean;
		single_tx_limit_usd: string;
		hourly_limit_usd: string;
		daily_limit_usd: string;
		hourly_outflow_usd: string;
		daily_outflow_usd: string;
		trip_reason?: string;
	}

	let rules = $state<PolicyRule[]>([]);
	let cbStatus = $state<CircuitBreakerStatus | null>(null);
	let loading = $state(true);
	let error = $state('');

	const TIERS = [
		{ tier: 0, range: '< $1K + whitelisted', approval: 'Auto',              delay: '1–5m',  tone: 'ok'   },
		{ tier: 1, range: '$1K – $10K',          approval: 'Auto + alert',      delay: '10m',   tone: 'ok'   },
		{ tier: 2, range: '$10K – $100K',        approval: '1 approver · FIDO2',delay: '30m',   tone: 'warn' },
		{ tier: 3, range: '$100K – $500K',       approval: '2 approvers',       delay: '2h',    tone: 'warn' },
		{ tier: 4, range: '> $500K',             approval: '3 approvers · cold', delay: '24h',   tone: 'crit' },
	];

	async function loadData() {
		loading = true;
		error = '';
		try {
			rules = await api.getJson<PolicyRule[]>('/v1/admin/policies');
		} catch { rules = []; }
		try {
			cbStatus = await api.getJson<CircuitBreakerStatus>('/v1/circuit-breaker/status');
		} catch { cbStatus = null; }
		loading = false;
	}

	function fmtUsd(val: string | undefined): string {
		if (!val) return '—';
		const n = parseFloat(val);
		if (isNaN(n)) return val;
		if (n >= 1_000_000) return `$${(n/1_000_000).toFixed(2)}M`;
		if (n >= 1_000) return `$${(n/1_000).toFixed(1)}K`;
		return `$${n.toFixed(2)}`;
	}

	function pctOfLimit(outflow: string | undefined, limit: string | undefined): number {
		if (!outflow || !limit) return 0;
		const o = parseFloat(outflow);
		const l = parseFloat(limit);
		if (l <= 0) return 0;
		return Math.min(100, (o / l) * 100);
	}

	function barTone(pct: number): 'ok' | 'warn' | 'crit' {
		if (pct < 60) return 'ok';
		if (pct < 85) return 'warn';
		return 'crit';
	}

	onMount(loadData);

	let hourlyPct = $derived(pctOfLimit(cbStatus?.hourly_outflow_usd, cbStatus?.hourly_limit_usd));
	let dailyPct = $derived(pctOfLimit(cbStatus?.daily_outflow_usd, cbStatus?.daily_limit_usd));
</script>

<!-- ═══ Head ═══ -->
<div class="page-head">
	<div>
		<h1>{$locale, t('policies.title')}</h1>
		<p class="page-sub">{$locale, t('policies.subtitle')}</p>
	</div>
	<button class="btn btn-primary" disabled>
		<span>{@html icon('plus', 14)}</span>
		<span>{$locale, t('policies.new_rule')}</span>
	</button>
</div>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
	</div>
{/if}

<!-- ═══ Circuit breaker ═══ -->
<section class="panel cb-panel">
	<header class="panel-head">
		<div>
			<h2>
				<span class="info-icon">{@html icon('shield', 14)}</span>
				{$locale, t('policies.cb_title')}
			</h2>
			<p class="panel-sub">{$locale, t('policies.cb_sub')}</p>
		</div>
		{#if cbStatus}
			{#if cbStatus.is_tripped}
				<span class="status-badge tripped"><span class="dot"></span>{$locale, t('policies.cb_tripped')}</span>
			{:else}
				<span class="status-badge normal"><span class="dot"></span>{$locale, t('policies.cb_normal')}</span>
			{/if}
		{:else}
			<span class="status-badge unknown"><span class="dot"></span>{$locale, t('policies.cb_unknown')}</span>
		{/if}
	</header>

	{#if cbStatus?.is_tripped}
		<div class="trip-banner">
			<span class="trip-icon">{@html icon('alert-triangle', 18)}</span>
			<div>
				<span class="trip-title">{$locale, t('policies.cb_halted')}</span>
				<span class="trip-reason">{cbStatus.trip_reason ?? 'Automated trip — requires admin override to resume.'}</span>
			</div>
		</div>
	{/if}

	{#if cbStatus}
		<div class="cb-grid">
			<div class="cb-tile">
				<span class="cb-label">{$locale, t('policies.cb_per_tx')}</span>
				<span class="cb-value mono">{fmtUsd(cbStatus.single_tx_limit_usd)}</span>
				<span class="cb-hint muted tiny">{$locale, t('policies.cb_per_tx_hint')}</span>
			</div>

			<div class="cb-tile">
				<div class="cb-row">
					<span class="cb-label">{$locale, t('policies.cb_hourly')}</span>
					<span class="cb-pct tone-{barTone(hourlyPct)}">{hourlyPct.toFixed(1)}%</span>
				</div>
				<div class="cb-meter">
					<div class="cb-fill tone-{barTone(hourlyPct)}" style="width:{hourlyPct}%"></div>
				</div>
				<span class="cb-usage mono tiny">
					{fmtUsd(cbStatus.hourly_outflow_usd)} / {fmtUsd(cbStatus.hourly_limit_usd)}
				</span>
			</div>

			<div class="cb-tile">
				<div class="cb-row">
					<span class="cb-label">{$locale, t('policies.cb_daily')}</span>
					<span class="cb-pct tone-{barTone(dailyPct)}">{dailyPct.toFixed(1)}%</span>
				</div>
				<div class="cb-meter">
					<div class="cb-fill tone-{barTone(dailyPct)}" style="width:{dailyPct}%"></div>
				</div>
				<span class="cb-usage mono tiny">
					{fmtUsd(cbStatus.daily_outflow_usd)} / {fmtUsd(cbStatus.daily_limit_usd)}
				</span>
			</div>
		</div>
	{:else if loading}
		<div class="panel-body">
			<div class="skel-line"></div>
		</div>
	{:else}
		<div class="panel-body">
			<p class="muted">{$locale, t('policies.cb_unavailable')}</p>
		</div>
	{/if}
</section>

<!-- ═══ Approval tiers ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{$locale, t('policies.tiers')}</h2>
			<p class="panel-sub">{$locale, t('policies.tiers_sub')}</p>
		</div>
	</header>

	<ul class="tier-list">
		{#each TIERS as t}
			<li class="tier-item tone-{t.tone}">
				<span class="tier-num mono">T{t.tier}</span>
				<div class="tier-main">
					<span class="tier-range">{t.range}</span>
					<span class="tier-approval muted">{t.approval}</span>
				</div>
				<div class="tier-delay">
					<span class="tiny muted">{$locale, t('policies.tier_delay')}</span>
					<span class="mono">{t.delay}</span>
				</div>
				<span class="tier-bar"></span>
			</li>
		{/each}
	</ul>
</section>

<!-- ═══ Custom rules ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{$locale, t('policies.custom')}</h2>
			<p class="panel-sub">{$locale, t('policies.custom_sub')}</p>
		</div>
		<span class="badge-count">{rules.length}</span>
	</header>

	{#if loading}
		<div class="panel-body">
			<div class="skel-line"></div>
			<div class="skel-line"></div>
		</div>
	{:else if rules.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('policies', 22)}</span>
			<div>
				<p class="empty-title">{$locale, t('policies.no_rules')}</p>
				<p class="muted">Default tier logic applies to all withdrawals. Add rules for asset-specific or destination-specific overrides.</p>
			</div>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th class="num">#</th>
					<th>{$locale, t('policies.col_name')}</th>
					<th>{$locale, t('policies.col_conditions')}</th>
					<th>{$locale, t('policies.col_action')}</th>
					<th>{$locale, t('policies.col_state')}</th>
				</tr>
			</thead>
			<tbody>
				{#each rules as rule}
					<tr class:disabled={!rule.enabled}>
						<td class="num mono">{rule.priority}</td>
						<td class="strong">{rule.name}</td>
						<td class="mono muted">{rule.conditions}</td>
						<td><span class="action-badge">{rule.action}</span></td>
						<td>
							{#if rule.enabled}
								<span class="chip tone-ok"><span class="chip-dot"></span>{$locale, t('policies.state_active')}</span>
							{:else}
								<span class="chip tone-muted">{$locale, t('policies.state_disabled')}</span>
							{/if}
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<style>
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

	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.danger { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.25); }

	/* Panel */
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
	.panel-head h2 {
		margin: 0 0 2px;
		font-size: 0.92rem;
		font-weight: 600;
		color: var(--fg-strong);
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }

	.info-icon { display: inline-flex; color: var(--accent); }

	.badge-count {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 3px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

	/* Buttons */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 13px;
		font-size: 0.84rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary { background: var(--accent); color: #0A0B0D; border-color: var(--accent); font-weight: 600; }
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); color: #0A0B0D; }

	/* Circuit breaker status */
	.status-badge {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 0.74rem;
		font-weight: 600;
		padding: 4px 11px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}
	.status-badge .dot {
		width: 6px; height: 6px;
		border-radius: 50%;
		background: currentColor;
	}
	.status-badge.normal {
		color: var(--green);
		background: var(--green-soft);
		border-color: rgba(52, 211, 153, 0.3);
	}
	.status-badge.normal .dot { animation: pulse-ok 2s infinite; }
	@keyframes pulse-ok {
		0% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0.5); }
		70% { box-shadow: 0 0 0 6px rgba(52, 211, 153, 0); }
		100% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0); }
	}
	.status-badge.tripped {
		color: var(--red);
		background: var(--red-soft);
		border-color: rgba(248, 113, 113, 0.3);
		animation: blink 1s infinite;
	}
	@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.65; } }
	.status-badge.unknown { color: var(--fg-muted); }

	.trip-banner {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		margin: var(--sp-4) var(--sp-5);
		padding: var(--sp-3) var(--sp-4);
		background: var(--red-soft);
		border: 1px solid rgba(248, 113, 113, 0.35);
		border-radius: var(--r-md);
		color: var(--red);
	}
	.trip-icon { display: inline-flex; flex-shrink: 0; margin-top: 1px; }
	.trip-banner > div { display: flex; flex-direction: column; gap: 2px; }
	.trip-title { font-weight: 600; font-size: 0.88rem; }
	.trip-reason { font-size: 0.82rem; opacity: 0.88; }

	.cb-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 1px;
		background: var(--border);
		border-top: 1px solid var(--border);
	}

	.cb-tile {
		padding: var(--sp-4) var(--sp-5);
		background: var(--surface);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.cb-row { display: flex; justify-content: space-between; align-items: center; }

	.cb-label {
		font-size: 0.72rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-muted);
	}

	.cb-value {
		font-size: 1.4rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
		line-height: 1;
	}

	.cb-pct {
		font-family: var(--font-mono);
		font-size: 0.86rem;
		font-weight: 650;
	}
	.cb-pct.tone-ok { color: var(--green); }
	.cb-pct.tone-warn { color: var(--yellow); }
	.cb-pct.tone-crit { color: var(--red); }

	.cb-meter {
		height: 6px;
		background: var(--surface-3);
		border-radius: 3px;
		overflow: hidden;
	}

	.cb-fill {
		height: 100%;
		border-radius: 3px;
		transition: width 0.3s;
	}
	.cb-fill.tone-ok { background: var(--green); }
	.cb-fill.tone-warn { background: var(--yellow); }
	.cb-fill.tone-crit { background: var(--red); }

	.cb-usage { color: var(--fg-muted); }

	.cb-hint { margin-top: auto; }
	.tiny { font-size: 0.7rem; }

	/* Tier list */
	.tier-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.tier-item {
		display: grid;
		grid-template-columns: 50px 1fr auto 4px;
		gap: var(--sp-4);
		align-items: center;
		padding: var(--sp-3) var(--sp-5);
		border-bottom: 1px solid var(--border);
		position: relative;
	}
	.tier-item:last-child { border-bottom: none; }
	.tier-item:hover { background: var(--hover-bg); }

	.tier-num {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		font-size: 0.85rem;
		font-weight: 700;
		color: var(--fg);
	}
	.tier-item.tone-ok .tier-num { border-color: rgba(52, 211, 153, 0.3); color: var(--green); }
	.tier-item.tone-warn .tier-num { border-color: rgba(245, 185, 66, 0.3); color: var(--yellow); }
	.tier-item.tone-crit .tier-num { border-color: rgba(248, 113, 113, 0.3); color: var(--red); }

	.tier-main { display: flex; flex-direction: column; gap: 2px; }
	.tier-range { font-size: 0.92rem; font-weight: 500; color: var(--fg-strong); }
	.tier-approval { font-size: 0.8rem; }

	.tier-delay { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; }

	.tier-bar {
		position: absolute;
		right: 0;
		top: 8px;
		bottom: 8px;
		width: 3px;
		border-radius: 3px 0 0 3px;
	}
	.tier-item.tone-ok .tier-bar { background: var(--green); }
	.tier-item.tone-warn .tier-bar { background: var(--yellow); }
	.tier-item.tone-crit .tier-bar { background: var(--red); }

	/* Data table (custom rules) */
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
	}
	.data-table tbody td { padding: var(--sp-3) var(--sp-5); border-bottom: 1px solid var(--border); }
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr:hover { background: var(--hover-bg); }
	.data-table th.num, .data-table td.num { text-align: right; }
	tr.disabled { opacity: 0.5; }

	.strong { color: var(--fg-strong); font-weight: 500; }
	.muted { color: var(--fg-muted); }

	.action-badge {
		display: inline-flex;
		padding: 2px 8px;
		font-size: 0.72rem;
		font-weight: 600;
		background: var(--surface-2);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		text-transform: capitalize;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		font-weight: 500;
		padding: 2px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}
	.chip.tone-ok { color: var(--green); background: var(--green-soft); border-color: rgba(52, 211, 153, 0.3); }
	.chip.tone-muted { color: var(--fg-muted); background: var(--surface-2); }
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	/* Empty & skel */
	.panel-empty { display: flex; align-items: center; gap: var(--sp-4); padding: var(--sp-6); }
	.empty-mark {
		display: inline-flex; align-items: center; justify-content: center;
		width: 44px; height: 44px;
		background: var(--accent-soft); color: var(--accent);
		border-radius: var(--r-md); flex-shrink: 0;
	}
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.skel-line {
		height: 24px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 6px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	@media (max-width: 900px) {
		.cb-grid { grid-template-columns: 1fr; }
		.tier-item { grid-template-columns: 40px 1fr 3px; }
		.tier-delay { grid-column: 2; flex-direction: row; justify-content: flex-start; gap: 8px; }
	}
</style>
