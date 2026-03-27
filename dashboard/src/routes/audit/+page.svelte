<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

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

	const OPERATIONS = ['all', 'key_derive', 'sign', 'address_create', 'withdrawal_approve',
		'withdrawal_reject', 'login', 'logout', 'policy_update', 'vault_create'];

	let entries        = $state<AuditEntry[]>([]);
	let loading        = $state(true);
	let error          = $state('');
	let filterOp       = $state('all');
	let filterFrom     = $state('');
	let filterTo       = $state('');
	let verifying      = $state(false);
	let integrityResult = $state<IntegrityResult | null>(null);

	function buildQuery(): string {
		const p = new URLSearchParams({ limit: '50' });
		if (filterOp !== 'all') p.set('action', filterOp);
		if (filterFrom)         p.set('from',   filterFrom);
		if (filterTo)           p.set('to',     filterTo);
		return `/v1/audit?${p}`;
	}

	async function load() {
		loading = true; error = ''; integrityResult = null;
		try {
			const data = await api.getJson<{ entries: AuditEntry[] }>(buildQuery());
			entries = data.entries ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load audit log';
		} finally { loading = false; }
	}

	async function verifyIntegrity() {
		verifying = true; integrityResult = null;
		try {
			const r = await api.getJson<IntegrityResult>('/v1/audit/verify');
			integrityResult = r;
		} catch (e) {
			integrityResult = { ok: false, checked: 0, message: e instanceof Error ? e.message : 'Verification failed' };
		} finally { verifying = false; }
	}

	onMount(load);

	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Audit Log</h1>

<div class="filters card">
	<label>Operation
		<select bind:value={filterOp}>
			{#each OPERATIONS as op}<option value={op}>{op}</option>{/each}
		</select>
	</label>
	<label>From <input type="date" bind:value={filterFrom} /></label>
	<label>To   <input type="date" bind:value={filterTo}   /></label>
	<button class="btn" onclick={load}>Apply</button>
	<button class="btn-ghost" onclick={verifyIntegrity} disabled={verifying}>
		{verifying ? 'Verifying…' : 'Verify Chain Integrity'}
	</button>
</div>

{#if integrityResult}
	<div class="integrity {integrityResult.ok ? 'ok' : 'fail'}">
		{#if integrityResult.ok}
			Hash chain intact — {integrityResult.checked} entries verified.
		{:else}
			Integrity failure: {integrityResult.message}
			{#if integrityResult.first_broken_id}(first broken: {integrityResult.first_broken_id}){/if}
		{/if}
	</div>
{/if}

<div class="card">
	<p class="notice">All entries are append-only and tamper-evident (SHA-256 hash chain).</p>

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if entries.length === 0}
		<p class="muted">No audit entries match the selected filters.</p>
	{:else}
		<table>
			<thead>
				<tr><th>Time</th><th>Actor</th><th>Action</th><th>Resource</th><th>Result</th></tr>
			</thead>
			<tbody>
				{#each entries as e}
					<tr>
						<td class="mono muted">{fmt(e.created_at)}</td>
						<td>{e.actor}</td>
						<td>{e.action}</td>
						<td class="muted">{e.resource}</td>
						<td class={e.result === 'ok' ? 'green' : 'red'}>{e.result}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.filters {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: flex-end;
		margin-bottom: 1rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.78rem;
		color: var(--fg-muted, #8b949e);
	}

	select, input[type="date"] {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg, #e1e4e8);
		padding: 0.35rem 0.6rem;
		font-size: 0.85rem;
	}

	.btn {
		background: var(--accent, #58a6ff);
		color: #0f1117;
		border: none;
		border-radius: 6px;
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
		align-self: flex-end;
	}

	.btn-ghost {
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg-muted, #8b949e);
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		cursor: pointer;
		align-self: flex-end;
	}
	.btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }

	.integrity {
		padding: 0.65rem 1rem;
		border-radius: 6px;
		font-size: 0.85rem;
		margin-bottom: 1rem;
	}
	.integrity.ok   { background: rgba(63,185,80,.12);  color: var(--green, #3fb950);  border: 1px solid var(--green, #3fb950); }
	.integrity.fail { background: rgba(248,81,73,.12);  color: var(--red, #f85149);    border: 1px solid var(--red, #f85149); }

	.notice {
		font-size: 0.82rem;
		color: var(--fg-muted, #8b949e);
		border-left: 2px solid var(--accent, #58a6ff);
		padding-left: 0.75rem;
		margin-bottom: 1rem;
	}

	table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.4rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.mono  { font-family: monospace; }
	.muted { color: var(--fg-muted, #8b949e); }
	.green { color: var(--green, #3fb950); }
	.red   { color: var(--red, #f85149); }
	.err   { color: var(--red, #f85149); }
</style>
