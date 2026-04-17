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

	let features = $state<Feature[]>([]);
	let summary = $state({ ready: 0, disabled: 0, misconfigured: 0, degraded: 0, total: 0 });
	let loading = $state(true);
	let error = $state('');
	let lastLoaded = $state<Date | null>(null);
	let autoRefresh = $state(true);
	let filter = $state<FeatureState | 'all' | 'problems'>('all');

	const REFRESH_MS = 15_000;
	let timer: ReturnType<typeof setInterval> | null = null;

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
				<span>Refresh now</span>
			</button>
		</div>
	</header>

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
</style>
