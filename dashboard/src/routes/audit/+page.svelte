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

	let entries = $state<AuditEntry[]>([]);
	let loading = $state(true);
	let error = $state('');

	onMount(async () => {
		try {
			const data = await api.getJson<{ entries: AuditEntry[] }>('/v1/audit');
			entries = data.entries ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load audit log';
		} finally {
			loading = false;
		}
	});

	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Audit Log</h1>

<div class="card">
	<p class="notice">
		All entries are append-only and tamper-evident (SHA-256 hash chain).
	</p>

	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if entries.length === 0}
		<p class="muted">No audit entries yet.</p>
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
