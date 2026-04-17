<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { api } from '$lib/api';
	import { icon } from '$lib/icons/ui';

	type FeatureState = 'ready' | 'disabled' | 'misconfigured' | 'degraded';

	interface Feature {
		name: string;
		summary: string;
		state: FeatureState;
		missing?: string[];
		reason?: string;
	}

	interface Transition {
		id: number;
		feature: string;
		kind: 'degradation' | 'misconfiguration' | 'recovery';
		reason: string | null;
		occurred_at: string;
	}

	interface UptimeEntry {
		feature: string;
		uptime_pct: string;
		down_seconds: number;
		transitions: number;
		slo_target_pct: string;
		slo_breach: boolean;
	}

	interface Silence {
		id: number;
		feature: string;
		starts_at: string;
		expires_at: string;
		reason: string;
		created_by: string;
		created_at: string;
	}

	let featureName = $derived($page.params.feature ?? '');
	let feature = $state<Feature | null>(null);
	let transitions = $state<Transition[]>([]);
	let uptime24h = $state<UptimeEntry | null>(null);
	let uptime7d = $state<UptimeEntry | null>(null);
	let uptime30d = $state<UptimeEntry | null>(null);
	let silenceHistory = $state<Silence[]>([]);
	let loading = $state(true);
	let error = $state('');

	async function load() {
		if (!featureName) return;
		loading = true;
		error = '';
		try {
			const [report, history, u24, u7, u30, silences] = await Promise.all([
				api.getJson<{ features: Feature[] }>('/v1/health/features'),
				api.getJson<{ transitions: Transition[] }>('/v1/health/features/history?limit=500'),
				api.getJson<{ features: UptimeEntry[] }>('/v1/health/features/uptime?window=24h'),
				api.getJson<{ features: UptimeEntry[] }>('/v1/health/features/uptime?window=7d'),
				api.getJson<{ features: UptimeEntry[] }>('/v1/health/features/uptime?window=30d'),
				api.getJson<{ silences: Silence[] }>('/v1/health/features/silences/history?limit=200'),
			]);
			feature = (report.features ?? []).find(f => f.name === featureName) ?? null;
			transitions = (history.transitions ?? []).filter(t => t.feature === featureName);
			uptime24h = (u24.features ?? []).find(u => u.feature === featureName) ?? null;
			uptime7d = (u7.features ?? []).find(u => u.feature === featureName) ?? null;
			uptime30d = (u30.features ?? []).find(u => u.feature === featureName) ?? null;
			silenceHistory = (silences.silences ?? []).filter(s => s.feature === featureName);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load feature detail';
		} finally {
			loading = false;
		}
	}

	onMount(load);

	function stateLabel(s: FeatureState): string {
		return { ready: 'READY', disabled: 'DISABLED', misconfigured: 'MISCONFIGURED', degraded: 'DEGRADED' }[s];
	}

	function reasonFor(f: Feature): string {
		if (f.state === 'ready') return 'ok';
		if (f.state === 'disabled') return 'not enabled';
		if (f.state === 'misconfigured') return `missing: ${(f.missing ?? []).join(', ')}`;
		if (f.state === 'degraded') return f.reason ?? 'degraded';
		return '';
	}

	function fmtOccurred(iso: string): string {
		const d = new Date(iso);
		const diff = (Date.now() - d.getTime()) / 1000;
		if (diff < 60) return 'just now';
		if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
		if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
		return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
	}

	function fmtAbsolute(iso: string): string {
		return new Date(iso).toLocaleString(undefined, {
			year: 'numeric', month: 'short', day: 'numeric',
			hour: '2-digit', minute: '2-digit', second: '2-digit',
		});
	}

	function kindLabel(k: Transition['kind']): string {
		return { degradation: 'DEGRADED', misconfiguration: 'MISCONFIG', recovery: 'RECOVERED' }[k];
	}

	function pctClass(u: UptimeEntry | null): string {
		if (!u) return '';
		return u.slo_breach ? 'uptime-breach' : 'uptime-ok';
	}
</script>

<div class="page">
	<a href="/readiness" class="back-link">← All features</a>
	<header class="page-head">
		<div>
			<h1 class="mono">{featureName}</h1>
			{#if feature}
				<div class="state-line">
					<span class="state-pill state-{feature.state}">{stateLabel(feature.state)}</span>
					<span class="summary">{feature.summary}</span>
				</div>
				<div class="reason">{reasonFor(feature)}</div>
			{/if}
		</div>
	</header>

	{#if error}
		<div class="banner banner-error">{error}</div>
	{/if}

	{#if loading}
		<div class="empty">Loading…</div>
	{:else if !feature}
		<div class="empty">Feature "{featureName}" is not in the current readiness report. It may have been removed in a recent deploy.</div>
	{:else}
		<section class="uptime-grid">
			{#each [
				{ label: '24 hours', u: uptime24h },
				{ label: '7 days',   u: uptime7d  },
				{ label: '30 days',  u: uptime30d },
			] as card}
				<div class="uptime-card">
					<div class="uptime-label">{card.label}</div>
					{#if card.u}
						<div class="uptime-value {pctClass(card.u)}">
							{parseFloat(card.u.uptime_pct).toFixed(4)}%
						</div>
						<div class="uptime-meta">
							SLO {parseFloat(card.u.slo_target_pct).toFixed(2)}%
							{#if card.u.slo_breach}<span class="breach-tag">BREACH</span>{/if}
						</div>
						<div class="uptime-meta">
							{card.u.down_seconds}s down · {card.u.transitions} transitions
						</div>
					{:else}
						<div class="uptime-value uptime-dim">—</div>
					{/if}
				</div>
			{/each}
		</section>

		<section class="history-section">
			<h2>Full transition history</h2>
			{#if transitions.length === 0}
				<div class="empty empty-small">No transitions recorded for this feature. It has been stable since service boot or the last retention cycle.</div>
			{:else}
				<ul class="history-list">
					{#each transitions as t (t.id)}
						<li class="history-row history-{t.kind}">
							<span class="history-kind kind-{t.kind}">{kindLabel(t.kind)}</span>
							<span class="history-reason" title={t.reason ?? ''}>{t.reason ?? '—'}</span>
							<span class="history-when mono" title={fmtAbsolute(t.occurred_at)}>{fmtOccurred(t.occurred_at)}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		{#if silenceHistory.length > 0}
			<section class="history-section">
				<h2>Silences on this feature</h2>
				<ul class="silences-list">
					{#each silenceHistory as s (s.id)}
						{@const active = new Date(s.expires_at).getTime() > Date.now()}
						<li class="silence-row" class:silence-past={!active}>
							<span class="silence-reason">{s.reason}</span>
							<span class="silence-window mono">
								{fmtAbsolute(s.starts_at)} → {fmtAbsolute(s.expires_at)}
							</span>
							<span class="silence-by">by {s.created_by}</span>
						</li>
					{/each}
				</ul>
			</section>
		{/if}
	{/if}
</div>

<style>
	.page { display: flex; flex-direction: column; gap: var(--sp-5); }

	.back-link {
		display: inline-block; color: var(--fg-muted);
		font-size: 0.84rem; margin-bottom: -8px;
	}
	.back-link:hover { color: var(--accent); }

	.page-head h1 { font-size: 1.3rem; margin: 0 0 var(--sp-2); letter-spacing: -0.015em; }
	.state-line { display: inline-flex; align-items: center; gap: 10px; margin-bottom: 4px; }
	.summary { color: var(--fg-muted); font-size: 0.9rem; }
	.reason {
		color: var(--fg); font-size: 0.86rem;
		font-family: var(--font-mono);
	}

	.state-pill {
		padding: 3px 9px; border-radius: var(--r-sm);
		font-size: 0.66rem; font-weight: 700;
		letter-spacing: 0.06em; text-transform: uppercase;
	}
	.state-ready        { background: var(--green-soft);  color: var(--green); }
	.state-disabled     { background: var(--surface-3);   color: var(--fg-muted); }
	.state-misconfigured{ background: var(--yellow-soft); color: var(--yellow); }
	.state-degraded     { background: var(--red-soft);    color: var(--red); }

	.banner { padding: 10px 14px; border-radius: var(--r-md); font-size: 0.88rem; }
	.banner-error {
		background: var(--red-soft); color: var(--red);
		border: 1px solid color-mix(in srgb, var(--red) 30%, transparent);
	}

	.empty {
		padding: var(--sp-8); text-align: center; color: var(--fg-muted);
		background: var(--surface); border: 1px dashed var(--border);
		border-radius: var(--r-md);
	}
	.empty-small { padding: var(--sp-4); font-size: 0.86rem; }

	.uptime-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: var(--sp-3);
	}
	.uptime-card {
		padding: var(--sp-4);
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
	}
	.uptime-label {
		font-size: 0.72rem; font-weight: 600;
		text-transform: uppercase; letter-spacing: 0.08em;
		color: var(--fg-dim);
	}
	.uptime-value {
		margin: 8px 0 6px;
		font-size: 1.8rem; font-weight: 650;
		font-family: var(--font-mono);
		color: var(--fg-strong);
	}
	.uptime-ok { color: var(--green); }
	.uptime-breach { color: var(--red); }
	.uptime-dim { color: var(--fg-dim); }
	.uptime-meta {
		font-size: 0.78rem; color: var(--fg-muted);
		font-family: var(--font-mono);
	}
	.breach-tag {
		display: inline-block;
		margin-left: 6px;
		padding: 1px 6px;
		background: var(--red-soft); color: var(--red);
		border-radius: var(--r-sm);
		font-size: 0.62rem; font-weight: 700; letter-spacing: 0.06em;
	}

	.history-section { display: flex; flex-direction: column; gap: var(--sp-3); }
	.history-section h2 { margin: 0; font-size: 1.05rem; letter-spacing: -0.015em; }

	.history-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
	.history-row {
		display: grid;
		grid-template-columns: 110px 1fr 160px;
		align-items: center; gap: var(--sp-3);
		padding: 8px 14px;
		background: var(--surface); border: 1px solid var(--border);
		border-radius: var(--r-md);
		font-size: 0.84rem;
	}
	.history-degradation       { border-left: 3px solid var(--red); }
	.history-misconfiguration  { border-left: 3px solid var(--yellow); }
	.history-recovery          { border-left: 3px solid var(--green); }

	.history-kind {
		padding: 2px 8px; border-radius: var(--r-sm);
		font-size: 0.64rem; font-weight: 700;
		letter-spacing: 0.06em; text-transform: uppercase;
		text-align: center;
	}
	.kind-degradation      { background: var(--red-soft);    color: var(--red); }
	.kind-misconfiguration { background: var(--yellow-soft); color: var(--yellow); }
	.kind-recovery         { background: var(--green-soft);  color: var(--green); }

	.history-reason { color: var(--fg-muted); }
	.history-when { color: var(--fg-muted); font-size: 0.78rem; text-align: right; }

	.silences-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
	.silence-row {
		display: grid;
		grid-template-columns: 1fr 2fr 140px;
		align-items: center; gap: var(--sp-3);
		padding: 8px 14px;
		background: var(--yellow-soft);
		border: 1px solid color-mix(in srgb, var(--yellow) 30%, transparent);
		border-radius: var(--r-md);
		font-size: 0.84rem;
	}
	.silence-past { background: var(--surface); border-color: var(--border); opacity: 0.75; }
	.silence-reason { color: var(--fg); }
	.silence-window { color: var(--fg-muted); font-size: 0.78rem; }
	.silence-by { color: var(--fg-dim); font-size: 0.78rem; text-align: right; }

	@media (max-width: 720px) {
		.history-row { grid-template-columns: auto 1fr; }
		.history-when { grid-column: 1 / -1; text-align: left; }
		.silence-row { grid-template-columns: 1fr; gap: 4px; }
		.silence-by { text-align: left; }
	}
</style>
