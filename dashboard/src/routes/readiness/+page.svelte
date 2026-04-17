<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
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

	interface Report {
		summary: { ready: number; disabled: number; misconfigured: number; degraded: number; total: number };
		features: Feature[];
	}

	interface Transition {
		id: number;
		feature: string;
		kind: 'degradation' | 'misconfiguration' | 'recovery';
		reason: string | null;
		occurred_at: string;
	}

	let features = $state<Feature[]>([]);
	let summary = $state({ ready: 0, disabled: 0, misconfigured: 0, degraded: 0, total: 0 });
	let history = $state<Transition[]>([]);
	let historyErr = $state('');
	let loading = $state(true);
	let error = $state('');
	let lastLoaded = $state<Date | null>(null);
	let autoRefresh = $state(true);
	let filter = $state<FeatureState | 'all' | 'problems'>('all');

	const REFRESH_MS = 15_000;
	let timer: ReturnType<typeof setInterval> | null = null;
	let probing = $state(false);
	let probeNote = $state('');

	async function load() {
		try {
			const data = await api.getJson<Report>('/v1/health/features');
			features = data.features ?? [];
			summary = data.summary;
			error = '';
			lastLoaded = new Date();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load readiness report';
		} finally {
			loading = false;
		}
		loadHistory();
	}

	async function loadHistory() {
		try {
			const data = await api.getJson<{ transitions: Transition[] }>(
				'/v1/health/features/history?limit=50'
			);
			history = data.transitions ?? [];
			historyErr = '';
		} catch (e) {
			historyErr = e instanceof Error ? e.message : 'Failed to load history';
		}
	}

	// Ask the server to re-run every cold-start probe, then poll the
	// report a couple of times so the user sees the new state without
	// having to click Refresh a second time. Probes are capped at 5s
	// each in the server, so 3s-then-6s gets us past nearly every
	// realistic refresh. Admin-only on the backend; non-admins get 403.
	async function requestProbe() {
		probing = true;
		probeNote = '';
		try {
			await api.post('/v1/health/features/refresh', {});
			probeNote = 'Probes re-running — report will update momentarily.';
			setTimeout(load, 3_000);
			setTimeout(() => { load(); probeNote = ''; }, 7_000);
		} catch (e) {
			probeNote = e instanceof Error ? e.message : 'Refresh request failed';
		} finally {
			probing = false;
		}
	}

	onMount(() => {
		load();
		if (autoRefresh) timer = setInterval(load, REFRESH_MS);
	});

	onDestroy(() => { if (timer) clearInterval(timer); });

	$effect(() => {
		if (timer) { clearInterval(timer); timer = null; }
		if (autoRefresh) timer = setInterval(load, REFRESH_MS);
	});

	let filtered = $derived(filter === 'all'
		? features
		: filter === 'problems'
			? features.filter(f => f.state === 'misconfigured' || f.state === 'degraded')
			: features.filter(f => f.state === filter));

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

	function fmtTime(d: Date | null): string {
		if (!d) return '—';
		return d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit', second: '2-digit' });
	}

	function fmtOccurred(iso: string): string {
		const d = new Date(iso);
		const diff = (Date.now() - d.getTime()) / 1000;
		if (diff < 60) return 'just now';
		if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
		if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
		return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
	}

	function kindLabel(k: Transition['kind']): string {
		return { degradation: 'DEGRADED', misconfiguration: 'MISCONFIG', recovery: 'RECOVERED' }[k];
	}
</script>

<div class="page">
	<header class="page-head">
		<div>
			<h1>Feature readiness</h1>
			<p class="sub">Live state of every subsystem. Degraded or misconfigured entries mean the feature is wired but can't actually serve requests — the endpoint will return 503 until it's fixed.</p>
		</div>
		<div class="head-actions">
			<label class="toggle">
				<input type="checkbox" bind:checked={autoRefresh} />
				<span>Auto-refresh</span>
			</label>
			<button class="btn" onclick={load} disabled={loading}>
				<span class="btn-icon">{@html icon('refresh', 14)}</span>
				<span>Reload report</span>
			</button>
			<button class="btn btn-primary" onclick={requestProbe} disabled={probing}>
				<span class="btn-icon">{@html icon('zap', 14)}</span>
				<span>{probing ? 'Probing…' : 'Re-probe now'}</span>
			</button>
		</div>
	</header>

	{#if probeNote}
		<div class="banner banner-info">{probeNote}</div>
	{/if}
	{#if error}
		<div class="banner banner-error">{error}</div>
	{/if}

	<section class="summary-row">
		<button class="chip chip-total" class:active={filter === 'all'} onclick={() => filter = 'all'}>
			<span class="chip-n">{summary.total}</span>
			<span class="chip-lbl">Total</span>
		</button>
		<button class="chip chip-ready" class:active={filter === 'ready'} onclick={() => filter = 'ready'}>
			<span class="chip-n">{summary.ready}</span>
			<span class="chip-lbl">Ready</span>
		</button>
		<button class="chip chip-disabled" class:active={filter === 'disabled'} onclick={() => filter = 'disabled'}>
			<span class="chip-n">{summary.disabled}</span>
			<span class="chip-lbl">Disabled</span>
		</button>
		<button class="chip chip-misc" class:active={filter === 'misconfigured'} onclick={() => filter = 'misconfigured'}>
			<span class="chip-n">{summary.misconfigured}</span>
			<span class="chip-lbl">Misconfigured</span>
		</button>
		<button class="chip chip-degraded" class:active={filter === 'degraded'} onclick={() => filter = 'degraded'}>
			<span class="chip-n">{summary.degraded}</span>
			<span class="chip-lbl">Degraded</span>
		</button>
		{#if summary.degraded + summary.misconfigured > 0}
			<button class="chip chip-problems" class:active={filter === 'problems'} onclick={() => filter = 'problems'}>
				<span class="chip-n">{summary.degraded + summary.misconfigured}</span>
				<span class="chip-lbl">Problems only</span>
			</button>
		{/if}
		<span class="last-loaded">Last: {fmtTime(lastLoaded)}</span>
	</section>

	{#if loading && features.length === 0}
		<div class="empty">Loading readiness report…</div>
	{:else if filtered.length === 0}
		<div class="empty">No features match the current filter.</div>
	{:else}
		<div class="feature-list">
			{#each filtered as f (f.name)}
				<article class="feature feature-{f.state}">
					<div class="feature-left">
						<span class="state-pill state-{f.state}">{stateLabel(f.state)}</span>
						<div class="feature-info">
							<div class="feature-name">{f.name}</div>
							<div class="feature-summary">{f.summary}</div>
						</div>
					</div>
					<div class="feature-reason" title={reasonFor(f)}>{reasonFor(f)}</div>
				</article>
			{/each}
		</div>
	{/if}

	<section class="history-section">
		<header class="history-head">
			<h2>Recent incidents</h2>
			<span class="history-hint">Last 50 transitions — degradations, misconfigurations, recoveries.</span>
		</header>
		{#if historyErr}
			<div class="banner banner-error">{historyErr}</div>
		{:else if history.length === 0}
			<div class="empty empty-small">No transitions recorded yet. Incidents will appear here as features change state.</div>
		{:else}
			<ul class="history-list">
				{#each history as t (t.id)}
					<li class="history-row history-{t.kind}">
						<span class="history-kind kind-{t.kind}">{kindLabel(t.kind)}</span>
						<span class="history-feature mono">{t.feature}</span>
						<span class="history-reason" title={t.reason ?? ''}>{t.reason ?? '—'}</span>
						<span class="history-when">{fmtOccurred(t.occurred_at)}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

<style>
	.page { display: flex; flex-direction: column; gap: var(--sp-5); }

	.page-head {
		display: flex; align-items: flex-start; justify-content: space-between;
		gap: var(--sp-4); flex-wrap: wrap;
	}
	.page-head h1 { font-size: 1.5rem; margin: 0 0 var(--sp-2); letter-spacing: -0.02em; }
	.sub { color: var(--fg-muted); max-width: 680px; font-size: 0.88rem; margin: 0; }

	.head-actions { display: flex; align-items: center; gap: var(--sp-3); }

	.toggle { display: inline-flex; align-items: center; gap: 6px; color: var(--fg-muted); font-size: 0.84rem; cursor: pointer; }
	.toggle input { accent-color: var(--accent); }

	.btn {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 6px 12px; font-size: 0.84rem;
		background: var(--surface-2); color: var(--fg);
		border: 1px solid var(--border); border-radius: var(--r-md);
		cursor: pointer; transition: background 0.15s, border-color 0.15s;
	}
	.btn:hover { background: var(--surface-3); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.6; cursor: not-allowed; }
	.btn-icon { display: inline-flex; color: var(--fg-muted); }

	.banner {
		padding: 10px 14px; border-radius: var(--r-md); font-size: 0.88rem;
	}
	.banner-error {
		background: var(--red-soft); color: var(--red);
		border: 1px solid color-mix(in srgb, var(--red) 30%, transparent);
	}
	.banner-info {
		background: var(--accent-soft); color: var(--accent);
		border: 1px solid var(--accent-line);
	}

	.btn-primary {
		background: var(--accent); color: #0A0B0D;
		border-color: var(--accent);
	}
	.btn-primary:hover { background: var(--accent-strong); border-color: var(--accent-strong); }
	.btn-primary .btn-icon { color: #0A0B0D; }

	.summary-row {
		display: flex; flex-wrap: wrap; gap: 8px; align-items: center;
	}
	.chip {
		display: inline-flex; align-items: baseline; gap: 6px;
		padding: 7px 12px;
		background: var(--surface-2); border: 1px solid var(--border);
		border-radius: var(--r-full);
		color: var(--fg); font-size: 0.82rem;
		cursor: pointer; transition: background 0.15s, border-color 0.15s;
	}
	.chip:hover { background: var(--surface-3); }
	.chip.active { border-color: var(--accent); background: var(--accent-soft); }
	.chip-n { font-weight: 700; color: var(--fg-strong); font-variant-numeric: tabular-nums; }
	.chip-lbl { color: var(--fg-muted); font-size: 0.78rem; }

	.chip-ready .chip-n    { color: var(--green); }
	.chip-disabled .chip-n { color: var(--fg-muted); }
	.chip-misc .chip-n     { color: var(--yellow); }
	.chip-degraded .chip-n { color: var(--red); }
	.chip-problems { border-color: color-mix(in srgb, var(--red) 40%, transparent); }
	.chip-problems .chip-n { color: var(--red); }

	.last-loaded {
		margin-left: auto;
		font-size: 0.76rem; color: var(--fg-dim);
		font-family: var(--font-mono);
	}

	.empty {
		padding: var(--sp-8); text-align: center; color: var(--fg-muted);
		background: var(--surface); border: 1px dashed var(--border);
		border-radius: var(--r-md);
	}

	.feature-list {
		display: flex; flex-direction: column; gap: 6px;
	}

	.feature {
		display: grid;
		grid-template-columns: minmax(280px, 1fr) minmax(200px, 2fr);
		align-items: center; gap: var(--sp-4);
		padding: 12px 16px;
		background: var(--surface); border: 1px solid var(--border);
		border-radius: var(--r-md);
		transition: border-color 0.15s, background 0.15s;
	}
	.feature:hover { border-color: var(--border-strong); }

	.feature-degraded      { border-left: 3px solid var(--red); }
	.feature-misconfigured { border-left: 3px solid var(--yellow); }

	.feature-left { display: flex; align-items: center; gap: var(--sp-3); min-width: 0; }

	.state-pill {
		padding: 3px 9px; border-radius: var(--r-sm);
		font-size: 0.66rem; font-weight: 700;
		letter-spacing: 0.06em; text-transform: uppercase;
		font-variant-numeric: tabular-nums;
		flex-shrink: 0;
	}
	.state-ready        { background: var(--green-soft);  color: var(--green); }
	.state-disabled     { background: var(--surface-3);   color: var(--fg-muted); }
	.state-misconfigured{ background: var(--yellow-soft); color: var(--yellow); }
	.state-degraded     { background: var(--red-soft);    color: var(--red); }

	.feature-info { min-width: 0; }
	.feature-name {
		font-family: var(--font-mono); font-size: 0.86rem;
		color: var(--fg-strong); font-weight: 600;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}
	.feature-summary {
		color: var(--fg-muted); font-size: 0.8rem;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.feature-reason {
		color: var(--fg); font-size: 0.82rem;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
		font-family: var(--font-mono);
	}

	@media (max-width: 720px) {
		.feature { grid-template-columns: 1fr; gap: 6px; }
		.feature-reason { white-space: normal; }
	}

	.history-section {
		margin-top: var(--sp-8);
		display: flex; flex-direction: column; gap: var(--sp-3);
	}
	.history-head { display: flex; align-items: baseline; justify-content: space-between; flex-wrap: wrap; gap: 8px; }
	.history-head h2 { margin: 0; font-size: 1.1rem; letter-spacing: -0.015em; }
	.history-hint { color: var(--fg-muted); font-size: 0.8rem; }

	.empty-small { padding: var(--sp-4); font-size: 0.86rem; }

	.history-list {
		list-style: none; padding: 0; margin: 0;
		display: flex; flex-direction: column; gap: 4px;
	}
	.history-row {
		display: grid;
		grid-template-columns: 110px minmax(180px, 220px) 1fr 90px;
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

	.history-feature {
		color: var(--fg-strong); font-weight: 600;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}
	.history-reason {
		color: var(--fg-muted);
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}
	.history-when {
		color: var(--fg-muted); font-size: 0.78rem;
		text-align: right; font-family: var(--font-mono);
	}

	@media (max-width: 720px) {
		.history-row { grid-template-columns: auto 1fr; gap: 6px; }
		.history-reason, .history-when { grid-column: 1 / -1; text-align: left; }
	}
</style>
