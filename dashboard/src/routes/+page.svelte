<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { t, locale } from '$lib/i18n';
	import { tokenIcon, chainIcon } from '$lib/icons/tokens';
	import { icon } from '$lib/icons/ui';

	// ─── Types ──────────────────────────────────────────────────
	interface AssetBalance {
		asset: string;
		chain?: string;
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

	interface ChainHealth {
		status: string;
		block_height?: number;
		latency_ms?: number;
		last_block_age_seconds?: number;
	}

	interface ChainsHealth {
		chains: Record<string, ChainHealth>;
	}

	// ─── State ──────────────────────────────────────────────────
	let portfolio = $state<Portfolio | null>(null);
	let activity  = $state<ActivityEvent[]>([]);
	let chains    = $state<Record<string, ChainHealth>>({});
	let lastSync  = $state<Date | null>(null);
	let loading   = $state(true);
	let error     = $state('');

	async function loadAll() {
		try {
			const [port, act, h] = await Promise.all([
				api.getJson<Portfolio>('/v1/user/portfolio').catch(() => null),
				api.getJson<{ events: ActivityEvent[] }>('/v1/user/activity').catch(() => ({ events: [] })),
				api.getJson<ChainsHealth>('/health/chains').catch(() => ({ chains: {} })),
			]);
			if (port) {
				port.balances = port.balances ?? port.assets ?? [];
				port.pending_approvals = port.pending_approvals ?? 0;
				portfolio = port;
			} else {
				portfolio = { total_usd: '0', balances: [], assets: [], pending_approvals: 0 };
			}
			activity = act.events ?? [];
			chains = h.chains ?? {};
			lastSync = new Date();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load data';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadAll();
		const interval = setInterval(loadAll, 30_000);
		return () => clearInterval(interval);
	});

	// ─── Derived ────────────────────────────────────────────────
	let totalUsd = $derived(Number(portfolio?.total_usd ?? '0'));
	let balances = $derived(portfolio?.balances ?? []);
	let chainEntries = $derived(Object.entries(chains));
	let chainsLive = $derived(chainEntries.filter(([, c]) => c.status === 'healthy' || c.status === 'ok').length);
	let chainsTotal = $derived(chainEntries.length || 4);

	// Mock 7-day AUM series for sparkline (would come from /v1/analytics/aum)
	const AUM_SERIES = [0, 0, 0, 0, 0, 0, 0];

	// ─── Formatters ─────────────────────────────────────────────
	function fmtUsd(n: number): string {
		if (!isFinite(n)) return '—';
		if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(2)}B`;
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
		if (n >= 1_000) return `${(n / 1_000).toFixed(2)}K`;
		return n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
	}

	function fmtUsdFull(n: number): string {
		if (!isFinite(n)) return '$0.00';
		return `$${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
	}

	function fmtRel(iso: string | Date | null): string {
		if (!iso) return '—';
		const d = iso instanceof Date ? iso : new Date(iso);
		const diffMs = Date.now() - d.getTime();
		const s = Math.floor(diffMs / 1000);
		if (s < 10) return 'just now';
		if (s < 60) return `${s}s ago`;
		const m = Math.floor(s / 60);
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
	}

	function statusTone(s: string): 'success' | 'pending' | 'danger' | 'muted' {
		switch (s) {
			case 'confirmed': return 'success';
			case 'broadcast':
			case 'pending':   return 'pending';
			case 'failed':    return 'danger';
			default:          return 'muted';
		}
	}

	function typeIcon(type: string): string {
		const tt = type.toLowerCase();
		if (tt.includes('deposit') || tt.includes('receive')) return 'arrow-down-left';
		if (tt.includes('withdraw') || tt.includes('send')) return 'arrow-up-right';
		if (tt.includes('approv')) return 'approvals';
		return 'activity';
	}

	function chainTone(s: string): string {
		if (s === 'healthy' || s === 'ok') return 'ok';
		if (s === 'degraded') return 'warn';
		if (s === 'down') return 'err';
		return 'unknown';
	}

	function chainToneLabel(s: string): string {
		if (s === 'healthy' || s === 'ok') return t('chain.online');
		if (s === 'degraded') return t('chain.degraded');
		if (s === 'down') return t('chain.offline');
		return t('chain.unknown');
	}

	// ─── Chart builders (inline SVG, no deps) ──────────────────
	// Allocation donut — returns array of {asset, pct, color, path}
	const COLORS = ['#6FA8FF', '#34D399', '#F5B942', '#F87171', '#A78BFA', '#60A5FA', '#FBB6CE', '#9CA3AF'];

	function donutSegments(): Array<{ asset: string; pct: number; color: string; d: string; usd: string }> {
		if (balances.length === 0) return [];
		const total = balances.reduce((a, b) => a + Number(b.usd_value || 0), 0);
		if (total <= 0) return [];
		const cx = 60, cy = 60, r = 45, rInner = 30;
		let angle = -Math.PI / 2;
		return balances.slice(0, 8).map((b, i) => {
			const pct = Number(b.usd_value) / total;
			const startAngle = angle;
			const endAngle = angle + pct * 2 * Math.PI;
			angle = endAngle;
			const large = pct > 0.5 ? 1 : 0;
			const x1 = cx + r * Math.cos(startAngle);
			const y1 = cy + r * Math.sin(startAngle);
			const x2 = cx + r * Math.cos(endAngle);
			const y2 = cy + r * Math.sin(endAngle);
			const x3 = cx + rInner * Math.cos(endAngle);
			const y3 = cy + rInner * Math.sin(endAngle);
			const x4 = cx + rInner * Math.cos(startAngle);
			const y4 = cy + rInner * Math.sin(startAngle);
			const d = [
				`M ${x1} ${y1}`,
				`A ${r} ${r} 0 ${large} 1 ${x2} ${y2}`,
				`L ${x3} ${y3}`,
				`A ${rInner} ${rInner} 0 ${large} 0 ${x4} ${y4}`,
				'Z',
			].join(' ');
			return {
				asset: b.asset,
				pct: pct * 100,
				color: COLORS[i % COLORS.length],
				d,
				usd: b.usd_value,
			};
		});
	}

	// Sparkline path from [y0, y1, ...]
	function sparkPath(values: number[], w = 80, h = 24): string {
		if (values.length < 2) return `M 0 ${h/2} L ${w} ${h/2}`;
		const min = Math.min(...values);
		const max = Math.max(...values);
		const range = max - min || 1;
		const step = w / (values.length - 1);
		return values.map((v, i) => {
			const x = i * step;
			const y = h - ((v - min) / range) * h;
			return `${i === 0 ? 'M' : 'L'} ${x.toFixed(1)} ${y.toFixed(1)}`;
		}).join(' ');
	}

	const CHAIN_LABELS: Record<string, string> = {
		bitcoin: 'Bitcoin',
		ethereum: 'Ethereum',
		solana: 'Solana',
		tron: 'Tron',
		arbitrum: 'Arbitrum',
		optimism: 'Optimism',
		polygon: 'Polygon',
		base: 'Base',
	};
</script>

<!-- ═════════════════════ Page header ═════════════════════ -->
<div class="page-head">
	<div class="title-row">
		<h1>{$locale, t('portfolio.title')}</h1>
		<span class="live-indicator" title="Auto-refresh every 30s">
			<span class="live-dot"></span>
			<span class="live-label">{$locale, t('portfolio.live')} · {$locale, t('portfolio.synced')} {fmtRel(lastSync)}</span>
		</span>
	</div>
	<div class="head-actions">
		<a class="btn btn-ghost" href="/transactions">
			<span>{@html icon('transactions', 14)}</span>
			<span>{$locale, t('portfolio.history')}</span>
		</a>
		<a class="btn btn-secondary" href="/withdraw">
			<span>{@html icon('arrow-up-right', 14)}</span>
			<span>Withdraw</span>
		</a>
		<a class="btn btn-primary" href="/deposit">
			<span>{@html icon('arrow-down-left', 14)}</span>
			<span>Deposit</span>
		</a>
	</div>
</div>

{#if error}
	<div class="alert alert-danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn btn-ghost sm" onclick={loadAll}>Retry</button>
	</div>
{/if}

<!-- ═════════════════════ KPI strip ═════════════════════ -->
<section class="kpi-strip">
	<div class="kpi-tile">
		<div class="kpi-head">
			<span class="kpi-label">{$locale, t('portfolio.total')}</span>
			<span class="kpi-change neutral">—</span>
		</div>
		<div class="kpi-value mono">
			<span class="ccy">$</span>{fmtUsd(totalUsd)}
		</div>
		<svg class="sparkline" viewBox="0 0 80 24" preserveAspectRatio="none" aria-hidden="true">
			<path d={sparkPath(AUM_SERIES)} fill="none" stroke="var(--accent)" stroke-width="1.5" />
		</svg>
	</div>

	<div class="kpi-tile">
		<div class="kpi-head">
			<span class="kpi-label">{$locale, t('portfolio.change_24h')}</span>
		</div>
		<div class="kpi-value mono neutral">
			<span class="ccy">$</span>0.00
		</div>
		<span class="kpi-foot muted">0.00% vs yesterday</span>
	</div>

	<div class="kpi-tile">
		<div class="kpi-head">
			<span class="kpi-label">{$locale, t('portfolio.assets_count')}</span>
		</div>
		<div class="kpi-value mono">{balances.length}</div>
		<span class="kpi-foot muted">across {Math.max(1, new Set(balances.map(b => b.chain ?? 'ethereum')).size)} chain{new Set(balances.map(b => b.chain)).size === 1 ? '' : 's'}</span>
	</div>

	<div class="kpi-tile">
		<div class="kpi-head">
			<span class="kpi-label">{$locale, t('portfolio.chains_online')}</span>
		</div>
		<div class="kpi-value mono">
			{chainsLive}<span class="of">/{chainsTotal}</span>
		</div>
		<span class="kpi-foot">
			{#if chainsLive === chainsTotal && chainsTotal > 0}
				<span class="chip-ok"><span class="dot"></span>{$locale, t('portfolio.all_operational')}</span>
			{:else if chainsLive === 0}
				<span class="chip-warn"><span class="dot"></span>{$locale, t('portfolio.awaiting_sync')}</span>
			{:else}
				<span class="chip-warn"><span class="dot"></span>{chainsTotal - chainsLive} offline</span>
			{/if}
		</span>
	</div>

	<div class="kpi-tile">
		<div class="kpi-head">
			<span class="kpi-label">{$locale, t('portfolio.pending_ops')}</span>
			{#if (portfolio?.pending_approvals ?? 0) > 0}
				<a href="/approvals" class="kpi-link">{$locale, t('portfolio.review')} {@html icon('chevron-right', 12)}</a>
			{/if}
		</div>
		<div class="kpi-value mono" class:warn={(portfolio?.pending_approvals ?? 0) > 0}>
			{portfolio?.pending_approvals ?? 0}
		</div>
		<span class="kpi-foot muted">{$locale, t('portfolio.awaiting_approval')}</span>
	</div>
</section>

<!-- ═════════════════════ Allocation + Chain health ═════════════════════ -->
<div class="two-col">
	<!-- Allocation -->
	<section class="panel">
		<header class="panel-head">
			<div>
				<h2>{$locale, t('portfolio.allocation')}</h2>
				<p class="panel-sub">{$locale, t('portfolio.allocation_sub')}</p>
			</div>
		</header>

		<div class="panel-body allocation-body">
			{#if balances.length === 0}
				<div class="donut-empty">
					<svg width="140" height="140" viewBox="0 0 120 120" aria-hidden="true">
						<circle cx="60" cy="60" r="45" fill="none" stroke="var(--border)" stroke-width="15" stroke-dasharray="4 4"/>
					</svg>
					<div class="empty-label">
						<span class="mono big">$0.00</span>
						<span class="muted">{$locale, t('portfolio.no_holdings')}</span>
					</div>
				</div>
			{:else}
				{@const segs = donutSegments()}
				<div class="donut-wrap">
					<svg width="140" height="140" viewBox="0 0 120 120" aria-hidden="true">
						{#each segs as s}
							<path d={s.d} fill={s.color} stroke="var(--surface)" stroke-width="1"/>
						{/each}
					</svg>
					<div class="donut-center">
						<span class="mono big">${fmtUsd(totalUsd)}</span>
						<span class="muted">{balances.length} asset{balances.length === 1 ? '' : 's'}</span>
					</div>
				</div>
				<ul class="legend">
					{#each segs as s}
						<li>
							<span class="legend-dot" style="background:{s.color}"></span>
							<span class="legend-asset">{s.asset}</span>
							<span class="legend-pct mono">{s.pct.toFixed(1)}%</span>
							<span class="legend-usd mono muted">${fmtUsd(Number(s.usd))}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</section>

	<!-- Chain health -->
	<section class="panel">
		<header class="panel-head">
			<div>
				<h2>{$locale, t('portfolio.infrastructure')}</h2>
				<p class="panel-sub">{$locale, t('portfolio.infrastructure_sub')}</p>
			</div>
			<button class="btn-icon-only" title="Refresh" onclick={loadAll}>{@html icon('refresh', 14)}</button>
		</header>

		<div class="chain-grid">
			{#if chainEntries.length === 0}
				<div class="chain-empty">No chains configured</div>
			{:else}
				{#each chainEntries as [name, ch]}
					<div class="chain-tile tone-{chainTone(ch.status)}">
						<div class="chain-tile-head">
							<span class="chain-icon">{@html chainIcon(name, 18)}</span>
							<span class="chain-name">{CHAIN_LABELS[name] ?? name}</span>
							<span class="chain-status">
								<span class="dot dot-{chainTone(ch.status)}"></span>
								{chainToneLabel(ch.status)}
							</span>
						</div>
						<div class="chain-tile-body">
							<div>
								<span class="chain-meta-label">Block</span>
								<span class="mono chain-meta-val">
									{ch.block_height ? ch.block_height.toLocaleString() : '—'}
								</span>
							</div>
							<div>
								<span class="chain-meta-label">Latency</span>
								<span class="mono chain-meta-val">
									{ch.latency_ms !== undefined ? `${ch.latency_ms}ms` : '—'}
								</span>
							</div>
						</div>
					</div>
				{/each}
			{/if}
		</div>
	</section>
</div>

<!-- ═════════════════════ Holdings ═════════════════════ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{$locale, t('portfolio.assets')}</h2>
			<p class="panel-sub">{$locale, t('portfolio.infrastructure_sub')}</p>
		</div>
		<span class="badge-count">{balances.length}</span>
	</header>

	{#if balances.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('coins', 22)}</span>
			<div>
				<p class="empty-title">{$locale, t('portfolio.no_balances')}</p>
				<p class="muted">{$locale, t('portfolio.no_balances_sub')}</p>
			</div>
			<a href="/deposit" class="btn btn-primary">
				<span>{@html icon('arrow-down-left', 14)}</span>
				<span>{$locale, t('portfolio.gen_address')}</span>
			</a>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>{$locale, t('portfolio.col_asset')}</th>
					<th class="num">{$locale, t('portfolio.col_balance')}</th>
					<th class="num">{$locale, t('portfolio.col_price')}</th>
					<th class="num">{$locale, t('portfolio.col_value')}</th>
					<th class="num">{$locale, t('portfolio.col_allocation')}</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each balances as row, i}
					<tr>
						<td>
							<div class="asset-cell">
								{@html tokenIcon(row.asset, 24)}
								<div>
									<span class="sym">{row.asset}</span>
									{#if row.chain}<span class="sub-muted">on {row.chain}</span>{/if}
								</div>
							</div>
						</td>
						<td class="num mono">{row.balance}</td>
						<td class="num mono muted">
							{#if Number(row.balance) > 0 && Number(row.usd_value) > 0}
								${(Number(row.usd_value) / Number(row.balance)).toFixed(2)}
							{:else}—{/if}
						</td>
						<td class="num mono strong">{fmtUsdFull(Number(row.usd_value))}</td>
						<td class="num">
							<div class="pct-row">
								<div class="pct-bar">
									<div class="pct-fill" style="width:{row.pct}%; background: {COLORS[i % COLORS.length]}"></div>
								</div>
								<span class="mono pct-num">{Number(row.pct).toFixed(1)}%</span>
							</div>
						</td>
						<td>
							<button class="btn-icon-only" aria-label="Actions">{@html icon('more', 14)}</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<!-- ═════════════════════ Activity ═════════════════════ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{$locale, t('portfolio.activity')}</h2>
			<p class="panel-sub">{$locale, t('portfolio.activity_sub')}</p>
		</div>
		<a href="/transactions" class="panel-action">{$locale, t('portfolio.view_all')} <span>{@html icon('chevron-right', 12)}</span></a>
	</header>

	{#if activity.length === 0}
		<div class="panel-empty small">
			<span class="empty-mark muted">{@html icon('clock', 18)}</span>
			<p class="muted">{$locale, t('portfolio.no_activity')}</p>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>{$locale, t('portfolio.col_type')}</th>
					<th>{$locale, t('portfolio.col_asset')}</th>
					<th class="num">{$locale, t('portfolio.col_amount')}</th>
					<th>{$locale, t('portfolio.col_status')}</th>
					<th class="num">{$locale, t('portfolio.col_time')}</th>
				</tr>
			</thead>
			<tbody>
				{#each activity.slice(0, 12) as ev}
					<tr>
						<td>
							<div class="act-type-cell">
								<span class="act-ico {statusTone(ev.status)}">{@html icon(typeIcon(ev.type), 12)}</span>
								<span class="capitalize">{ev.type}</span>
							</div>
						</td>
						<td>
							<div class="token-cell">
								{@html tokenIcon(ev.asset, 16)}
								<span>{ev.asset}</span>
							</div>
						</td>
						<td class="num mono">{ev.amount}</td>
						<td><span class="status-pill {statusTone(ev.status)}">{ev.status}</span></td>
						<td class="num mono muted">{fmtRel(ev.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<style>
	/* ═════════════════════ Page head ═════════════════════ */
	.page-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-5);
		flex-wrap: wrap;
	}

	.title-row { display: flex; align-items: center; gap: var(--sp-4); flex-wrap: wrap; }

	h1 {
		margin: 0;
		font-size: 1.5rem;
		font-weight: 650;
		letter-spacing: -0.022em;
	}

	.live-indicator {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 0.75rem;
		color: var(--fg-muted);
		padding: 4px 10px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
	}

	.live-dot {
		width: 6px; height: 6px;
		border-radius: 50%;
		background: var(--green);
		box-shadow: 0 0 0 0 rgba(52, 211, 153, 0.4);
		animation: pulse 2s infinite;
	}

	@keyframes pulse {
		0% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0.4); }
		70% { box-shadow: 0 0 0 8px rgba(52, 211, 153, 0); }
		100% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0); }
	}

	.head-actions { display: flex; gap: var(--sp-2); }

	/* ═════════════════════ Buttons ═════════════════════ */
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
		transition: background 0.15s, border-color 0.15s;
	}
	.btn.sm { padding: 4px 8px; font-size: 0.76rem; }
	.btn:hover { background: var(--hover-bg); border-color: var(--border-strong); color: var(--fg-strong); }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }
	.btn-secondary { background: var(--surface-2); }
	.btn-primary {
		background: var(--accent); color: #0A0B0D; border-color: var(--accent); font-weight: 600;
	}
	.btn-primary:hover { background: var(--accent-strong); border-color: var(--accent-strong); color: #0A0B0D; }

	.btn-icon-only {
		display: inline-flex; align-items: center; justify-content: center;
		width: 28px; height: 28px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-dim);
		cursor: pointer;
		transition: background 0.15s, color 0.15s, border-color 0.15s;
	}
	.btn-icon-only:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	/* ═════════════════════ Alert ═════════════════════ */
	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert-danger {
		background: var(--red-soft);
		color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.25);
	}
	.alert .btn { margin-left: auto; }

	/* ═════════════════════ KPI strip ═════════════════════ */
	.kpi-strip {
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

	.kpi-tile {
		padding: var(--sp-4) var(--sp-5);
		background: var(--surface);
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-height: 98px;
		position: relative;
	}

	.kpi-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.kpi-label {
		font-size: 0.68rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--fg-muted);
	}

	.kpi-change {
		font-size: 0.72rem;
		font-weight: 600;
		padding: 2px 7px;
		border-radius: var(--r-full);
	}
	.kpi-change.neutral { background: var(--surface-2); color: var(--fg-muted); }

	.kpi-value {
		font-size: 1.35rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
		line-height: 1.1;
		display: flex;
		align-items: baseline;
		gap: 2px;
	}
	.kpi-value.neutral { color: var(--fg-muted); }
	.kpi-value.warn { color: var(--yellow); }

	.ccy { color: var(--fg-muted); font-size: 0.9rem; font-weight: 500; }
	.of  { color: var(--fg-muted); font-size: 0.85rem; font-weight: 500; margin-left: 2px; }

	.kpi-foot {
		font-size: 0.72rem;
		color: var(--fg-muted);
	}
	.muted { color: var(--fg-muted); }

	.sparkline {
		width: 100%;
		height: 24px;
		margin-top: auto;
	}

	.kpi-link {
		font-size: 0.72rem;
		color: var(--accent);
		display: inline-flex;
		align-items: center;
		gap: 1px;
	}

	.chip-ok, .chip-warn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		font-weight: 500;
	}
	.chip-ok { color: var(--green); }
	.chip-warn { color: var(--yellow); }
	.chip-ok .dot, .chip-warn .dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	/* ═════════════════════ Panels ═════════════════════ */
	.panel {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		margin-bottom: var(--sp-4);
		overflow: hidden;
		box-shadow: var(--shadow-1);
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
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--fg-strong);
	}

	.panel-sub {
		margin: 0;
		font-size: 0.78rem;
		color: var(--fg-muted);
	}

	.panel-body { padding: var(--sp-5); }

	.panel-action {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		font-size: 0.78rem;
		color: var(--fg-muted);
		font-weight: 500;
	}
	.panel-action:hover { color: var(--accent); }

	.badge-count {
		font-size: 0.7rem;
		font-weight: 600;
		color: var(--fg-muted);
		background: var(--surface-2);
		padding: 3px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}

	/* ═════════════════════ Two-col ═════════════════════ */
	.two-col {
		display: grid;
		grid-template-columns: 1fr 1.2fr;
		gap: var(--sp-4);
		margin-bottom: var(--sp-4);
	}

	/* ═════════════════════ Allocation ═════════════════════ */
	.allocation-body {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: var(--sp-6);
		align-items: center;
	}

	.donut-wrap, .donut-empty {
		position: relative;
		width: 140px;
		height: 140px;
		flex-shrink: 0;
	}

	.donut-center, .empty-label {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
	}

	.donut-center .big, .empty-label .big {
		font-size: 1rem;
		font-weight: 600;
		color: var(--fg-strong);
	}

	.legend {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 0.82rem;
		max-height: 180px;
		overflow-y: auto;
	}

	.legend li {
		display: grid;
		grid-template-columns: 10px 1fr auto auto;
		gap: 10px;
		align-items: center;
		padding: 4px 0;
	}

	.legend-dot { width: 10px; height: 10px; border-radius: 2px; display: inline-block; }
	.legend-asset { color: var(--fg); font-weight: 500; }
	.legend-pct { color: var(--fg); font-weight: 500; }
	.legend-usd { font-size: 0.78rem; }

	/* ═════════════════════ Chain grid ═════════════════════ */
	.chain-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 1px;
		background: var(--border);
	}

	.chain-empty {
		grid-column: span 2;
		padding: var(--sp-6);
		text-align: center;
		color: var(--fg-muted);
		background: var(--surface);
	}

	.chain-tile {
		background: var(--surface);
		padding: var(--sp-4) var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-3);
		transition: background 0.15s;
	}
	.chain-tile:hover { background: var(--hover-bg); }

	.chain-tile-head {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.chain-icon { display: inline-flex; flex-shrink: 0; }
	.chain-icon :global(svg) { border-radius: 50%; }

	.chain-name {
		flex: 1;
		font-size: 0.86rem;
		font-weight: 600;
		color: var(--fg-strong);
	}

	.chain-status {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		color: var(--fg-muted);
	}

	.dot {
		width: 6px; height: 6px; border-radius: 50%;
		background: var(--fg-dim);
	}
	.dot-ok { background: var(--green); box-shadow: 0 0 0 2px rgba(52, 211, 153, 0.18); }
	.dot-warn { background: var(--yellow); box-shadow: 0 0 0 2px rgba(245, 185, 66, 0.18); }
	.dot-err { background: var(--red); box-shadow: 0 0 0 2px rgba(248, 113, 113, 0.18); }
	.dot-unknown { background: var(--fg-dim); }

	.chain-tile-body {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--sp-3);
		font-size: 0.78rem;
	}

	.chain-tile-body > div {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.chain-meta-label {
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-dim);
	}

	.chain-meta-val {
		font-size: 0.85rem;
		color: var(--fg);
		font-weight: 500;
	}

	/* ═════════════════════ Data tables ═════════════════════ */
	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}

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

	.data-table tbody td {
		padding: var(--sp-3) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}

	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr:hover { background: var(--hover-bg); }

	.data-table th.num, .data-table td.num {
		text-align: right;
	}

	.data-table td.strong { color: var(--fg-strong); font-weight: 500; }

	.asset-cell { display: flex; align-items: center; gap: 10px; }
	.asset-cell :global(svg) { border-radius: 50%; }
	.asset-cell > div { display: flex; flex-direction: column; gap: 1px; line-height: 1.2; }
	.sym { font-weight: 600; color: var(--fg-strong); }
	.sub-muted { font-size: 0.72rem; color: var(--fg-muted); }

	.pct-row {
		display: flex;
		align-items: center;
		gap: 8px;
		justify-content: flex-end;
	}

	.pct-bar {
		width: 90px;
		height: 5px;
		background: var(--surface-3);
		border-radius: 999px;
		overflow: hidden;
	}

	.pct-fill {
		height: 100%;
		border-radius: 999px;
	}

	.pct-num { font-size: 0.78rem; min-width: 42px; text-align: right; color: var(--fg-muted); }

	.act-type-cell { display: flex; align-items: center; gap: 8px; }
	.capitalize { text-transform: capitalize; font-weight: 500; color: var(--fg-strong); }

	.act-ico {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 22px; height: 22px;
		border-radius: var(--r-sm);
		flex-shrink: 0;
	}
	.act-ico.success { background: var(--green-soft); color: var(--green); }
	.act-ico.pending { background: var(--yellow-soft); color: var(--yellow); }
	.act-ico.danger  { background: var(--red-soft); color: var(--red); }
	.act-ico.muted   { background: var(--surface-2); color: var(--fg-muted); }

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

	/* ═════════════════════ Panel empty ═════════════════════ */
	.panel-empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-6) var(--sp-5);
	}
	.panel-empty.small { padding: var(--sp-4) var(--sp-5); }

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
	.empty-mark.muted { background: var(--surface-2); color: var(--fg-muted); }

	.panel-empty > div {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.empty-title {
		margin: 0;
		font-size: 0.92rem;
		font-weight: 600;
		color: var(--fg-strong);
	}

	.panel-empty p { margin: 0; font-size: 0.82rem; }

	/* ═════════════════════ Responsive ═════════════════════ */
	@media (max-width: 1280px) {
		.kpi-strip { grid-template-columns: repeat(3, 1fr); }
	}

	@media (max-width: 900px) {
		.two-col { grid-template-columns: 1fr; }
		.kpi-strip { grid-template-columns: repeat(2, 1fr); }
		.allocation-body { grid-template-columns: 1fr; text-align: center; }
		.donut-wrap { margin: 0 auto; }
	}

	@media (max-width: 600px) {
		.kpi-strip { grid-template-columns: 1fr; }
		.chain-grid { grid-template-columns: 1fr; }
		.data-table { font-size: 0.78rem; }
		.data-table thead th, .data-table tbody td { padding: var(--sp-2) var(--sp-3); }
		.panel-empty { flex-direction: column; text-align: center; }
	}
</style>
