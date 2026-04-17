<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';
	import { t, locale } from '$lib/i18n';

	interface DetailedChainHealth {
		status: string;
		rpc_url: string;
		latest_block: number | null;
		scanner_block: number | null;
		scanner_lag: number | null;
		last_scan_at: string | null;
		avg_rpc_latency_ms: number | null;
		error_rate_pct: number;
		deposits_today: number;
		withdrawals_today: number;
	}

	interface SystemHealth {
		uptime_seconds: number;
		db_pool_size: number;
		db_pool_active: number;
		memory_mb: number;
		pending_webhooks: number;
		circuit_breaker: string;
	}

	interface DetailedHealth {
		chains: Record<string, DetailedChainHealth>;
		system: SystemHealth;
		generated_at: string;
	}

	let detailedHealth = $state<DetailedHealth | null>(null);
	let loading = $state(true);

	async function load() {
		loading = true;
		try {
			detailedHealth = await api.getJson<DetailedHealth>('/v1/health/detailed').catch(() => null);
		} finally {
			loading = false;
		}
	}

	onMount(load);

	function chainTone(s: string): 'ok' | 'warn' | 'err' | 'unknown' {
		if (s === 'ok' || s === 'healthy') return 'ok';
		if (s === 'degraded' || s === 'lagging') return 'warn';
		if (s === 'down') return 'err';
		return 'unknown';
	}

	function lagTone(lag: number | null): 'ok' | 'warn' | 'err' {
		if (lag === null) return 'ok';
		if (lag < 5) return 'ok';
		if (lag < 20) return 'warn';
		return 'err';
	}

	function fmtUptime(secs: number): string {
		const d = Math.floor(secs / 86400);
		const h = Math.floor((secs % 86400) / 3600);
		const m = Math.floor((secs % 3600) / 60);
		if (d > 0) return `${d}d ${h}h`;
		if (h > 0) return `${h}h ${m}m`;
		return `${m}m`;
	}

	function fmtLat(ms: number | null): string {
		return ms != null ? `${Math.round(ms)} ms` : '—';
	}
</script>

<!-- ═══ Head ═══ -->
<div class="page-head">
	<div>
		<h1>{$locale, t('settings.title')}</h1>
		<p class="page-sub">{$locale, t('settings.subtitle')}</p>
	</div>
	<button class="btn btn-ghost" onclick={load}>
		<span>{@html icon('refresh', 14)}</span>
		<span>Refresh</span>
	</button>
</div>

<!-- ═══ System runtime ═══ -->
{#if loading}
	<section class="panel">
		<div class="panel-body"><div class="skel-line"></div><div class="skel-line"></div></div>
	</section>
{:else if detailedHealth}
	{@const sys = detailedHealth.system}
	<section class="kpi-grid">
		<div class="kpi">
			<span class="kpi-label">{$locale, t('settings.kpi_uptime')}</span>
			<span class="kpi-value mono">{fmtUptime(sys.uptime_seconds)}</span>
		</div>
		<div class="kpi">
			<span class="kpi-label">{$locale, t('settings.kpi_dbpool')}</span>
			<span class="kpi-value mono">{sys.db_pool_active}<span class="of">/{sys.db_pool_size}</span></span>
			<span class="kpi-hint muted tiny">active / max</span>
		</div>
		<div class="kpi">
			<span class="kpi-label">{$locale, t('settings.kpi_memory')}</span>
			<span class="kpi-value mono">{sys.memory_mb > 0 ? `${sys.memory_mb}` : '—'}<span class="of">{sys.memory_mb > 0 ? ' MB' : ''}</span></span>
		</div>
		<div class="kpi">
			<span class="kpi-label">{$locale, t('settings.kpi_webhooks')}</span>
			<span class="kpi-value mono" class:tone-warn={sys.pending_webhooks > 0}>{sys.pending_webhooks}</span>
		</div>
		<div class="kpi">
			<span class="kpi-label">{$locale, t('settings.kpi_cb')}</span>
			<span class="kpi-value">
				{#if sys.circuit_breaker === 'normal'}
					<span class="chip tone-ok"><span class="chip-dot"></span>Operational</span>
				{:else}
					<span class="chip tone-err"><span class="chip-dot"></span>{sys.circuit_breaker}</span>
				{/if}
			</span>
		</div>
	</section>

	<!-- ═══ Chain infrastructure ═══ -->
	<section class="panel">
		<header class="panel-head">
			<div>
				<h2>{$locale, t('settings.chain_infra')}</h2>
				<p class="panel-sub">{$locale, t('settings.chain_infra_sub')}</p>
			</div>
			<span class="badge-count">{Object.keys(detailedHealth.chains).length}</span>
		</header>

		<div class="chain-grid">
			{#each Object.entries(detailedHealth.chains) as [name, ch]}
				<div class="chain-card">
					<div class="chain-card-head">
						<span class="chain-icon-wrap">{@html chainIcon(name, 20)}</span>
						<div class="chain-card-title">
							<span class="chain-name">{name.charAt(0).toUpperCase() + name.slice(1)}</span>
							<code class="chain-rpc mono tiny muted">{ch.rpc_url || '—'}</code>
						</div>
						<span class="chip tone-{chainTone(ch.status)}">
							<span class="chip-dot"></span>
							{ch.status}
						</span>
					</div>

					<div class="chain-metrics">
						<div>
							<span class="metric-label">{$locale, t('settings.metric_scanner_block')}</span>
							<span class="metric-val mono">{ch.scanner_block?.toLocaleString() ?? '—'}</span>
						</div>
						<div>
							<span class="metric-label">{$locale, t('settings.metric_lag')}</span>
							<span class="metric-val mono tone-{lagTone(ch.scanner_lag)}">
								{ch.scanner_lag != null ? `${ch.scanner_lag} blk` : '—'}
							</span>
						</div>
						<div>
							<span class="metric-label">{$locale, t('settings.metric_rpc_lat')}</span>
							<span class="metric-val mono">{fmtLat(ch.avg_rpc_latency_ms)}</span>
						</div>
						<div>
							<span class="metric-label">{$locale, t('settings.metric_err_rate')}</span>
							<span class="metric-val mono" class:tone-warn={ch.error_rate_pct > 1}>{ch.error_rate_pct.toFixed(2)}%</span>
						</div>
						<div>
							<span class="metric-label">{$locale, t('settings.metric_dep_24h')}</span>
							<span class="metric-val mono">{ch.deposits_today}</span>
						</div>
						<div>
							<span class="metric-label">{$locale, t('settings.metric_wd_24h')}</span>
							<span class="metric-val mono">{ch.withdrawals_today}</span>
						</div>
					</div>
				</div>
			{/each}
		</div>

		<div class="panel-foot">
			<span>{@html icon('file-text', 12)}</span>
			<span class="muted tiny">Override per-chain via <code class="mono">CUSTODY_CHAINS__*</code> environment variables.</span>
		</div>
	</section>

	<!-- ═══ Webhook events ═══ -->
	<section class="panel">
		<header class="panel-head">
			<div>
				<h2>{$locale, t('settings.webhook_events')}</h2>
				<p class="panel-sub">{$locale, t('settings.webhook_events_sub')}</p>
			</div>
			<a href="/webhooks" class="btn btn-ghost">
				<span>{$locale, t('settings.manage_endpoints')}</span>
				<span>{@html icon('chevron-right', 12)}</span>
			</a>
		</header>

		<ul class="event-list">
			<li>
				<span class="event-ico tone-ok">{@html icon('arrow-down-left', 14)}</span>
				<div>
					<code class="mono">deposit_confirmed</code>
					<span class="muted">User deposit reached required confirmations</span>
				</div>
			</li>
			<li>
				<span class="event-ico tone-info">{@html icon('radio', 14)}</span>
				<div>
					<code class="mono">withdrawal_broadcast</code>
					<span class="muted">Signed transaction submitted to the network</span>
				</div>
			</li>
			<li>
				<span class="event-ico tone-ok">{@html icon('check', 14)}</span>
				<div>
					<code class="mono">withdrawal_confirmed</code>
					<span class="muted">Transaction confirmed on-chain</span>
				</div>
			</li>
			<li>
				<span class="event-ico tone-err">{@html icon('alert-triangle', 14)}</span>
				<div>
					<code class="mono">withdrawal_failed</code>
					<span class="muted">Transaction failed after retry budget exhausted</span>
				</div>
			</li>
		</ul>

		<div class="panel-foot">
			<span>{@html icon('shield-check', 12)}</span>
			<span class="muted tiny">Signed with HMAC-SHA256. Timestamps &gt; 30s are rejected.</span>
		</div>
	</section>

	<p class="generated-at muted tiny">Generated: {new Date(detailedHealth.generated_at).toLocaleString()}</p>
{:else}
	<div class="panel">
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('alert-triangle', 22)}</span>
			<div>
				<p class="empty-title">{$locale, t('settings.unavailable')}</p>
				<p class="muted">{$locale, t('settings.unavailable_sub')}</p>
			</div>
		</div>
	</div>
{/if}

<style>
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-5);
	}
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	/* KPI grid */
	.kpi-grid {
		display: grid;
		grid-template-columns: repeat(5, 1fr);
		gap: 1px;
		background: var(--border);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		overflow: hidden;
		margin-bottom: var(--sp-4);
		box-shadow: var(--shadow-1);
	}
	.kpi {
		padding: var(--sp-4) var(--sp-5);
		background: var(--surface);
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.kpi-label {
		font-size: 0.68rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--fg-muted);
	}
	.kpi-value {
		font-size: 1.3rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
		line-height: 1;
	}
	.kpi-value.tone-warn { color: var(--yellow); }
	.of { color: var(--fg-muted); font-size: 0.8rem; font-weight: 500; }
	.kpi-hint { margin-top: 2px; }
	.tiny { font-size: 0.7rem; }

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		font-weight: 500;
		padding: 2px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
		text-transform: capitalize;
	}
	.chip.tone-ok { color: var(--green); background: var(--green-soft); border-color: rgba(52, 211, 153, 0.3); }
	.chip.tone-warn { color: var(--yellow); background: var(--yellow-soft); border-color: rgba(245, 185, 66, 0.3); }
	.chip.tone-err { color: var(--red); background: var(--red-soft); border-color: rgba(248, 113, 113, 0.3); }
	.chip.tone-unknown { color: var(--fg-muted); background: var(--surface-2); }
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

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
	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }
	.panel-foot {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: var(--sp-3) var(--sp-5);
		background: var(--surface-2);
		border-top: 1px solid var(--border);
	}
	.panel-foot > span:first-child { color: var(--accent); display: inline-flex; }
	.panel-foot code {
		background: var(--surface);
		padding: 1px 6px;
		border-radius: 4px;
		border: 1px solid var(--border);
		font-size: 0.7rem;
	}

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
		padding: 7px 12px;
		font-size: 0.84rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		text-decoration: none;
	}
	.btn:hover { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	/* Chain grid */
	.chain-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
		gap: 1px;
		background: var(--border);
	}

	.chain-card {
		background: var(--surface);
		padding: var(--sp-4) var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}

	.chain-card-head {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.chain-icon-wrap { display: inline-flex; flex-shrink: 0; }
	.chain-icon-wrap :global(svg) { border-radius: 50%; }

	.chain-card-title { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.chain-name { font-size: 0.9rem; font-weight: 600; color: var(--fg-strong); }
	.chain-rpc {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.chain-metrics {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: var(--sp-3);
		padding: var(--sp-3);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
	}
	.chain-metrics > div {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.metric-label {
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--fg-muted);
	}
	.metric-val {
		font-size: 0.86rem;
		color: var(--fg-strong);
		font-weight: 500;
	}
	.metric-val.tone-ok { color: var(--green); }
	.metric-val.tone-warn { color: var(--yellow); }
	.metric-val.tone-err { color: var(--red); }

	/* Event list */
	.event-list {
		list-style: none;
		margin: 0;
		padding: var(--sp-4) var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
	}
	.event-list li {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.event-ico {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px; height: 28px;
		border-radius: var(--r-sm);
		flex-shrink: 0;
	}
	.event-ico.tone-ok { background: var(--green-soft); color: var(--green); }
	.event-ico.tone-info { background: var(--accent-soft); color: var(--accent); }
	.event-ico.tone-err { background: var(--red-soft); color: var(--red); }
	.event-list li > div { flex: 1; display: flex; flex-direction: column; gap: 2px; }
	.event-list code {
		font-size: 0.82rem;
		color: var(--fg-strong);
		font-weight: 500;
	}
	.event-list span.muted { font-size: 0.78rem; color: var(--fg-muted); }

	/* Empty */
	.panel-empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-6);
	}
	.empty-mark {
		display: inline-flex; align-items: center; justify-content: center;
		width: 44px; height: 44px;
		background: var(--yellow-soft); color: var(--yellow);
		border-radius: var(--r-md); flex-shrink: 0;
	}
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.skel-line {
		height: 44px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 6px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	.muted { color: var(--fg-muted); }

	.generated-at { text-align: right; }

	@media (max-width: 1100px) {
		.kpi-grid { grid-template-columns: repeat(3, 1fr); }
	}
	@media (max-width: 700px) {
		.kpi-grid { grid-template-columns: repeat(2, 1fr); }
		.chain-metrics { grid-template-columns: 1fr; }
	}
</style>
