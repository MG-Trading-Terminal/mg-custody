<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { icon } from '$lib/icons/ui';
	import { t, locale } from '$lib/i18n';

	interface AuditEntry {
		id: string;
		actor: string;
		action: string;
		resource: string;
		result: string;
		created_at: string;
	}

	interface IntegrityResult {
		ok: boolean;
		checked: number;
		first_broken_id?: string;
		message: string;
	}

	const OPERATIONS = [
		'all','key_derive','sign','address_create','withdrawal_approve',
		'withdrawal_reject','login','logout','policy_update','vault_create',
	];

	let entries = $state<AuditEntry[]>([]);
	let loading = $state(true);
	let error = $state('');
	let filterOp = $state('all');
	let filterFrom = $state('');
	let filterTo = $state('');
	let filterSearch = $state('');
	let verifying = $state(false);
	let integrityResult = $state<IntegrityResult | null>(null);

	function buildQuery(): string {
		const p = new URLSearchParams({ limit: '100' });
		if (filterOp !== 'all') p.set('action', filterOp);
		if (filterFrom)         p.set('from', filterFrom);
		if (filterTo)           p.set('to', filterTo);
		if (filterSearch)       p.set('q', filterSearch);
		return `/v1/audit?${p}`;
	}

	async function load() {
		loading = true;
		error = '';
		integrityResult = null;
		try {
			const data = await api.getJson<{ entries: AuditEntry[] }>(buildQuery());
			entries = data.entries ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load audit log';
		} finally { loading = false; }
	}

	async function verifyIntegrity() {
		verifying = true;
		integrityResult = null;
		try {
			integrityResult = await api.getJson<IntegrityResult>('/v1/audit/verify');
		} catch (e) {
			integrityResult = { ok: false, checked: 0, message: e instanceof Error ? e.message : 'Verification failed' };
		} finally { verifying = false; }
	}

	onMount(load);

	function fmtDate(iso: string): string {
		return new Date(iso).toLocaleString(undefined, {
			month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit',
		});
	}

	function actionIcon(a: string): string {
		if (a.includes('sign')) return 'key';
		if (a.includes('derive')) return 'derivation';
		if (a.includes('withdraw')) return 'arrow-up-right';
		if (a.includes('deposit') || a.includes('address')) return 'arrow-down-left';
		if (a.includes('login')) return 'lock';
		if (a.includes('logout')) return 'logout';
		if (a.includes('policy')) return 'policies';
		if (a.includes('vault') || a.includes('wallet')) return 'wallets';
		if (a.includes('approve')) return 'check';
		if (a.includes('reject') || a.includes('deny')) return 'x';
		return 'file-text';
	}

	function actionTone(a: string): 'success' | 'danger' | 'muted' | 'info' {
		if (a.includes('reject') || a.includes('deny') || a.includes('revoke')) return 'danger';
		if (a.includes('approve') || a.includes('create')) return 'success';
		if (a.includes('sign') || a.includes('derive')) return 'info';
		return 'muted';
	}
</script>

<!-- ═══ Head ═══ -->
<div class="page-head">
	<div>
		<h1>{$locale, t('audit.title')}</h1>
		<p class="page-sub">{$locale, t('audit.subtitle')}</p>
	</div>
	<button class="btn btn-secondary" onclick={verifyIntegrity} disabled={verifying}>
		{#if verifying}
			<span class="spinner sm"></span>
			<span>{$locale, t('audit.verifying')}</span>
		{:else}
			<span>{@html icon('shield-check', 14)}</span>
			<span>{$locale, t('audit.verify')}</span>
		{/if}
	</button>
</div>

<!-- ═══ Integrity banner ═══ -->
{#if integrityResult}
	<div class="integrity-banner {integrityResult.ok ? 'ok' : 'fail'}">
		<span class="integrity-icon">
			{@html icon(integrityResult.ok ? 'shield-check' : 'alert-triangle', 20)}
		</span>
		<div class="integrity-body">
			{#if integrityResult.ok}
				<span class="integrity-title">{$locale, t('audit.verified')}</span>
				<span class="integrity-sub">
					<span class="mono">{integrityResult.checked.toLocaleString()}</span> entries checked — no tampering detected
				</span>
			{:else}
				<span class="integrity-title">{$locale, t('audit.failed')}</span>
				<span class="integrity-sub">
					{integrityResult.message}
					{#if integrityResult.first_broken_id}
						· first broken: <code class="mono">{integrityResult.first_broken_id}</code>
					{/if}
				</span>
			{/if}
		</div>
		<button class="integrity-close" onclick={() => integrityResult = null}>{@html icon('x', 14)}</button>
	</div>
{/if}

<!-- ═══ Filters ═══ -->
<section class="filter-bar">
	<div class="search-wrap">
		<span class="search-icon">{@html icon('search', 14)}</span>
		<input
			type="text"
			bind:value={filterSearch}
			placeholder={($locale, t('audit.search_hint'))}
			onkeydown={(e) => { if (e.key === 'Enter') load(); }}
		/>
	</div>

	<select class="filter-select" bind:value={filterOp} onchange={load}>
		{#each OPERATIONS as op}
			<option value={op}>{op === 'all' ? 'All operations' : op.replace(/_/g, ' ')}</option>
		{/each}
	</select>

	<div class="date-range">
		<input type="date" bind:value={filterFrom} onchange={load} aria-label="From" />
		<span class="date-sep">→</span>
		<input type="date" bind:value={filterTo} onchange={load} aria-label="To" />
	</div>

	<button class="btn btn-ghost" onclick={load}>
		<span>{@html icon('refresh', 14)}</span>
		<span>Refresh</span>
	</button>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
	</div>
{/if}

<!-- ═══ Timeline ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>{$locale, t('audit.stream')}</h2>
			<p class="panel-sub">{loading ? 'Loading…' : `${entries.length} event${entries.length === 1 ? '' : 's'}`}</p>
		</div>
		<span class="hash-chip mono">
			<span>{@html icon('git-branch', 12)}</span>
			<span>{$locale, t('audit.chain_label')}</span>
		</span>
	</header>

	{#if loading}
		<div class="panel-body">
			{#each Array(6) as _}<div class="skel-line"></div>{/each}
		</div>
	{:else if entries.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('audit', 22)}</span>
			<div>
				<p class="empty-title">{$locale, t('audit.no_events')}</p>
				<p class="muted">{$locale, t('audit.no_events_sub')}</p>
			</div>
		</div>
	{:else}
		<ul class="timeline">
			{#each entries as e}
				<li class="event">
					<span class="event-icon tone-{actionTone(e.action)}">{@html icon(actionIcon(e.action), 13)}</span>
					<div class="event-body">
						<div class="event-line">
							<span class="event-actor">{e.actor}</span>
							<span class="event-action mono">{e.action}</span>
							{#if e.resource}
								<span class="event-resource mono muted">{e.resource}</span>
							{/if}
						</div>
						<span class="event-meta">
							<span class="mono tiny muted">{e.id.slice(0, 12)}</span>
							<span class="tiny muted">·</span>
							<span class="tiny muted">{fmtDate(e.created_at)}</span>
						</span>
					</div>
					<span class="event-result {e.result === 'ok' || e.result === 'success' ? 'success' : 'danger'}">
						{e.result}
					</span>
				</li>
			{/each}
		</ul>
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

	/* Integrity banner */
	.integrity-banner {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-radius: var(--r-md);
		border: 1px solid transparent;
		margin-bottom: var(--sp-4);
	}
	.integrity-banner.ok {
		background: var(--green-soft);
		color: var(--green);
		border-color: rgba(52, 211, 153, 0.3);
	}
	.integrity-banner.fail {
		background: var(--red-soft);
		color: var(--red);
		border-color: rgba(248, 113, 113, 0.3);
	}
	.integrity-icon { display: inline-flex; flex-shrink: 0; }
	.integrity-body { flex: 1; display: flex; flex-direction: column; gap: 2px; }
	.integrity-title { font-weight: 600; font-size: 0.9rem; }
	.integrity-sub { font-size: 0.82rem; opacity: 0.9; }
	.integrity-sub code { font-size: 0.78rem; }
	.integrity-close {
		background: transparent;
		border: none;
		color: inherit;
		cursor: pointer;
		opacity: 0.6;
		padding: 4px;
	}
	.integrity-close:hover { opacity: 1; }

	/* Filter bar */
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
	}
	.search-wrap input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

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
		text-transform: capitalize;
	}

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
		color-scheme: dark;
	}
	:global(:root.light) .date-range input { color-scheme: light; }
	.date-sep { color: var(--fg-dim); font-size: 0.8rem; }

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
	}
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-secondary { background: var(--surface-2); }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.spinner {
		width: 13px; height: 13px;
		border: 2px solid var(--fg-muted);
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.spinner.sm { width: 11px; height: 11px; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* Alert */
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
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }

	.hash-chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 10px;
		font-size: 0.72rem;
		color: var(--fg-muted);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
	}
	.hash-chip span:first-child { color: var(--accent); display: inline-flex; }

	/* Timeline */
	.timeline {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.event {
		display: flex;
		align-items: center;
		gap: var(--sp-3);
		padding: var(--sp-3) var(--sp-5);
		border-bottom: 1px solid var(--border);
		transition: background 0.1s;
	}
	.event:last-child { border-bottom: none; }
	.event:hover { background: var(--hover-bg); }

	.event-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px; height: 26px;
		border-radius: var(--r-sm);
		flex-shrink: 0;
	}
	.event-icon.tone-success { background: var(--green-soft); color: var(--green); }
	.event-icon.tone-danger { background: var(--red-soft); color: var(--red); }
	.event-icon.tone-info { background: var(--accent-soft); color: var(--accent); }
	.event-icon.tone-muted { background: var(--surface-2); color: var(--fg-muted); }

	.event-body { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.event-line { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }

	.event-actor {
		font-size: 0.86rem;
		font-weight: 600;
		color: var(--fg-strong);
	}

	.event-action {
		font-size: 0.78rem;
		color: var(--fg);
		background: var(--surface-2);
		padding: 2px 7px;
		border-radius: var(--r-sm);
	}

	.event-resource {
		font-size: 0.78rem;
		color: var(--fg-muted);
	}

	.event-meta { display: inline-flex; align-items: center; gap: 6px; }
	.tiny { font-size: 0.7rem; }
	.muted { color: var(--fg-muted); }

	.event-result {
		font-size: 0.68rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 2px 8px;
		border-radius: var(--r-full);
		flex-shrink: 0;
	}
	.event-result.success { background: var(--green-soft); color: var(--green); }
	.event-result.danger { background: var(--red-soft); color: var(--red); }

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
		height: 44px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 6px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	@media (max-width: 800px) {
		.filter-bar { flex-direction: column; align-items: stretch; }
		.date-range { flex: 1; }
		.event { flex-wrap: wrap; }
		.event-result { margin-left: auto; }
	}
</style>
